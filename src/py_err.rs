use std::io;
use std::error::Error;

use pyo3::{
    exceptions::PyFileNotFoundError, exceptions::PyNotImplementedError, exceptions::PyOSError,
    exceptions::PyRuntimeError, exceptions::PyValueError, PyErr,
};

use evtx_rs::err;
use evtx_rs::err::{ChunkError, DeserializationError, EvtxError, InputError, SerializationError};

pub(crate) struct PyEvtxError(pub(crate) EvtxError);

pub(crate) fn py_err_from_io_err(e: &io::Error) -> PyErr {
    match e.kind() {
        io::ErrorKind::NotFound => PyErr::new::<PyFileNotFoundError, _>(format!("{e}")),
        _ => PyErr::new::<PyOSError, _>(format!("{e}")),
    }
}

impl From<PyEvtxError> for PyErr {
    fn from(err: PyEvtxError) -> Self {
        match err.0 {
            err::EvtxError::FailedToParseChunk {
                chunk_id: _,
                source,
            } => match *source {
                ChunkError::FailedToSeekToChunk(io) => py_err_from_io_err(&io),
                other => PyErr::new::<PyRuntimeError, _>(format!("{other}")),
            },
            EvtxError::InputError(e) => match e {
                InputError::FailedToOpenFile {
                    source: inner,
                    path: _,
                } => py_err_from_io_err(&inner),
            },
            EvtxError::SerializationError(e) => match e {
                SerializationError::Unimplemented { .. } => {
                    PyErr::new::<PyNotImplementedError, _>(format!("{e}"))
                }
                _ => PyErr::new::<PyRuntimeError, _>(format!("{e}")),
            },
            EvtxError::DeserializationError(e) => match e {
                DeserializationError::Io(ref io) => py_err_from_io_err(io),
                DeserializationError::IoWithContext(ref io) => match io.source() {
                    Some(inner_io_err) => match inner_io_err.downcast_ref::<io::Error>() {
                        Some(actual_inner_io_err) => py_err_from_io_err(actual_inner_io_err),
                        None => PyErr::new::<PyRuntimeError, _>(format!("{e}")),
                    },
                    None => PyErr::new::<PyRuntimeError, _>(format!("{e}")),
                },
                DeserializationError::FailedToReadToken { ref source, .. } => match source.source()
                {
                    Some(inner_io_err) => match inner_io_err.downcast_ref::<io::Error>() {
                        Some(actual_inner_io_err) => py_err_from_io_err(actual_inner_io_err),
                        None => PyErr::new::<PyRuntimeError, _>(format!("{e}")),
                    },
                    None => PyErr::new::<PyRuntimeError, _>(format!("{e}")),
                },
                _ => PyErr::new::<PyRuntimeError, _>(format!("{e}")),
            },
            EvtxError::Unimplemented { .. } => {
                PyErr::new::<PyNotImplementedError, _>(format!("{}", err.0))
            }
            EvtxError::IoError(io) => py_err_from_io_err(&io),
            _ => PyErr::new::<PyRuntimeError, _>(format!("{}", err.0)),
        }
    }
}

#[cfg(feature = "wevt_templates")]
pub(crate) fn py_err_from_wevt_cache_error(e: evtx_rs::wevt_templates::WevtCacheError) -> PyErr {
    use evtx_rs::wevt_templates::WevtCacheError;
    match e {
        WevtCacheError::CrimParse { .. }
        | WevtCacheError::TempSliceOutOfBounds { .. }
        | WevtCacheError::TemplateNotFound { .. }
        | WevtCacheError::TempTooSmall { .. } => PyErr::new::<PyRuntimeError, _>(format!("{e}")),
    }
}

#[cfg(feature = "wevt_templates")]
pub(crate) fn py_err_from_wevt_extract_error(
    e: evtx_rs::wevt_templates::WevtTemplateExtractError,
) -> PyErr {
    PyErr::new::<PyRuntimeError, _>(format!("{e}"))
}

#[cfg(feature = "wevt_templates")]
pub(crate) fn py_err_from_wevt_cache_file_error(
    e: evtx_rs::wevt_templates::WevtCacheFileError,
) -> PyErr {
    use evtx_rs::wevt_templates::WevtCacheFileError as E;
    use pyo3::exceptions::PyIOError;

    match e {
        E::OutputExists { .. }
        | E::InvalidMagic { .. }
        | E::UnsupportedVersion { .. }
        | E::UnknownEntryKind { .. }
        | E::EntryLengthTooLarge { .. } => PyErr::new::<PyValueError, _>(format!("{e}")),
        E::EntryCountOverflow { .. } => PyErr::new::<PyRuntimeError, _>(format!("{e}")),
        E::Io { .. } => PyErr::new::<PyIOError, _>(format!("{e}")),
    }
}

