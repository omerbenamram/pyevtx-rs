use evtx::{EvtxParser, EvtxRecord, SerializedEvtxRecord, EvtxChunkData};
use pyo3::exceptions::RuntimeError;
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyDict};
use pyo3::PyIterProtocol;
use std::collections::HashMap;
use std::panic;
use std::fs::File;
use std::sync::{Arc, Mutex};
use evtx::xml_output::XmlOutput;

#[pyclass]
pub struct PyEvtxParser {
    inner: EvtxParser<File>,
    records: Option<Vec<Result<SerializedEvtxRecord, failure::Error>>>,
    current_chunk_number: u16,
}

#[pymethods]
impl PyEvtxParser {
    #[new]
    fn new(obj: &PyRawObject, file: String) -> PyResult<()> {
        let inner = EvtxParser::from_path(file)
            .map_err(|e| PyErr::new::<RuntimeError, _>(format!("{}", e)))?;

        obj.init({
            PyEvtxParser {
                inner: inner,
                records: None,
                current_chunk_number: 0,
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
        let parser = &mut self.inner;
        let validate_checksum = parser.config.validate_checksums;
        let chunk_count = parser.header.chunk_count;

        loop {
            if let Some(record) = self.records.as_mut().and_then(|records| records.pop()) {
                return Some(PyEvtxParser::record_to_pyobject(record));
            }

            match EvtxParser::allocate_chunk(
                &mut parser.data,
                self.current_chunk_number,
                validate_checksum,
            ) {
                Err(err) => {
                    // We try to read past the `chunk_count` to allow for dirty files.
                    // But if we failed, it means we really are at the end of the file.
                    if self.current_chunk_number >= chunk_count {
                        return None;
                    } else {
                        self.current_chunk_number += 1;
                        return Some(PyEvtxParser::record_to_pyobject(Err(err)));
                    }
                }
                Ok(None) => {
                    // We try to read past the `chunk_count` to allow for dirty files.
                    // But if we get an empty chunk, we need to keep looking.
                    // Increment and try again.
                    self.current_chunk_number += 1;
                }
                Ok(Some(mut chunk)) => {
                    self.current_chunk_number += 1;

                    match chunk.parse(&parser.config) {
                        Err(err) => {
                            return Some(PyEvtxParser::record_to_pyobject(Err(err)));
                        },
                        Ok(mut chunk) => {
                            self.records = Some(chunk.iter_serialized_records::<XmlOutput<Vec<u8>>>().collect());
                        }
                    }
                }
            };
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
