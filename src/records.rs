use std::any::Any;
use std::collections::BTreeMap;
use std::fmt::{Display, Write};
use std::ops::Range;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_stub_gen::derive::*;

use evtx_rs::err::EvtxError;
use evtx_rs::{EvtxChunkData, IntoIterChunks, ParserSettings};

use crate::file_like::ReadSeek;
use crate::py_err::{error_message, PyEvtxError};

#[derive(Copy, Clone, PartialOrd, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    JSON,
    XML,
}

/// Writes a record's timestamp as its dict has it, with ` UTC` after the RFC 3339 time.
fn write_timestamp(text: &mut String, timestamp: &impl Display) {
    let start = text.len();
    write!(text, "{timestamp}").expect("a String takes any text");
    if !text[start..].ends_with("UTC") {
        text.push_str(" UTC");
    }
}

/// A record of a parsed chunk, its text in the chunk's `text`.
struct RecordText {
    event_record_id: u64,
    timestamp: Range<usize>,
    data: Range<usize>,
    recovered: bool,
}

/// A parsed chunk's records, with their errors in their places. Their text is in one buffer:
/// the thread that turns them into dicts frees what the parsing threads allocated, and freeing
/// each record's strings made glibc's allocator lock out the parsing threads.
#[derive(Default)]
struct ChunkRecords {
    text: String,
    records: std::vec::IntoIter<PyResult<RecordText>>,
}

impl ChunkRecords {
    fn failed(error: PyErr) -> Self {
        ChunkRecords {
            text: String::new(),
            records: vec![Err(error)].into_iter(),
        }
    }
}

fn record_to_pydict<'py>(
    text: &str,
    record: RecordText,
    py: Python<'py>,
) -> PyResult<Bound<'py, PyDict>> {
    let pyrecord = PyDict::new(py);

    pyrecord.set_item("event_record_id", record.event_record_id)?;
    pyrecord.set_item("timestamp", &text[record.timestamp])?;
    pyrecord.set_item("data", &text[record.data])?;
    pyrecord.set_item("recovered", record.recovered)?;
    Ok(pyrecord)
}

/// Emits the `RuntimeWarning` that `skip_errors=True` turns `error` into.
pub(crate) fn warn_skipped(py: Python<'_>, error: &PyErr) -> PyResult<()> {
    py.import("warnings")?.call_method1(
        "warn",
        (
            format!("Skipping EVTX data: {error}"),
            py.get_type::<pyo3::exceptions::PyRuntimeWarning>(),
            1,
        ),
    )?;
    Ok(())
}

/// A chunk's records, or the panic its parse ended in (in a `Mutex`, which makes it `Sync`).
type Parsed = Result<ChunkRecords, Mutex<Box<dyn Any + Send>>>;

#[gen_stub_pyclass]
#[pyclass]
pub struct PyRecordsIterator {
    chunks: ChunkPipeline,
    /// The records of the chunk being returned.
    records: ChunkRecords,
    skip_errors: bool,
}

impl PyRecordsIterator {
    /// An iterator over the records of `chunks`, which parses `num_threads` chunks at a time.
    pub(crate) fn new(
        chunks: IntoIterChunks<Box<dyn ReadSeek>>,
        settings: Arc<ParserSettings>,
        output_format: OutputFormat,
        num_threads: usize,
        skip_errors: bool,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        PyRecordsIterator {
            chunks: ChunkPipeline {
                chunks,
                settings,
                output_format,
                num_threads,
                next_in: 0,
                next_out: 0,
                exhausted: false,
                parsed: BTreeMap::new(),
                sender,
                receiver: Mutex::new(receiver),
            },
            records: ChunkRecords::default(),
            skip_errors,
        }
    }

    fn next(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        loop {
            match self.next_record(py) {
                Err(error) if self.skip_errors && error.is_instance_of::<PyRuntimeError>(py) => {
                    warn_skipped(py, &error)?;
                }
                result => return result,
            }
        }
    }

    fn next_record(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        loop {
            if let Some(record) = self.records.records.next() {
                let record = record_to_pydict(&self.records.text, record?, py)?;
                return Ok(Some(record.into_any().unbind()));
            }
            let chunks = &mut self.chunks;
            match py.detach(|| chunks.next()) {
                Some(records) => self.records = records,
                None => return Ok(None),
            }
        }
    }
}

/// Parses a log's chunks on rayon's pool, `num_threads` at a time, while the records of the
/// chunks before them are returned, and returns each chunk's records in the log's order.
struct ChunkPipeline {
    chunks: IntoIterChunks<Box<dyn ReadSeek>>,
    settings: Arc<ParserSettings>,
    output_format: OutputFormat,
    num_threads: usize,
    /// Chunks are numbered in the order they are read. Those from `next_out` up to `next_in`
    /// are being parsed, or wait in `parsed` for the ones before them.
    next_in: u64,
    next_out: u64,
    exhausted: bool,
    parsed: BTreeMap<u64, Parsed>,
    sender: Sender<(u64, Parsed)>,
    receiver: Mutex<Receiver<(u64, Parsed)>>,
}

impl ChunkPipeline {
    /// The records of the next chunk, or `None` at the end of the log. This waits for the chunk
    /// to be parsed, so call it with the GIL released.
    fn next(&mut self) -> Option<ChunkRecords> {
        if self.num_threads == 1 {
            // The one thread is this one.
            return Some(match self.chunks.next()? {
                Ok(chunk) => chunk_records(chunk, &self.settings, self.output_format),
                Err(e) => ChunkRecords::failed(PyEvtxError(e).into()),
            });
        }
        loop {
            self.fill();
            if let Some(parsed) = self.parsed.remove(&self.next_out) {
                self.next_out += 1;
                // Keep `num_threads` chunks being parsed while the caller goes through this one.
                self.fill();
                return Some(parsed.unwrap_or_else(|panic| {
                    panic::resume_unwind(panic.into_inner().unwrap_or_else(PoisonError::into_inner))
                }));
            }
            if self.next_out == self.next_in {
                return None;
            }
            let receiver = self
                .receiver
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner);
            let (number, parsed) = receiver.recv().expect("the pipeline keeps a sender");
            self.parsed.insert(number, parsed);
        }
    }

    /// Reads chunks and hands them to the pool until `num_threads` are being parsed.
    fn fill(&mut self) {
        while !self.exhausted && self.next_in - self.next_out < self.num_threads as u64 {
            let Some(chunk) = self.chunks.next() else {
                self.exhausted = true;
                break;
            };
            let number = self.next_in;
            self.next_in += 1;
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(e) => {
                    let records = ChunkRecords::failed(PyEvtxError(e).into());
                    self.parsed.insert(number, Ok(records));
                    continue;
                }
            };
            let settings = Arc::clone(&self.settings);
            let (output_format, sender) = (self.output_format, self.sender.clone());
            rayon::spawn(move || {
                // A panic is passed on to the caller: left to rayon, it would abort the process.
                let parsed = panic::catch_unwind(AssertUnwindSafe(|| {
                    chunk_records(chunk, &settings, output_format)
                }));
                // The receiver is gone if the iterator was dropped.
                let _ = sender.send((number, parsed.map_err(Mutex::new)));
            });
        }
    }
}

/// The records of a chunk, rendered as `output_format`, or the error the chunk can't be parsed
/// with.
fn chunk_records(
    mut data: EvtxChunkData,
    settings: &Arc<ParserSettings>,
    output_format: OutputFormat,
) -> ChunkRecords {
    let first_id = data.header.first_event_record_id;
    let last_id = data.header.last_event_record_id;
    let number = data.number.expect("the parser numbers the chunks it reads");
    let capacity = 2 * data.data.len();
    let mut chunk = match data.parse(Arc::clone(settings)) {
        Ok(chunk) => chunk,
        Err(e) => {
            return ChunkRecords::failed(PyRuntimeError::new_err(format!(
                "Failed to parse chunk number {number} (record IDs {first_id}..{last_id}): {}",
                error_message(&e),
            )));
        }
    };
    let mut text = String::with_capacity(capacity);
    let mut records = vec![];
    for record in chunk.iter() {
        let record = match record {
            Ok(record) => record,
            Err(error) => {
                records.push(Err(PyEvtxError(error).into()));
                continue;
            }
        };
        let event_record_id = record.event_record_id;
        let recovered = record.recovered;
        let rendered = match output_format {
            OutputFormat::XML => record.into_xml(),
            OutputFormat::JSON => record.into_json(),
        };
        let rendered = match rendered {
            Ok(rendered) => rendered,
            // A carved record that can't be rendered is noise, as in evtx's own record
            // iterators.
            Err(_) if recovered => continue,
            Err(source) => {
                records.push(Err(PyEvtxError(EvtxError::FailedToParseRecord {
                    record_id: event_record_id,
                    source: Box::new(source),
                })
                .into()));
                continue;
            }
        };
        let start = text.len();
        write_timestamp(&mut text, &rendered.timestamp);
        let end_of_timestamp = text.len();
        text.push_str(&rendered.data);
        records.push(Ok(RecordText {
            event_record_id,
            timestamp: start..end_of_timestamp,
            data: end_of_timestamp..text.len(),
            recovered,
        }));
    }
    ChunkRecords {
        text,
        records: records.into_iter(),
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
