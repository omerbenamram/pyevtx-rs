use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};
use pyo3::{exceptions::PyTypeError, PyResult};
use pyo3_file::PyFileLikeObject;

use evtx_rs::EvtxParser;

use crate::py_err::PyEvtxError;

use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

pub(crate) trait ReadSeek: Read + Seek + Send + Sync + 'static {}

impl<T: Read + Seek + Send + Sync + 'static> ReadSeek for T {}

/// The exception that a file-like object's `read()` or `seek()` raised, kept for the caller
/// of the parser: the parser sees an I/O error of some kind, or the end of the file, and what
/// it makes of that is not what Python raised (a `KeyboardInterrupt`, say).
///
/// Whoever has the parser read takes it afterwards and raises it. Until then every read
/// fails, so the parser can't go on to make something else of the file.
#[derive(Clone, Default)]
pub(crate) struct Raised(Arc<Mutex<Option<PyErr>>>);

impl Raised {
    fn slot(&self) -> std::sync::MutexGuard<'_, Option<PyErr>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The exception, which is then the caller's to raise.
    pub(crate) fn take(&self) -> Option<PyErr> {
        self.slot().take()
    }

    /// The exception if there is one, and otherwise `error`, which the parser gave.
    pub(crate) fn or(&self, error: impl Into<PyErr>) -> PyErr {
        self.take().unwrap_or_else(|| error.into())
    }

    /// The items of `reads`, an iterator that reads the file. After the item that the
    /// file-like object raised during, whatever the parser made of it, comes the exception,
    /// and nothing else.
    pub(crate) fn watch<'a, I: Iterator + 'a>(
        &'a self,
        mut reads: I,
    ) -> impl Iterator<Item = PyResult<I::Item>> + 'a {
        let mut ended = false;
        std::iter::from_fn(move || {
            if ended {
                return None;
            }
            let item = reads.next();
            if let Some(raised) = self.take() {
                ended = true;
                return Some(Err(raised));
            }
            item.map(Ok)
        })
    }

    fn io<T>(&self, operation: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
        if self.slot().is_some() {
            return Err(io::Error::other("the file-like object raised an exception"));
        }
        let result = operation();
        if let Err(error) = &result {
            // pyo3 turns an exception into an `io::Error` that holds it. An
            // `InterruptedError` is one that `std` tries again after, and is not kept.
            let raised = error.get_ref().and_then(|e| e.downcast_ref::<PyErr>());
            if let (Some(raised), false) = (raised, error.kind() == io::ErrorKind::Interrupted) {
                *self.slot() = Some(Python::attach(|py| raised.clone_ref(py)));
            }
        }
        result
    }
}

/// A file-like object whose exceptions are kept in `raised`.
struct Watched {
    file: PyFileLikeObject,
    raised: Raised,
}

impl Read for Watched {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let Watched { file, raised } = self;
        raised.io(|| file.read(buf))
    }
}

impl Seek for Watched {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let Watched { file, raised } = self;
        raised.io(|| file.seek(pos))
    }
}

#[derive(Debug)]
pub(crate) enum FileOrFileLike {
    File(PathBuf),
    FileLike(PyFileLikeObject),
}

/// The path that `obj` is, when it is one: a `str`, a `bytes` or an `os.PathLike`, as
/// `open()` takes them. `None` for anything else, such as a file-like object.
///
/// The path is the one Python means, byte for byte: a file name that isn't valid UTF-8 comes
/// from `os.listdir` and `pathlib` as a `str` with surrogate escapes, or as `bytes`, and
/// either opens the file.
pub(crate) fn path_from_pyany(obj: &Bound<'_, PyAny>) -> PyResult<Option<PathBuf>> {
    let os = obj.py().import("os")?;
    // `os.PathLike` is what `os.fspath` goes by: a type with `__fspath__`.
    if !(obj.is_instance_of::<PyString>()
        || obj.is_instance_of::<PyBytes>()
        || obj.is_instance(&os.getattr("PathLike")?)?)
    {
        return Ok(None);
    }
    // A `str` or `bytes`, or a `TypeError` that says what `__fspath__` returned instead.
    let mut path = os.call_method1("fspath", (obj,))?;
    if let Ok(bytes) = path.cast::<PyBytes>() {
        // No file name has a NUL in it, and a log's contents do.
        if bytes.as_bytes().contains(&0) {
            return Err(PyTypeError::new_err(
                "a bytes object is taken as a path, and this one has a NUL byte in it: \
                 to parse data held in memory, pass io.BytesIO(data)",
            ));
        }
        // As the file system encoding has it: on Unix, with the bytes that aren't valid in
        // it as surrogate escapes, which the extraction below turns back into those bytes.
        path = os.call_method1("fsdecode", (path,))?;
    }
    path.extract::<PathBuf>().map(Some)
}

/// The path that the argument `name` is: as `path_from_pyany`, with a `TypeError` for
/// anything that isn't a path.
pub(crate) fn required_path(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<PathBuf> {
    path_from_pyany(obj)?.ok_or_else(|| {
        PyTypeError::new_err(format!(
            "{} must be a path (str, bytes or os.PathLike)",
            name
        ))
    })
}

impl FileOrFileLike {
    /// A parser of the log, and where what its file-like object raises is kept (nothing, for
    /// a path).
    pub(crate) fn into_parser(self) -> PyResult<(EvtxParser<Box<dyn ReadSeek>>, Raised)> {
        let raised = Raised::default();
        let reader = match self {
            FileOrFileLike::File(path) => Box::new(std::fs::File::open(path)?) as Box<dyn ReadSeek>,
            FileOrFileLike::FileLike(file) => {
                let raised = raised.clone();
                Box::new(Watched { file, raised })
            }
        };
        let parser = EvtxParser::from_read_seek(reader)
            // What `read()` raised, as it raised it, rather than an `OSError` made of it.
            .map_err(|error| raised.or(PyEvtxError(error)))?;
        Ok((parser, raised))
    }

    pub(crate) fn from_pyobject(path_or_file_like: Py<PyAny>) -> PyResult<FileOrFileLike> {
        Python::attach(|py| {
            let bound = path_or_file_like.bind(py);
            if let Some(path) = path_from_pyany(bound)? {
                return Ok(FileOrFileLike::File(path));
            }

            // A text-mode file gives `str`, decoded and with its newlines translated, and a
            // log is bytes: say so here, instead of failing on whatever the decoding makes of
            // the header. Reading nothing shows what `read()` gives, for a wrapper of a text
            // file as for an `io.TextIOBase`.
            // Whatever is wrong with a `read()` that raises, the reader below says.
            let reads_text = bound
                .call_method1("read", (0,))
                .map_or(false, |read| read.is_instance_of::<PyString>());
            if reads_text {
                return Err(PyTypeError::new_err(
                    "The file object is in text mode, and an EVTX log is binary: open it in \
                     binary mode, as in open(path, \"rb\"), or pass the path itself.",
                ));
            }

            // We only need read + seek
            match PyFileLikeObject::with_requirements(path_or_file_like, true, false, true, false) {
                Ok(f) => Ok(FileOrFileLike::FileLike(f)),
                Err(e) => Err(e),
            }
        })
    }
}
