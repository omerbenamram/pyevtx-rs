#![allow(clippy::new_ret_no_self)]
#![deny(unused_must_use)]
#![cfg_attr(not(debug_assertions), deny(clippy::dbg_macro))]

use evtx_rs::{
    err,
    err::{ChunkError, DeserializationError, EvtxError, InputError, SerializationError},
    EvtxParser, IntoIterChunks, ParserSettings, SerializedEvtxRecord,
};

use pyo3::types::PyDict;
use pyo3::types::PyString;

use pyo3::{
    exceptions::PyFileNotFoundError, exceptions::PyNotImplementedError, exceptions::PyOSError,
    exceptions::PyRuntimeError, exceptions::PyValueError, prelude::*,
};

use encoding::all::encodings;
use pyo3_file::PyFileLikeObject;

use std::error::Error;
use std::fs::File;
use std::io;
use std::io::{Read, Seek};
use std::sync::Arc;
use std::vec::IntoIter;

pub trait ReadSeek: Read + Seek + Send + Sync + 'static {
    fn tell(&mut self) -> io::Result<u64> {
        self.stream_position()
    }
}

impl<T: Read + Seek + Send + Sync + 'static> ReadSeek for T {}

struct PyEvtxError(EvtxError);

fn py_err_from_io_err(e: &io::Error) -> PyErr {
    match e.kind() {
        io::ErrorKind::NotFound => PyErr::new::<PyFileNotFoundError, _>(format!("{}", e)),
        _ => PyErr::new::<PyOSError, _>(format!("{}", e)),
    }
}

impl From<PyEvtxError> for PyErr {
    fn from(err: PyEvtxError) -> Self {
        match err.0 {
            err::EvtxError::FailedToParseChunk {
                chunk_id: _,
                source,
            } => match source {
                ChunkError::FailedToSeekToChunk(io) => py_err_from_io_err(&io),
                _ => PyErr::new::<PyRuntimeError, _>(format!("{}", source)),
            },
            EvtxError::InputError(e) => match e {
                InputError::FailedToOpenFile {
                    source: inner,
                    path: _,
                } => py_err_from_io_err(&inner),
            },
            EvtxError::SerializationError(e) => match e {
                SerializationError::Unimplemented { .. } => {
                    PyErr::new::<PyNotImplementedError, _>(format!("{}", e))
                }
                _ => PyErr::new::<PyRuntimeError, _>(format!("{}", e)),
            },
            EvtxError::DeserializationError(e) => match e {
                DeserializationError::UnexpectedIoError(ref io) => match io.source() {
                    Some(inner_io_err) => match inner_io_err.downcast_ref::<io::Error>() {
                        Some(actual_inner_io_err) => py_err_from_io_err(actual_inner_io_err),
                        None => PyErr::new::<PyRuntimeError, _>(format!("{}", e)),
                    },
                    None => PyErr::new::<PyRuntimeError, _>(format!("{}", e)),
                },
                _ => PyErr::new::<PyRuntimeError, _>(format!("{}", e)),
            },
            EvtxError::Unimplemented { .. } => {
                PyErr::new::<PyNotImplementedError, _>(format!("{}", err.0))
            }
            _ => PyErr::new::<PyRuntimeError, _>(format!("{}", err.0)),
        }
    }
}

#[derive(Copy, Clone, PartialOrd, PartialEq, Eq)]
pub enum OutputFormat {
    JSON,
    XML,
}

#[derive(Debug)]
enum FileOrFileLike {
    File(String),
    FileLike(PyFileLikeObject),
}

impl FileOrFileLike {
    pub fn from_pyobject(path_or_file_like: PyObject) -> PyResult<FileOrFileLike> {
        Python::with_gil(|py| {
            if let Ok(string_ref) = path_or_file_like.downcast_bound::<PyString>(py) {
                return Ok(FileOrFileLike::File(
                    string_ref.to_string_lossy().to_string(),
                ));
            }

            // We only need read + seek
            match PyFileLikeObject::with_requirements(path_or_file_like, true, false, true, true) {
                Ok(f) => Ok(FileOrFileLike::FileLike(f)),
                Err(e) => Err(e),
            }
        })
    }
}

#[pyclass]
/// PyEvtxParser(self, path_or_file_like, number_of_threads=0, ansi_codec='windows-1252', /)
/// --
///
/// Returns an instance of the parser.
///
/// Args:
///     `path_or_file_like`: a path (string), or a file-like object.
///
///     `number_of_threads` (int, optional):
///            limit the number of worker threads used by rust.
///            `0` (the default) will let the library decide how many threads to use
///            based on the number of cores available.
///
///     `ansi_codec`(str, optional) to control encoding of ansi strings inside the evtx file.
///
///                  Possible values:
///                      ascii, ibm866, iso-8859-1, iso-8859-2, iso-8859-3, iso-8859-4,
///                      iso-8859-5, iso-8859-6, iso-8859-7, iso-8859-8, iso-8859-10,
///                      iso-8859-13, iso-8859-14, iso-8859-15, iso-8859-16,
///                      koi8-r, koi8-u, mac-roman, windows-874, windows-1250, windows-1251,
///                      windows-1252, windows-1253, windows-1254, windows-1255,
///                      windows-1256, windows-1257, windows-1258, mac-cyrillic, utf-8,
///                      windows-949, euc-jp, windows-31j, gbk, gb18030, hz, big5-2003,
///                      pua-mapped-binary, iso-8859-8-i
///
pub struct PyEvtxParser {
    inner: Option<EvtxParser<Box<dyn ReadSeek>>>,
    configuration: ParserSettings,
}

#[pymethods]
impl PyEvtxParser {
    #[new]
    #[pyo3(signature = (path_or_file_like, number_of_threads=None, ansi_codec=None))]
    fn new(
        path_or_file_like: PyObject,
        number_of_threads: Option<usize>,
        ansi_codec: Option<String>,
    ) -> PyResult<Self> {
        let file_or_file_like = FileOrFileLike::from_pyobject(path_or_file_like)?;

        // Setup `ansi_codec`
        let codec = if let Some(codec) = ansi_codec {
            match encodings().iter().find(|c| c.name() == codec) {
                Some(encoding) => *encoding,
                None => {
                    return Err(PyErr::new::<PyValueError, _>(format!(
                        "Unknown encoding `[{}]`, see help for possible values",
                        codec
                    )));
                }
            }
        } else {
            ParserSettings::default().get_ansi_codec()
        };

        // Setup `number_of_threads`
        let number_of_threads = match number_of_threads {
            Some(number) => number,
            None => *ParserSettings::default().get_num_threads(),
        };

        let configuration = ParserSettings::new()
            .ansi_codec(codec)
            .num_threads(number_of_threads);

        let boxed_read_seek = match file_or_file_like {
            FileOrFileLike::File(s) => {
                let file = File::open(s)?;
                Box::new(file) as Box<dyn ReadSeek>
            }
            FileOrFileLike::FileLike(f) => Box::new(f) as Box<dyn ReadSeek>,
        };

        let parser = EvtxParser::from_read_seek(boxed_read_seek)
            .map_err(PyEvtxError)?
            .with_configuration(configuration.clone());

        Ok(PyEvtxParser {
            inner: Some(parser),
            configuration,
        })
    }

    /// records(self, /)
    /// --
    ///
    /// Returns an iterator that yields either an XML record, or a `RuntimeError` object.
    ///
    /// Note - Iterating over records can raise a `RuntimeError` if the parser encounters an invalid record.
    ///        If using a regular for-loop, this could abruptly terminate the iteration.
    ///
    ///        It is recommended to wrap this iterator with a logic that will continue iteration
    ///        in case an exception object is returned.
    fn records(&mut self) -> PyResult<PyRecordsIterator> {
        self.records_iterator(OutputFormat::XML)
    }

    /// records_json(self, /)
    /// --
    ///
    /// Returns an iterator that yields either a JSON record, or a `RuntimeError` object.
    ///
    /// Note - Iterating over records can raise a `RuntimeError` if the parser encounters an invalid record.
    ///        If using a regular for-loop, this could abruptly terminate the iteration.
    ///
    ///        It is recommended to wrap this iterator with a logic that will continue iteration
    ///        in case an exception object is returned.
    fn records_json(&mut self) -> PyResult<PyRecordsIterator> {
        self.records_iterator(OutputFormat::JSON)
    }

    fn __iter__(mut slf: PyRefMut<Self>) -> PyResult<PyRecordsIterator> {
        slf.records()
    }
    fn __next__(_slf: PyRefMut<Self>) -> PyResult<Option<PyObject>> {
        Err(PyErr::new::<PyNotImplementedError, _>("Using `next()` over `PyEvtxParser` is not supported. Try iterating over `PyEvtxParser(...).records()`"))
    }
}

impl PyEvtxParser {
    fn records_iterator(&mut self, output_format: OutputFormat) -> PyResult<PyRecordsIterator> {
        let inner = match self.inner.take() {
            Some(inner) => inner,
            None => {
                return Err(PyErr::new::<PyRuntimeError, _>(
                    "PyEvtxParser can only be used once",
                ));
            }
        };

        Ok(PyRecordsIterator {
            inner: inner.into_chunks(),
            records_iter: Vec::new().into_iter(),
            settings: Arc::new(self.configuration.clone()),
            output_format,
        })
    }
}

fn record_to_pydict(record: SerializedEvtxRecord<String>, py: Python) -> PyResult<Bound<'_, PyDict>> {
    let pyrecord = PyDict::new(py);

    pyrecord.set_item("event_record_id", record.event_record_id)?;
    pyrecord.set_item("timestamp", format!("{}", record.timestamp))?;
    pyrecord.set_item("data", record.data)?;
    Ok(pyrecord)
}

fn record_to_pyobject(
    r: Result<SerializedEvtxRecord<String>, EvtxError>,
    py: Python,
) -> PyResult<PyObject> {
    match r {
        Ok(r) => match record_to_pydict(r, py) {
            Ok(dict) => Ok(dict.into_pyobject(py)?.into()),
            Err(e) => Ok(e.into_pyobject(py)?.into()),
        },
        Err(e) => Err(PyEvtxError(e).into()),
    }
}

#[pyclass]
pub struct PyRecordsIterator {
    inner: IntoIterChunks<Box<dyn ReadSeek>>,
    records_iter: IntoIter<Result<SerializedEvtxRecord<String>, EvtxError>>,
    settings: Arc<ParserSettings>,
    output_format: OutputFormat,
}

impl PyRecordsIterator {
    fn next(&mut self) -> PyResult<Option<PyObject>> {
        let mut chunk_id = 0;

        loop {
            if let Some(record) = self.records_iter.next() {
                let record = Python::with_gil(|py| record_to_pyobject(record, py).map(Some));

                return record;
            }

            let chunk = self.inner.next();
            chunk_id += 1;

            match chunk {
                None => return Ok(None),
                Some(chunk_result) => match chunk_result {
                    Err(e) => {
                        return Err(PyEvtxError(e).into());
                    }
                    Ok(mut chunk) => {
                        let parsed_chunk = chunk.parse(self.settings.clone());

                        match parsed_chunk {
                            Err(e) => {
                                return Err(PyEvtxError(EvtxError::FailedToParseChunk {
                                    chunk_id,
                                    source: e,
                                })
                                .into());
                            }
                            Ok(mut chunk) => {
                                let records: Vec<_> = match self.output_format {
                                    OutputFormat::XML => chunk
                                        .iter()
                                        .filter_map(|r| r.ok())
                                        .map(|r| r.into_xml())
                                        .collect(),
                                    OutputFormat::JSON => chunk
                                        .iter()
                                        .filter_map(|r| r.ok())
                                        .map(|r| r.into_json())
                                        .collect(),
                                };

                                self.records_iter = records.into_iter();
                            }
                        }
                    }
                },
            }
        }
    }
}

#[pymethods]
impl PyRecordsIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> PyResult<Option<PyObject>> {
        slf.next()
    }
}

// Don't use double quotes ("") inside this docstring, this will crash pyo3.
/// Parses an evtx file.
///
/// This will print each record as an XML string.
///
///```python
/// from evtx import PyEvtxParser
///
/// def main():
///    parser = PyEvtxParser('./samples/Security_short_selected.evtx')
///    for record in parser.records():
///        print(f'Event Record ID: {record['event_record_id']}')
///        print(f'Event Timestamp: {record['timestamp']}')
///        print(record['data'])
///        print('------------------------------------------')
///```
///
/// And this will print each record as a JSON string.
///
/// ```python
/// from evtx import PyEvtxParser
///
/// def main():
///    parser = PyEvtxParser('./samples/Security_short_selected.evtx')
///    for record in parser.records_json():
///        print(f'Event Record ID: {record['event_record_id']}')
///        print(f'Event Timestamp: {record['timestamp']}')
///        print(record['data'])
///        print(f'------------------------------------------')
///```
#[pymodule]
fn evtx(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyEvtxParser>()?;
    m.add_class::<PyRecordsIterator>()?;

    Ok(())
}
