use evtx::{EvtxParser, EvtxRecord, SerializedEvtxRecord, EvtxChunkData};
use pyo3::exceptions::RuntimeError;
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyDict};
use pyo3::PyIterProtocol;
use std::collections::HashMap;
use std::panic;
use std::fs::File;
use evtx::{XmlOutput, IntoIterChunks, ParserSettings};

#[pyclass]
pub struct PyEvtxParser {
    inner: IntoIterChunks<File>,
    records: Option<Vec<Result<SerializedEvtxRecord, failure::Error>>>,
    current_chunk_number: u16,
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
                current_chunk_number: 0,
                settings: ParserSettings::new(),
            }
        });

        Ok(())
    }
}

impl PyEvtxParser {
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
            Err(e) => PyErr::new::<RuntimeError, _>(format!("{}", e)).to_object(py),
        }
    }

    fn next(&mut self) -> Option<PyObject> {
        loop {
            if let Some(record) = self.records.as_mut().and_then(|records| records.pop()) {
                return Some(PyEvtxParser::record_to_pyobject(record));
            }

            let chunk = self.inner.next();

            match chunk {
                None => return None,
                Some(mut chunk_result) => {
                    match chunk_result {
                        Err(err) => {
                            return Some(PyEvtxParser::record_to_pyobject(Err(err)));
                        }
                        Ok(mut chunk) => {
                            let parsed_chunk = chunk.parse(&self.settings);

                            match parsed_chunk {
                                Err(err) => {
                                    return Some(PyEvtxParser::record_to_pyobject(Err(err)));
                                }
                                Ok(mut chunk) => {
                                    self.records = Some(chunk.iter_serialized_records::<XmlOutput<Vec<u8>>>().collect());
                                }
                            }
                        }
                    }
                }
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
fn evtx_parser(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyEvtxParser>()?;

    Ok(())
}
