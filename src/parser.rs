use std::fs::File;
use std::sync::Arc;

use encoding::all::encodings;
use pyo3::prelude::*;
use pyo3::{exceptions::PyNotImplementedError, exceptions::PyRuntimeError, exceptions::PyValueError};
use pyo3_stub_gen::derive::*;

use evtx_rs::{EvtxParser, ParserSettings};

use crate::file_like::{FileOrFileLike, ReadSeek};
use crate::py_err::PyEvtxError;
use crate::records::{OutputFormat, PyRecordsIterator};

#[gen_stub_pyclass]
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
pub struct PyEvtxParser {
    pub(crate) inner: Option<EvtxParser<Box<dyn ReadSeek>>>,
    pub(crate) configuration: ParserSettings,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyEvtxParser {
    #[new]
    #[pyo3(signature = (
        path_or_file_like,
        number_of_threads=None,
        ansi_codec=None,
        validate_checksums=None,
        separate_json_attributes=None,
        indent=None,
        wevt_cache=None
    ))]
    fn new(
        path_or_file_like: Py<PyAny>,
        number_of_threads: Option<usize>,
        ansi_codec: Option<String>,
        validate_checksums: Option<bool>,
        separate_json_attributes: Option<bool>,
        indent: Option<bool>,
        wevt_cache: Option<Py<PyAny>>,
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

        let mut configuration = ParserSettings::new()
            .ansi_codec(codec)
            .num_threads(number_of_threads);

        if let Some(validate_checksums) = validate_checksums {
            configuration = configuration.validate_checksums(validate_checksums);
        }
        if let Some(separate_json_attributes) = separate_json_attributes {
            configuration = configuration.separate_json_attributes(separate_json_attributes);
        }
        if let Some(indent) = indent {
            configuration = configuration.indent(indent);
        }

        if let Some(wevt_cache) = wevt_cache {
            #[cfg(feature = "wevt_templates")]
            {
                let cache = crate::wevt_cache::wevt_cache_from_pyobject(wevt_cache)?;
                configuration = configuration.wevt_cache(Some(cache));
            }

            #[cfg(not(feature = "wevt_templates"))]
            {
                let _ = wevt_cache;
                return Err(PyErr::new::<PyNotImplementedError, _>(
                    "WEVT support is not enabled in this build (compile with `wevt_templates`)",
                ));
            }
        }

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

    fn __next__(_slf: PyRefMut<Self>) -> PyResult<Option<Py<PyAny>>> {
        Err(PyErr::new::<PyNotImplementedError, _>(
            "Using `next()` over `PyEvtxParser` is not supported. Try iterating over `PyEvtxParser(...).records()`",
        ))
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

