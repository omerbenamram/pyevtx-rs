use std::fs::File;
use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::{
    exceptions::PyNotImplementedError, exceptions::PyRuntimeError, exceptions::PyTypeError,
    exceptions::PyValueError,
};
use pyo3_stub_gen::derive::*;

use evtx_rs::mta::MtaFile;
use evtx_rs::{AnsiCodec, EvtxParser, ParserSettings};

use crate::file_like::{path_string_from_pyany, FileOrFileLike, ReadSeek};
use crate::py_err::{py_err_from_mta_error, PyEvtxError};
use crate::records::{OutputFormat, PyRecordsIterator};

#[gen_stub_pyclass]
#[pyclass]
/// PyEvtxParser(path_or_file_like, number_of_threads=None, ansi_codec=None, validate_checksums=None, separate_json_attributes=None, indent=None, wevt_cache=None, skip_errors=False, recover_records=False, mta=None)
/// --
///
/// Returns an instance of the parser.
///
/// Args:
///     `path_or_file_like`: a path (str or `os.PathLike`), or a binary file-like object
///         with `read` and `seek`.
///
///     `number_of_threads` (int, optional): how many chunks to parse at the same time, each
///         on a thread of its own, while the records of the chunks before them are returned.
///         `0`, the default, uses one per core, and so does any number above that. `1`
///         parses on the calling thread. Chunks are parsed with the GIL released.
///
///     `ansi_codec` (str, optional): the encoding of the ANSI strings inside the log. Any
///         ASCII-compatible WHATWG Encoding Standard label, such as windows-1252 (the
///         default), windows-1251, iso-8859-2, koi8-r, gbk, shift_jis or utf-8. `ascii`
///         and `iso-8859-1` map to windows-1252.
///
///     `validate_checksums` (bool, optional): check each chunk's header and records
///         against their CRC32s, and fail on a chunk that doesn't match. Off by default.
///
///     `separate_json_attributes` (bool, optional): in JSON, put an element's attributes
///         in an object of their own, `<name>_attributes`, instead of under
///         `#attributes`. Off by default.
///
///     `indent` (bool, optional): indent the XML and JSON. On by default.
///
///     `wevt_cache` (WevtCache, str or `os.PathLike`, optional): `WEVT_TEMPLATE` templates,
///         as a `WevtCache` or the path of a `.wevtcache` file, to render the records
///         whose template the log is missing or can't read.
///
///     `skip_errors` (bool, optional): when set, a record or chunk that can't be parsed
///         emits a `RuntimeWarning` and iteration goes on, instead of raising
///         `RuntimeError`. Off by default.
///
///     `recover_records` (bool, optional): also yield the records left in unallocated chunk
///         space, such as those of a cleared chunk or in the slack after a chunk's last
///         record. They have `recovered` set to `True`, and the ones that can't be parsed
///         are left out. Off by default.
///
///     `mta` (str or `os.PathLike`, optional): the path of an `.MTA` file, from the
///         `LocaleMetaData` folder Windows saves next to a log exported with display
///         information. Each record then gets a `RenderingInfo` element with its message
///         and its level, task, opcode and keyword names, in the file's language.
pub struct PyEvtxParser {
    pub(crate) inner: Option<EvtxParser<Box<dyn ReadSeek>>>,
    pub(crate) configuration: ParserSettings,
    pub(crate) skip_errors: bool,
    /// How many chunks the records iterators parse at a time.
    pub(crate) num_threads: usize,
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
        skip_errors=false,
        recover_records=false,
        mta=None
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        path_or_file_like: Py<PyAny>,
        number_of_threads: Option<usize>,
        ansi_codec: Option<String>,
        validate_checksums: Option<bool>,
        separate_json_attributes: Option<bool>,
        indent: Option<bool>,
        wevt_cache: Option<Py<PyAny>>,
        skip_errors: bool,
        recover_records: bool,
        mta: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let file_or_file_like = FileOrFileLike::from_pyobject(path_or_file_like)?;

        let codec = resolve_ansi_codec(ansi_codec)?;

        // Chunks are parsed on rayon's pool, so more threads than it has would only hold more
        // chunks in memory.
        let pool_threads = rayon::current_num_threads();
        let num_threads = match number_of_threads.unwrap_or(0) {
            0 => pool_threads,
            n => n.min(pool_threads),
        };

        let mut configuration = ParserSettings::new()
            .ansi_codec(codec)
            .num_threads(num_threads);

        if let Some(validate_checksums) = validate_checksums {
            configuration = configuration.validate_checksums(validate_checksums);
        }
        if let Some(separate_json_attributes) = separate_json_attributes {
            configuration = configuration.separate_json_attributes(separate_json_attributes);
        }
        if let Some(indent) = indent {
            configuration = configuration.indent(indent);
        }
        configuration = configuration.recover_records(recover_records);
        if let Some(mta) = mta {
            let path = path_string_from_pyany(mta.bind(py))?.ok_or_else(|| {
                PyErr::new::<PyTypeError, _>("mta must be a path (str or os.PathLike)")
            })?;
            let mta = MtaFile::from_path(path).map_err(py_err_from_mta_error)?;
            configuration = configuration.mta(Some(Arc::new(mta)));
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
            num_threads,
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

        Ok(PyRecordsIterator::new(
            inner.into_chunks(),
            Arc::new(self.configuration.clone()),
            output_format,
            self.num_threads,
            self.skip_errors,
        ))
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
