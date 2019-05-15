#![allow(clippy::new_ret_no_self)]

use evtx::{EvtxParser, SerializedEvtxRecord, JsonOutput};
use evtx::{IntoIterChunks, ParserSettings, XmlOutput};
use pyo3::exceptions::RuntimeError;
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
            .map_err(|e| PyErr::new::<RuntimeError, _>(format!("{}", e)))?;

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
            output_format: output_format,
        })
    }
}


fn err_to_object(e: failure::Error, py: Python) -> PyObject {
    PyErr::new::<RuntimeError, _>(format!("{}", e)).to_object(py)
}

fn record_to_pydict(gil: Python, record: SerializedEvtxRecord) -> PyResult<&PyDict> {
    let pyrecord = PyDict::new(gil);

    pyrecord.set_item("event_record_id", record.event_record_id)?;
    pyrecord.set_item("timestamp", format!("{}", record.timestamp))?;
    pyrecord.set_item("data", record.data)?;
    Ok(pyrecord)
}

fn record_to_pyobject(r: Result<SerializedEvtxRecord, failure::Error>) -> PyObject {
    let gil = Python::acquire_gil();
    let py = gil.python();

    match r {
        Ok(r) => match record_to_pydict(py, r) {
            Ok(dict) => dict.to_object(py),
            Err(e) => e.to_object(py),
        },
        Err(e) => err_to_object(e, py),
    }
}


#[pyclass]
pub struct PyRecordsIterator {
    inner: IntoIterChunks<File>,
    records: Option<Vec<Result<SerializedEvtxRecord, failure::Error>>>,
    settings: ParserSettings,
    output_format: OutputFormat,
}

impl PyRecordsIterator {
    fn next(&mut self) -> Option<PyObject> {
        let gil = Python::acquire_gil();
        let py = gil.python();

        loop {
            if let Some(record) = self.records.as_mut().and_then(Vec::pop) {
                return Some(record_to_pyobject(record));
            }

            let chunk = self.inner.next();

            match chunk {
                None => return None,
                Some(chunk_result) => match chunk_result {
                    Err(e) => {
                        return Some(err_to_object(e, py));
                    }
                    Ok(mut chunk) => {
                        let parsed_chunk = chunk.parse(&self.settings);

                        match parsed_chunk {
                            Err(e) => {
                                return Some(err_to_object(e, py));
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
    fn __next__(mut slf: PyRefMut<Self>) -> PyResult<Option<PyObject>> {
        unimplemented!()
    }
}


#[pyproto]
impl PyIterProtocol for PyRecordsIterator {
    fn __iter__(slf: PyRefMut<Self>) -> PyResult<Py<PyRecordsIterator>> {
        Ok(slf.into())
    }
    fn __next__(mut slf: PyRefMut<Self>) -> PyResult<Option<PyObject>> {
        Ok(slf.next())
    }
}

#[pymodule]
fn evtx(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyEvtxParser>()?;
    m.add_class::<PyRecordsIterator>()?;

    Ok(())
}
