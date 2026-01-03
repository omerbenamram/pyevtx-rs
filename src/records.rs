use std::sync::Arc;
use std::vec::IntoIter;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::*;

use evtx_rs::err::EvtxError;
use evtx_rs::{IntoIterChunks, ParserSettings, SerializedEvtxRecord};

use crate::file_like::ReadSeek;
use crate::py_err::PyEvtxError;

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
    match r {
        Ok(r) => match record_to_pydict(r, py) {
            Ok(dict) => Ok(dict.into_pyobject(py)?.into()),
            Err(e) => Ok(e.into_pyobject(py)?.into()),
        },
        Err(e) => Err(PyEvtxError(e).into()),
    }
}

#[gen_stub_pyclass]
#[pyclass]
pub struct PyRecordsIterator {
    pub(crate) inner: IntoIterChunks<Box<dyn ReadSeek>>,
    pub(crate) records_iter: IntoIter<Result<SerializedEvtxRecord<String>, EvtxError>>,
    pub(crate) settings: Arc<ParserSettings>,
    pub(crate) output_format: OutputFormat,
}

impl PyRecordsIterator {
    fn next(&mut self) -> PyResult<Option<Py<PyAny>>> {
        let mut chunk_id = 0;

        loop {
            if let Some(record) = self.records_iter.next() {
                let record = Python::attach(|py| record_to_pyobject(record, py).map(Some));
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
                                    source: Box::new(e),
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

#[gen_stub_pymethods]
#[pymethods]
impl PyRecordsIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> PyResult<Option<Py<PyAny>>> {
        slf.next()
    }
}

