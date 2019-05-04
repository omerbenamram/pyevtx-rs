#![allow(clippy::new_ret_no_self)]

use evtx::{EvtxParser, SerializedEvtxRecord};
use evtx::{IntoIterChunks, ParserSettings, XmlOutput};
use pyo3::exceptions::RuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::PyIterProtocol;

use std::fs::File;

#[pyclass]
pub struct PyEvtxParser {
    inner: IntoIterChunks<File>,
    records: Option<Vec<Result<SerializedEvtxRecord, failure::Error>>>,
    settings: ParserSettings,
}

#[pymethods]
impl PyEvtxParser {
    #[new]
    fn new(obj: &PyRawObject, file: String) -> PyResult<()> {
        let inner = EvtxParser::from_path(file)
            .map_err(|e| PyErr::new::<RuntimeError, _>(format!("{}", e)))?;

        obj.init({
            PyEvtxParser {
                inner: inner.into_chunks(),
                records: None,
                settings: ParserSettings::new(),
            }
        });

        Ok(())
    }
}

impl PyEvtxParser {
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
            Ok(r) => match Self::record_to_pydict(py, r) {
                Ok(dict) => dict.to_object(py),
                Err(e) => e.to_object(py),
            },
            Err(e) => PyEvtxParser::err_to_object(e, py),
        }
    }

    fn next(&mut self) -> Option<PyObject> {
        let gil = Python::acquire_gil();
        let py = gil.python();

        loop {
            if let Some(record) = self.records.as_mut().and_then(Vec::pop) {
                return Some(PyEvtxParser::record_to_pyobject(record));
            }

            let chunk = self.inner.next();

            match chunk {
                None => return None,
                Some(chunk_result) => match chunk_result {
                    Err(e) => {
                        return Some(PyEvtxParser::err_to_object(e, py));
                    }
                    Ok(mut chunk) => {
                        let parsed_chunk = chunk.parse(&self.settings);

                        match parsed_chunk {
                            Err(e) => {
                                return Some(PyEvtxParser::err_to_object(e, py));
                            }
                            Ok(mut chunk) => {
                                self.records = Some(
                                    chunk
                                        .iter_serialized_records::<XmlOutput<Vec<u8>>>()
                                        .collect(),
                                );
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
    fn __iter__(slf: PyRefMut<Self>) -> PyResult<Py<PyEvtxParser>> {
        Ok(slf.into())
    }
    fn __next__(mut slf: PyRefMut<Self>) -> PyResult<Option<PyObject>> {
        Ok(slf.next())
    }
}

#[pymodule]
fn evtx(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyEvtxParser>()?;

    Ok(())
}
