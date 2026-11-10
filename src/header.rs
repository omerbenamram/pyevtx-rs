//! What a log's file header and chunk headers say, for triaging dirty, wrapped or damaged logs.

use pyo3::prelude::*;
use pyo3_stub_gen::derive::*;

use evtx_rs::{ChunkFlags, EvtxChunkData, EvtxFileHeader, HeaderFlags};

/// Chunks start after the 4096-byte header block, 65536 bytes apart.
const HEADER_BLOCK_SIZE: u64 = 4096;
const CHUNK_SIZE: u64 = 65536;

#[gen_stub_pyclass(module = "evtx._native")]
#[pyclass(module = "evtx", name = "FileHeader", frozen)]
#[derive(Clone)]
/// A log's file header, as `PyEvtxParser.header` gives it.
///
/// A log that wasn't closed cleanly is dirty, and its header can be behind its chunks: their
/// record IDs are what `records()` goes by.
pub struct PyFileHeader {
    /// The chunk that holds the oldest records, by the header.
    #[pyo3(get)]
    first_chunk_number: u64,
    /// The chunk being written, by the header.
    #[pyo3(get)]
    last_chunk_number: u64,
    /// The ID the next record gets, by the header.
    #[pyo3(get)]
    next_record_id: u64,
    #[pyo3(get)]
    header_size: u32,
    #[pyo3(get)]
    minor_version: u16,
    #[pyo3(get)]
    major_version: u16,
    #[pyo3(get)]
    header_block_size: u16,
    /// How many chunks the header says the log has.
    #[pyo3(get)]
    chunk_count: u16,
    /// The header's flags, every bit as stored: `0x1` dirty, `0x2` full, `0x4` no CRC32.
    #[pyo3(get)]
    flags: u32,
    /// Whether the log wasn't closed cleanly.
    #[pyo3(get)]
    is_dirty: bool,
    /// Whether the log is full.
    #[pyo3(get)]
    is_full: bool,
    /// The CRC32 of the header's first 120 bytes, as stored.
    #[pyo3(get)]
    checksum: u32,
    /// The CRC32 of the header's first 120 bytes as read.
    #[pyo3(get)]
    computed_checksum: u32,
    /// Whether the header is intact: `checksum` is `computed_checksum`, or the flags say that
    /// the header has no CRC32. A log whose header isn't is still read, since chunks are found at
    /// fixed offsets.
    #[pyo3(get)]
    checksum_valid: bool,
}

impl From<&EvtxFileHeader> for PyFileHeader {
    fn from(header: &EvtxFileHeader) -> Self {
        PyFileHeader {
            first_chunk_number: header.first_chunk_number,
            last_chunk_number: header.last_chunk_number,
            next_record_id: header.next_record_id,
            header_size: header.header_size,
            minor_version: header.minor_version,
            major_version: header.major_version,
            header_block_size: header.header_block_size,
            chunk_count: header.chunk_count,
            flags: header.flags.bits(),
            is_dirty: header.flags.contains(HeaderFlags::DIRTY),
            is_full: header.flags.contains(HeaderFlags::FULL),
            checksum: header.checksum,
            computed_checksum: header.computed_checksum,
            checksum_valid: header.validate_checksum(),
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyFileHeader {
    fn __repr__(&self) -> String {
        format!(
            "FileHeader(version={}.{}, chunks {}..{}, next_record_id={}, flags={:#x}, checksum_valid={})",
            self.major_version,
            self.minor_version,
            self.first_chunk_number,
            self.last_chunk_number,
            self.next_record_id,
            self.flags,
            py_bool(self.checksum_valid),
        )
    }
}

#[gen_stub_pyclass(module = "evtx._native")]
#[pyclass(module = "evtx", name = "ChunkHeader", frozen)]
/// A chunk's header, as `PyEvtxParser.chunks()` gives it.
pub struct PyChunkHeader {
    /// The chunk's number in the file.
    #[pyo3(get)]
    number: u64,
    /// Where the chunk starts in the file.
    #[pyo3(get)]
    offset: u64,
    #[pyo3(get)]
    first_event_record_number: u64,
    #[pyo3(get)]
    last_event_record_number: u64,
    #[pyo3(get)]
    first_event_record_id: u64,
    #[pyo3(get)]
    last_event_record_id: u64,
    #[pyo3(get)]
    header_size: u32,
    /// Where the chunk's last record starts, from the start of the chunk.
    #[pyo3(get)]
    last_event_record_data_offset: u32,
    /// Where the chunk's free space starts, after its records, from the start of the chunk.
    #[pyo3(get)]
    free_space_offset: u32,
    /// The chunk's flags, every bit as stored: `0x1` dirty, `0x4` no CRC32.
    #[pyo3(get)]
    flags: u32,
    /// Whether the chunk wasn't closed cleanly.
    #[pyo3(get)]
    is_dirty: bool,
    /// The CRC32 of the chunk's records, as stored.
    #[pyo3(get)]
    events_checksum: u32,
    /// The CRC32 of the chunk's header, as stored.
    #[pyo3(get)]
    header_chunk_checksum: u32,
    /// Whether the header's CRC32 matches, or the flags say that the chunk has none.
    #[pyo3(get)]
    header_checksum_valid: bool,
    /// Whether the records' CRC32 matches, or the flags say that the chunk has none.
    #[pyo3(get)]
    events_checksum_valid: bool,
}

impl PyChunkHeader {
    /// The header of `chunk`, which the parser read, so that it knows its number.
    pub(crate) fn new(chunk: &EvtxChunkData) -> Self {
        let number = chunk
            .number
            .expect("the parser numbers the chunks it reads");
        let header = &chunk.header;
        PyChunkHeader {
            number,
            offset: HEADER_BLOCK_SIZE + number * CHUNK_SIZE,
            first_event_record_number: header.first_event_record_number,
            last_event_record_number: header.last_event_record_number,
            first_event_record_id: header.first_event_record_id,
            last_event_record_id: header.last_event_record_id,
            header_size: header.header_size,
            last_event_record_data_offset: header.last_event_record_data_offset,
            free_space_offset: header.free_space_offset,
            flags: header.flags.bits(),
            is_dirty: header.flags.contains(ChunkFlags::DIRTY),
            events_checksum: header.events_checksum,
            header_chunk_checksum: header.header_chunk_checksum,
            header_checksum_valid: chunk.validate_header_checksum(),
            events_checksum_valid: chunk.validate_data_checksum(),
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyChunkHeader {
    fn __repr__(&self) -> String {
        format!(
            "ChunkHeader(number={}, record_ids={}..{}, flags={:#x}, header_checksum_valid={}, events_checksum_valid={})",
            self.number,
            self.first_event_record_id,
            self.last_event_record_id,
            self.flags,
            py_bool(self.header_checksum_valid),
            py_bool(self.events_checksum_valid),
        )
    }
}

fn py_bool(value: bool) -> &'static str {
    if value {
        "True"
    } else {
        "False"
    }
}
