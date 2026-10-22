use std::fs::File;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::{
    exceptions::PyNotImplementedError, exceptions::PyRuntimeError, exceptions::PyValueError,
};
use pyo3_stub_gen::derive::*;

use evtx_rs::{AnsiCodec, EvtxParser, ParserSettings};

use crate::file_like::{FileOrFileLike, ReadSeek};
use crate::py_err::PyEvtxError;
use crate::records::{OutputFormat, PyRecordsIterator};

#[gen_stub_pyclass]
#[pyclass]
/// PyEvtxParser(self, path_or_file_like, number_of_threads=0, ansi_codec='windows-1252', /)
/// --
///
/// Returns an instance of the parser.
///
/// Args:
///     `path_or_file_like`: a path (string), or a file-like object.
///
///     `number_of_threads` (int, optional):
///            limit the number of worker threads used by rust.
///            `0` (the default) will let the library decide how many threads to use
///            based on the number of cores available.
///
///     `ansi_codec`(str, optional) to control encoding of ansi strings inside the evtx file.
///
///                  Any ASCII-compatible WHATWG Encoding Standard label, such as
///                  windows-1252 (the default), windows-1251, iso-8859-2, koi8-r,
///                  gbk, shift_jis or utf-8. `ascii` and `iso-8859-1` map to windows-1252.
pub struct PyEvtxParser {
    pub(crate) inner: Option<EvtxParser<Box<dyn ReadSeek>>>,
    pub(crate) configuration: ParserSettings,
    pub(crate) skip_errors: bool,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyEvtxParser {
    #[new]
    #[pyo3(signature = (
        path_or_file_like,
        number_of_threads=None,
        ansi_codec=None,
        validate_checksums=None,
        separate_json_attributes=None,
        indent=None,
        wevt_cache=None,
        skip_errors=false
    ))]
    fn new(
        path_or_file_like: Py<PyAny>,
        number_of_threads: Option<usize>,
        ansi_codec: Option<String>,
        validate_checksums: Option<bool>,
        separate_json_attributes: Option<bool>,
        indent: Option<bool>,
        wevt_cache: Option<Py<PyAny>>,
        skip_errors: bool,
    ) -> PyResult<Self> {
        let file_or_file_like = FileOrFileLike::from_pyobject(path_or_file_like)?;

        let codec = resolve_ansi_codec(ansi_codec)?;

        // Setup `number_of_threads` (0 = let evtx pick, matching ParserSettings::default()).
        let number_of_threads = number_of_threads.unwrap_or(0);

        let mut configuration = ParserSettings::new()
            .ansi_codec(codec)
            .num_threads(number_of_threads);

        if let Some(validate_checksums) = validate_checksums {
            configuration = configuration.validate_checksums(validate_checksums);
        }
        if let Some(separate_json_attributes) = separate_json_attributes {
            configuration = configuration.separate_json_attributes(separate_json_attributes);
        }
        if let Some(indent) = indent {
            configuration = configuration.indent(indent);
        }

        if let Some(wevt_cache) = wevt_cache {
            #[cfg(feature = "wevt_templates")]
            {
                let cache = crate::wevt_cache::wevt_cache_from_pyobject(wevt_cache)?;
                configuration = configuration.wevt_cache(Some(cache));
            }

            #[cfg(not(feature = "wevt_templates"))]
            {
                let _ = wevt_cache;
                return Err(PyErr::new::<PyNotImplementedError, _>(
                    "WEVT support is not enabled in this build (compile with `wevt_templates`)",
                ));
            }
        }

        let boxed_read_seek = match file_or_file_like {
            FileOrFileLike::File(s) => {
                let file = File::open(s)?;
                Box::new(file) as Box<dyn ReadSeek>
            }
            FileOrFileLike::FileLike(f) => Box::new(f) as Box<dyn ReadSeek>,
        };

        let parser = EvtxParser::from_read_seek(boxed_read_seek)
            .map_err(PyEvtxError)?
            .with_configuration(configuration.clone());

        Ok(PyEvtxParser {
            inner: Some(parser),
            configuration,
            skip_errors,
        })
    }

    /// records(self, /)
    /// --
    ///
    /// Returns an iterator of XML records. Parse errors raise `RuntimeError` by default.
    /// With `skip_errors=True`, recoverable parse errors emit `RuntimeWarning` and
    /// iteration continues. Python warnings filters can promote these warnings to errors.
    /// Exceptions are never yielded as records.
    fn records(&mut self) -> PyResult<PyRecordsIterator> {
        self.records_iterator(OutputFormat::XML)
    }

    /// records_json(self, /)
    /// --
    ///
    /// Returns an iterator of JSON records. Parse errors raise `RuntimeError` by default.
    /// With `skip_errors=True`, recoverable parse errors emit `RuntimeWarning` and
    /// iteration continues. Python warnings filters can promote these warnings to errors.
    /// Exceptions are never yielded as records.
    fn records_json(&mut self) -> PyResult<PyRecordsIterator> {
        self.records_iterator(OutputFormat::JSON)
    }

    fn __iter__(mut slf: PyRefMut<Self>) -> PyResult<PyRecordsIterator> {
        slf.records()
    }

    fn __next__(_slf: PyRefMut<Self>) -> PyResult<Option<Py<PyAny>>> {
        Err(PyErr::new::<PyNotImplementedError, _>(
            "Using `next()` over `PyEvtxParser` is not supported. Try iterating over `PyEvtxParser(...).records()`",
        ))
    }
}

impl PyEvtxParser {
    fn records_iterator(&mut self, output_format: OutputFormat) -> PyResult<PyRecordsIterator> {
        let inner = match self.inner.take() {
            Some(inner) => inner,
            None => {
                return Err(PyErr::new::<PyRuntimeError, _>(
                    "PyEvtxParser can only be used once",
                ));
            }
        };

        Ok(PyRecordsIterator {
            inner: inner.into_chunks(),
            records_iter: Vec::new().into_iter(),
            settings: Arc::new(self.configuration.clone()),
            output_format,
            skip_errors: self.skip_errors,
        })
    }
}

/// Resolves an `ansi_codec` argument (a WHATWG Encoding Standard label) to the
/// codec evtx decodes ANSI strings with. `None` keeps evtx's default, windows-1252.
pub(crate) fn resolve_ansi_codec(ansi_codec: Option<String>) -> PyResult<AnsiCodec> {
    let Some(label) = ansi_codec else {
        return Ok(encoding_rs::WINDOWS_1252);
    };
    let codec = encoding_rs::Encoding::for_label(label.as_bytes()).ok_or_else(|| {
        PyErr::new::<PyValueError, _>(format!(
            "Unknown encoding `{label}`, expected an Encoding Standard label such as windows-1252"
        ))
    })?;
    if !codec.is_ascii_compatible() {
        return Err(PyErr::new::<PyValueError, _>(format!(
            "`{label}` is not an ASCII-compatible encoding; ANSI strings cannot be UTF-16"
        )));
    }
    Ok(codec)
}
