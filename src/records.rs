use std::sync::Arc;
use std::vec::IntoIter;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::*;

use evtx_rs::err::EvtxError;
use evtx_rs::{IntoIterChunks, ParserSettings, SerializedEvtxRecord};

use crate::file_like::ReadSeek;
use crate::py_err::{error_message, PyEvtxError};

#[derive(Copy, Clone, PartialOrd, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    JSON,
    XML,
}

fn timestamp_to_py_string(timestamp: &impl std::fmt::Display) -> String {
    let ts = timestamp.to_string();
    if ts.ends_with("UTC") {
        ts
    } else {
        format!("{ts} UTC")
    }
}

fn record_to_pydict(
    record: SerializedEvtxRecord<String>,
    py: Python<'_>,
) -> PyResult<Bound<'_, PyDict>> {
    let pyrecord = PyDict::new(py);

    pyrecord.set_item("event_record_id", record.event_record_id)?;
    pyrecord.set_item("timestamp", timestamp_to_py_string(&record.timestamp))?;
    pyrecord.set_item("data", record.data)?;
    Ok(pyrecord)
}

fn record_to_pyobject(
    r: Result<SerializedEvtxRecord<String>, EvtxError>,
    py: Python<'_>,
) -> PyResult<Py<PyAny>> {
    let record = r.map_err(PyEvtxError)?;
    Ok(record_to_pydict(record, py)?.into_any().unbind())
}

#[gen_stub_pyclass]
#[pyclass]
pub struct PyRecordsIterator {
    pub(crate) inner: IntoIterChunks<Box<dyn ReadSeek>>,
    pub(crate) records_iter: IntoIter<Result<SerializedEvtxRecord<String>, EvtxError>>,
    pub(crate) settings: Arc<ParserSettings>,
    pub(crate) output_format: OutputFormat,
    pub(crate) skip_errors: bool,
}

impl PyRecordsIterator {
    fn next(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        loop {
            match self.next_record(py) {
                Err(error)
                    if self.skip_errors
                        && error.is_instance_of::<pyo3::exceptions::PyRuntimeError>(py) =>
                {
                    py.import("warnings")?.call_method1(
                        "warn",
                        (
                            format!("Skipping EVTX data: {error}"),
                            py.get_type::<pyo3::exceptions::PyRuntimeWarning>(),
                            1,
                        ),
                    )?;
                }
                result => return result,
            }
        }
    }

    fn next_record(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        loop {
            if let Some(record) = self.records_iter.next() {
                return record_to_pyobject(record, py).map(Some);
            }

            let chunk = self.inner.next();

            match chunk {
                None => return Ok(None),
                Some(chunk_result) => match chunk_result {
                    Err(e) => {
                        return Err(PyEvtxError(e).into());
                    }
                    Ok(mut chunk) => {
                        let first_id = chunk.header.first_event_record_id;
                        let last_id = chunk.header.last_event_record_id;
                        let parsed_chunk = chunk.parse(self.settings.clone());

                        match parsed_chunk {
                            Err(e) => {
                                return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                                    "Failed to parse chunk containing record IDs {first_id}..{last_id}: {}",
                                    error_message(&e),
                                )));
                            }
                            Ok(mut chunk) => {
                                let records: Vec<_> = chunk
                                    .iter()
                                    .map(|record| {
                                        let record = record?;
                                        let record_id = record.event_record_id;
                                        let rendered = match self.output_format {
                                            OutputFormat::XML => record.into_xml(),
                                            OutputFormat::JSON => record.into_json(),
                                        };
                                        rendered.map_err(|source| EvtxError::FailedToParseRecord {
                                            record_id,
                                            source: Box::new(source),
                                        })
                                    })
                                    .collect();

                                self.records_iter = records.into_iter();
                            }
                        }
                    }
                },
            }
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyRecordsIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        slf.next(py)
    }
}
