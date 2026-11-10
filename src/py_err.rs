use std::{error::Error, io};

use pyo3::{
    exceptions::PyFileNotFoundError, exceptions::PyNotImplementedError, exceptions::PyOSError,
    exceptions::PyRuntimeError, exceptions::PyValueError, PyErr,
};

use evtx_rs::err::{ChunkError, DeserializationError, EvtxError, InputError, SerializationError};

pub(crate) struct PyEvtxError(pub(crate) EvtxError);

pub(crate) fn error_message(error: &(dyn Error + 'static)) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

/// An `.MTA` file that can't be read raises `OSError` (`FileNotFoundError` when it is missing),
/// and one that can't be parsed `ValueError`.
pub(crate) fn py_err_from_mta_error(e: evtx_rs::mta::MtaError) -> PyErr {
    match &e {
        evtx_rs::mta::MtaError::Io { source, .. } => {
            io_error_with_message(source, error_message(&e))
        }
        _ => PyValueError::new_err(e.to_string()),
    }
}

/// The error of an operation on `path`, with the path in its message.
#[cfg(feature = "wevt_templates")]
pub(crate) fn py_err_from_path_io_err(path: &std::path::Path, e: &io::Error) -> PyErr {
    match e.raw_os_error() {
        // As Python raises it: `OSError(errno, strerror, filename)` is the subclass that the
        // errno has, such as `FileNotFoundError`, with its `errno` and `filename` set.
        Some(errno) => {
            // Without Rust's " (os error 2)", which Python's `[Errno 2]` already says.
            let message = e.to_string();
            let strerror = message.split(" (os error").next().unwrap_or(&message);
            PyOSError::new_err((errno, strerror.to_string(), path.as_os_str().to_os_string()))
        }
        None => io_error_with_message(e, format!("{}: {}", path.display(), e)),
    }
}

fn io_error_with_message(e: &io::Error, message: String) -> PyErr {
    match e.kind() {
        io::ErrorKind::NotFound => PyFileNotFoundError::new_err(message),
        _ => PyOSError::new_err(message),
    }
}

impl From<PyEvtxError> for PyErr {
    fn from(err: PyEvtxError) -> Self {
        let message = error_message(&err.0);
        match err.0 {
            EvtxError::FailedToParseChunk { source, .. } => match *source {
                // This upstream variant does not expose the IO error through source().
                ChunkError::FailedToSeekToChunk(io) => {
                    io_error_with_message(&io, format!("{message}: {io}"))
                }
                _ => PyRuntimeError::new_err(message),
            },
            EvtxError::InputError(InputError::FailedToOpenFile { source, .. }) => {
                io_error_with_message(&source, message)
            }
            EvtxError::SerializationError(SerializationError::Unimplemented { .. })
            | EvtxError::Unimplemented { .. } => PyNotImplementedError::new_err(message),
            EvtxError::DeserializationError(DeserializationError::Io(io))
            | EvtxError::IoError(io) => io_error_with_message(&io, message),
            EvtxError::DeserializationError(
                DeserializationError::FailedToDeserializeTemplate { source, .. },
            ) => match *source {
                DeserializationError::Io(io) => io_error_with_message(&io, message),
                _ => PyRuntimeError::new_err(message),
            },
            _ => PyRuntimeError::new_err(message),
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
