use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};
use pyo3::{exceptions::PyTypeError, PyErr, PyResult};
use pyo3_file::PyFileLikeObject;

use std::io::{Read, Seek};

pub(crate) trait ReadSeek: Read + Seek + Send + Sync + 'static {
}

impl<T: Read + Seek + Send + Sync + 'static> ReadSeek for T {}

#[derive(Debug)]
pub(crate) enum FileOrFileLike {
    File(String),
    FileLike(PyFileLikeObject),
}

pub(crate) fn path_string_from_pyany(obj: &Bound<'_, PyAny>) -> PyResult<Option<String>> {
    // Plain strings are valid paths.
    if let Ok(s) = obj.downcast::<PyString>() {
        return Ok(Some(s.to_string_lossy().to_string()));
    }

    // Support pathlib.Path and other os.PathLike objects.
    if obj.hasattr("__fspath__")? {
        let path = obj.call_method0("__fspath__")?;
        if let Ok(s) = path.downcast::<PyString>() {
            return Ok(Some(s.to_string_lossy().to_string()));
        }
        if let Ok(b) = path.downcast::<PyBytes>() {
            return Ok(Some(String::from_utf8_lossy(b.as_bytes()).to_string()));
        }
        return Err(PyErr::new::<PyTypeError, _>(
            "__fspath__ must return str or bytes",
        ));
    }

    Ok(None)
}

impl FileOrFileLike {
    pub(crate) fn from_pyobject(path_or_file_like: Py<PyAny>) -> PyResult<FileOrFileLike> {
        Python::attach(|py| {
            let bound = path_or_file_like.bind(py);
            if let Some(path) = path_string_from_pyany(&bound)? {
                return Ok(FileOrFileLike::File(path));
            }

            // We only need read + seek
            match PyFileLikeObject::with_requirements(path_or_file_like, true, false, true, true) {
                Ok(f) => Ok(FileOrFileLike::FileLike(f)),
                Err(e) => Err(e),
            }
        })
    }
}

