use evtx::{EvtxParser, EvtxRecord};
use pyo3::exceptions::RuntimeError;
use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyDict};
use pyo3::PyIterProtocol;
use std::collections::HashMap;

#[pyclass]
pub struct PyEvtxParser {
    iter: Box<Iterator<Item = PyObject> + Send>,
}

#[pymethods]
impl PyEvtxParser {
    #[new]
    fn new(obj: &PyRawObject, file: String) -> PyResult<()> {
        let inner = EvtxParser::from_path(file)
            .map_err(|e| PyErr::new::<RuntimeError, _>(format!("{}", e)))?;

        obj.init({
            PyEvtxParser {
                iter: Box::new(inner.records().into_iter().map(Self::record_to_pyobject)),
            }
        });

        Ok(())
    }
}

impl PyEvtxParser {
    fn record_to_pydict(gil: Python, record: EvtxRecord) -> PyResult<&PyDict> {
        let pyrecord = PyDict::new(gil);

        pyrecord.set_item("event_record_id", record.event_record_id)?;
        pyrecord.set_item("timestamp", format!("{}", record.timestamp))?;
        pyrecord.set_item("data", record.data)?;
        Ok(pyrecord)
    }
    fn record_to_pyobject(r: Result<EvtxRecord, failure::Error>) -> PyObject {
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
}

#[pyproto]
impl PyIterProtocol for PyEvtxParser {
    fn __iter__(slf: PyRefMut<Self>) -> PyResult<Py<PyEvtxParser>> {
        Ok(slf.into())
    }
    fn __next__(mut slf: PyRefMut<Self>) -> PyResult<Option<PyObject>> {
        Ok(slf.iter.next())
    }
}

#[pymodule]
fn evtx_parser(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyEvtxParser>()
}
