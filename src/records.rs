use std::any::Any;
use std::collections::BTreeMap;
use std::fmt::{Display, Write};
use std::ops::Range;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::{self, ThreadId};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::sync::MutexExt;
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

/// A chunk's records, or the panic its parse ended in.
type Parsed = Result<ChunkRecords, Box<dyn Any + Send>>;

/// The chunks that the pool's threads have parsed, by number, until the iterator takes them.
///
/// Not a channel: a thread that waits on one parks, and on macOS a thread that has parked can't
/// park again in a forked child (its semaphore is the parent's, and the child dies in
/// `dispatch_semaphore_wait`). A condition variable has no such state.
#[derive(Default)]
struct Arrivals {
    parsed: Mutex<Vec<(u64, Parsed)>>,
    arrived: Condvar,
}

impl Arrivals {
    fn send(&self, number: u64, parsed: Parsed) {
        let mut queue = self.parsed.lock().unwrap_or_else(PoisonError::into_inner);
        queue.push((number, parsed));
        self.arrived.notify_one();
    }

    /// Waits until at least one chunk has been parsed, and takes what there is.
    fn receive(&self) -> Vec<(u64, Parsed)> {
        let mut queue = self.parsed.lock().unwrap_or_else(PoisonError::into_inner);
        while queue.is_empty() {
            queue = self
                .arrived
                .wait(queue)
                .unwrap_or_else(PoisonError::into_inner);
        }
        std::mem::take(&mut *queue)
    }
}

#[gen_stub_pyclass(module = "evtx._native")]
#[pyclass(module = "evtx", frozen)]
pub struct PyRecordsIterator {
    /// Python threads can share the iterator: a call that finds another one going waits for it
    /// with the GIL released, since that one can be waiting for a chunk with the GIL released.
    records: Mutex<Records>,
    /// The thread that holds `records`, while it does. Reading a chunk calls a file-like
    /// object's `read` and `seek`, which are Python code and can come back to this iterator.
    advancing: Mutex<Option<ThreadId>>,
    /// Whether that thread came back to the iterator during the call that is advancing it.
    reentered: AtomicBool,
    skip_errors: bool,
}

/// Says that no thread is advancing the iterator any more, on the way out of `next`.
struct Advancing<'a>(&'a Mutex<Option<ThreadId>>);

impl Drop for Advancing<'_> {
    fn drop(&mut self) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

/// Where a records iterator is: the chunks, and the records of the one being returned.
struct Records {
    chunks: ChunkPipeline,
    current: ChunkRecords,
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
        let chunks = ChunkPipeline {
            chunks,
            settings,
            output_format,
            num_threads,
            process: std::process::id(),
            next_in: 0,
            next_out: 0,
            exhausted: false,
            parsed: BTreeMap::new(),
            arrivals: Arc::default(),
        };
        PyRecordsIterator {
            records: Mutex::new(Records {
                chunks,
                current: ChunkRecords::default(),
            }),
            advancing: Mutex::new(None),
            reentered: AtomicBool::new(false),
            skip_errors,
        }
    }

    fn next(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        loop {
            // Lets go of the records before warning: warnings filters and hooks run Python code.
            let record = self.advance(py)?;
            match record {
                Err(error) if self.skip_errors && error.is_instance_of::<PyRuntimeError>(py) => {
                    warn_skipped(py, &error)?;
                }
                result => return result,
            }
        }
    }
}

impl PyRecordsIterator {
    /// The next record or its parse error, in the inner result. The outer error is for a call
    /// that can't advance the iterator at all, which `skip_errors` doesn't skip.
    fn advance(&self, py: Python<'_>) -> PyResult<PyResult<Option<Py<PyAny>>>> {
        let this_thread = thread::current().id();
        let advancing = || {
            self.advancing
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
        };
        // Only this thread sets it to its own ID, and only while it holds the records.
        if *advancing() == Some(this_thread) {
            // Waiting for the records would be waiting for this thread, forever.
            self.reentered.store(true, Ordering::Relaxed);
            return Err(reentered_error());
        }
        let mut records = self
            .records
            .lock_py_attached(py)
            // A panic in a call before left the records as they were.
            .unwrap_or_else(PoisonError::into_inner);
        *advancing() = Some(this_thread);
        // Dropped before the records are let go of.
        let _advancing = Advancing(&self.advancing);
        self.reentered.store(false, Ordering::Relaxed);
        match records.next(py) {
            // The error above went through `read()` or `seek()` to the parser, which reports
            // a read that failed as a chunk cut short. Say what happened instead.
            Err(error) if self.reentered.swap(false, Ordering::Relaxed) => {
                let reentered = reentered_error();
                reentered.set_cause(py, Some(error));
                Err(reentered)
            }
            record => Ok(record),
        }
    }
}

fn reentered_error() -> PyErr {
    PyRuntimeError::new_err(
        "The records iterator was advanced from inside its own file-like object's read() or \
         seek(), while it was reading the next chunk from it. A file-like object must not \
         iterate over the parser that reads it.",
    )
}

impl Records {
    fn next(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        loop {
            if let Some(record) = self.current.records.next() {
                let record = record_to_pydict(&self.current.text, record?, py)?;
                return Ok(Some(record.into_any().unbind()));
            }
            let chunks = &mut self.chunks;
            match py.detach(|| chunks.next()) {
                Some(records) => self.current = records,
                None => return Ok(None),
            }
        }
    }
}

/// Parses a log's chunks on the pool of `crate::pool`, `num_threads` at a time, while the records of the
/// chunks before them are returned, and returns each chunk's records in the log's order.
struct ChunkPipeline {
    chunks: IntoIterChunks<Box<dyn ReadSeek>>,
    settings: Arc<ParserSettings>,
    output_format: OutputFormat,
    num_threads: usize,
    /// The process that handed out the chunks being parsed: its pool's threads have them.
    process: u32,
    /// Chunks are numbered in the order they are read. Those from `next_out` up to `next_in`
    /// are being parsed, or wait in `parsed` for the ones before them.
    next_in: u64,
    next_out: u64,
    exhausted: bool,
    parsed: BTreeMap<u64, Parsed>,
    arrivals: Arc<Arrivals>,
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
            if self.next_in != self.next_out && self.process != std::process::id() {
                // The threads that were parsing the chunks in flight stayed in the parent. So
                // did whichever of them held the lock of `arrivals`, if one did: the child
                // starts a new one, before it hands out any chunk.
                self.process = std::process::id();
                self.arrivals = Arc::default();
                self.parsed.clear();
                let lost = self.next_in - self.next_out;
                self.next_out = self.next_in;
                return Some(ChunkRecords::failed(PyRuntimeError::new_err(format!(
                    "The process forked while {lost} chunk(s) were being parsed, and their \
                     records are lost. Create the parser after the fork."
                ))));
            }
            self.fill();
            if let Some(parsed) = self.parsed.remove(&self.next_out) {
                self.next_out += 1;
                // Keep `num_threads` chunks being parsed while the caller goes through this one.
                self.fill();
                return Some(parsed.unwrap_or_else(|panic| panic::resume_unwind(panic)));
            }
            if self.next_out == self.next_in {
                return None;
            }
            self.parsed.extend(self.arrivals.receive());
        }
    }

    /// Reads chunks and hands them to the pool until `num_threads` are being parsed.
    fn fill(&mut self) {
        while !self.exhausted && self.next_in - self.next_out < self.num_threads as u64 {
            let Some(chunk) = self.chunks.next() else {
                self.exhausted = true;
                break;
            };
            // With chunks in flight, it stays the process whose threads have them.
            if self.next_in == self.next_out {
                self.process = std::process::id();
            }
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
            let (output_format, arrivals) = (self.output_format, Arc::clone(&self.arrivals));
            let parse = move || {
                // A panic is passed on to the caller: left to rayon, it would abort the process.
                let parsed = panic::catch_unwind(AssertUnwindSafe(|| {
                    chunk_records(chunk, &settings, output_format)
                }));
                arrivals.send(number, parsed);
            };
            // Asked for each time: after a fork, the pool is a new one (see `crate::pool`).
            match crate::pool::pool() {
                Ok(pool) => pool.spawn(parse),
                // Without threads (a process at its limit of them), this one parses.
                Err(_) => parse(),
            }
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

    /// The next record: a dict with its `event_record_id`, its `timestamp`, its `data` (the
    /// XML or JSON text) and whether it was `recovered`.
    #[gen_stub(override_return_type(type_repr = "_types.Record", imports = ("evtx._types")))]
    fn __next__(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        self.next(py)
    }
}
