use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};
use pyo3::{exceptions::PyTypeError, PyResult};
use pyo3_file::PyFileLikeObject;

use std::io::{Read, Seek};
use std::path::PathBuf;

pub(crate) trait ReadSeek: Read + Seek + Send + Sync + 'static {}

impl<T: Read + Seek + Send + Sync + 'static> ReadSeek for T {}

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
    if !(obj.is_instance_of::<PyString>()
        || obj.is_instance_of::<PyBytes>()
        || obj.hasattr("__fspath__")?)
    {
        return Ok(None);
    }
    let os = obj.py().import("os")?;
    // A `str` or `bytes`, or a `TypeError` that says what `__fspath__` returned instead.
    let mut path = os.call_method1("fspath", (obj,))?;
    if path.is_instance_of::<PyBytes>() {
        // As the file system encoding has it: on Unix, with the bytes that aren't valid in
        // it as surrogate escapes, which the extraction below turns back into those bytes.
        path = os.call_method1("fsdecode", (path,))?;
    }
    path.extract::<PathBuf>().map(Some)
}

impl FileOrFileLike {
    pub(crate) fn from_pyobject(path_or_file_like: Py<PyAny>) -> PyResult<FileOrFileLike> {
        Python::attach(|py| {
            let bound = path_or_file_like.bind(py);
            if let Some(path) = path_from_pyany(bound)? {
                return Ok(FileOrFileLike::File(path));
            }

            // A text-mode file gives `str`, decoded and with its newlines translated, and a
            // log is bytes: say so here, instead of failing on whatever the decoding makes of
            // the header.
            let text_io_base = py.import("io")?.getattr("TextIOBase")?;
            if bound.is_instance(&text_io_base)? {
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
