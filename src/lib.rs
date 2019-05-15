#![allow(clippy::new_ret_no_self)]

use evtx::{EvtxParser, SerializedEvtxRecord, JsonOutput, IntoIterChunks, ParserSettings, XmlOutput};
use pyo3::exceptions::{RuntimeError, NotImplementedError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::PyIterProtocol;

use std::fs::File;

#[derive(Copy, Clone, PartialOrd, PartialEq)]
pub enum OutputFormat {
    JSON,
    XML,
}

#[pyclass]
pub struct PyEvtxParser {
    inner: Option<EvtxParser<File>>
}

#[pymethods]
impl PyEvtxParser {
    #[new]
    fn new(obj: &PyRawObject, file: String) -> PyResult<()> {
        let inner = EvtxParser::from_path(file)
            .map_err(evtx_err_to_pyerr)?;

        obj.init({
            PyEvtxParser {
                inner: Some(inner)
            }
        });

        Ok(())
    }

    fn records(&mut self) -> PyResult<PyRecordsIterator> {
        self.records_iterator(OutputFormat::XML)
    }

    fn records_json(&mut self) -> PyResult<PyRecordsIterator> {
        self.records_iterator(OutputFormat::JSON)
    }
}

impl PyEvtxParser {
    fn records_iterator(&mut self, output_format: OutputFormat) -> PyResult<PyRecordsIterator> {
        let inner = match self.inner.take() {
            Some(inner) => inner,
            None => {
                return Err(PyErr::new::<RuntimeError, _>("PyEvtxParser can only be used once"));
            }
        };

        Ok(PyRecordsIterator {
            inner: inner.into_chunks(),
            records: None,
            settings: ParserSettings::new(),
            output_format,
        })
    }
}

fn evtx_err_to_pyerr(e: evtx::err::Error) -> PyErr {
    match e {
        evtx::err::Error::IO { source, backtrace: _ } => {
            source.into()
        }
        _ => {
            PyErr::new::<RuntimeError, _>(format!("{}", e))
        }
    }
}

fn record_to_pydict(gil: Python, record: SerializedEvtxRecord) -> PyResult<&PyDict> {
    let pyrecord = PyDict::new(gil);

    pyrecord.set_item("event_record_id", record.event_record_id)?;
    pyrecord.set_item("timestamp", format!("{}", record.timestamp))?;
    pyrecord.set_item("data", record.data)?;
    Ok(pyrecord)
}

fn record_to_pyobject(r: Result<SerializedEvtxRecord, evtx::err::Error>, py: Python) -> PyResult<PyObject> {
    match r {
        Ok(r) => match record_to_pydict(py, r) {
            Ok(dict) => Ok(dict.to_object(py)),
            Err(e) => Ok(e.to_object(py)),
        },
        Err(e) => Err(evtx_err_to_pyerr(e)),
    }
}


#[pyclass]
pub struct PyRecordsIterator {
    inner: IntoIterChunks<File>,
    records: Option<Vec<Result<SerializedEvtxRecord, evtx::err::Error>>>,
    settings: ParserSettings,
    output_format: OutputFormat,
}

impl PyRecordsIterator {
    fn next(&mut self) -> PyResult<Option<PyObject>> {
        let gil = Python::acquire_gil();
        let py = gil.python();

        loop {
            if let Some(record) = self.records.as_mut().and_then(Vec::pop) {
                return record_to_pyobject(record, py).map(Some);
            }

            let chunk = self.inner.next();

            match chunk {
                None => return Ok(None),
                Some(chunk_result) => match chunk_result {
                    Err(e) => {
                        return Err(evtx_err_to_pyerr(e));
                    }
                    Ok(mut chunk) => {
                        let parsed_chunk = chunk.parse(&self.settings);

                        match parsed_chunk {
                            Err(e) => {
                                return Err(evtx_err_to_pyerr(e));
                            }
                            Ok(mut chunk) => {
                                self.records = match self.output_format {
                                    OutputFormat::XML => {
                                        Some(chunk
                                            .iter_serialized_records::<XmlOutput<Vec<u8>>>()
                                            .collect())
                                    }
                                    OutputFormat::JSON => {
                                        Some(chunk
                                            .iter_serialized_records::<JsonOutput<Vec<u8>>>()
                                            .collect())
                                    }
                                };
                            }
                        }
                    }
                },
            }
        }
    }
}


#[pyproto]
impl PyIterProtocol for PyEvtxParser {
    fn __iter__(mut slf: PyRefMut<Self>) -> PyResult<PyRecordsIterator> {
        slf.records()
    }
    fn __next__(_slf: PyRefMut<Self>) -> PyResult<Option<PyObject>> {
        Err(PyErr::new::<NotImplementedError, _>(""))
    }
}


#[pyproto]
impl PyIterProtocol for PyRecordsIterator {
    fn __iter__(slf: PyRefMut<Self>) -> PyResult<Py<PyRecordsIterator>> {
        Ok(slf.into())
    }
    fn __next__(mut slf: PyRefMut<Self>) -> PyResult<Option<PyObject>> {
        slf.next()
    }
}

#[pymodule]
fn evtx(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyEvtxParser>()?;
    m.add_class::<PyRecordsIterator>()?;

    Ok(())
}
