#![cfg(feature = "wevt_templates")]

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bumpalo::Bump;
use encoding::all::encodings;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PySequence};
use pyo3::{
    exceptions::PyKeyError, exceptions::PyRuntimeError, exceptions::PyTypeError,
    exceptions::PyValueError,
};
use pyo3_stub_gen::derive::*;

use evtx_rs::binxml::value_variant::BinXmlValue;
use evtx_rs::wevt_templates::{normalize_guid, render_temp_to_xml_with_values, WevtCache};
use evtx_rs::{EvtxParser, ParserSettings};

use crate::file_like::{path_string_from_pyany, FileOrFileLike, ReadSeek};
use crate::py_err::{
    py_err_from_io_err, py_err_from_wevt_cache_error, py_err_from_wevt_cache_file_error,
    py_err_from_wevt_extract_error, PyEvtxError,
};

#[derive(Debug, Clone)]
struct PyWevtResource {
    data: Arc<Vec<u8>>,
}

#[gen_stub_pyclass]
#[pyclass(name = "WevtCache")]
pub struct PyWevtCache {
    inner: Arc<WevtCache>,
    event_to_template_guid: std::collections::HashMap<(String, u16, u8), String>,
    temps_by_guid: std::collections::HashMap<String, Arc<Vec<u8>>>,
    resources: Vec<PyWevtResource>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyWevtCache {
    #[new]
    fn new() -> Self {
        Self {
            inner: Arc::new(WevtCache::in_memory()),
            event_to_template_guid: std::collections::HashMap::new(),
            temps_by_guid: std::collections::HashMap::new(),
            resources: Vec::new(),
        }
    }

    #[staticmethod]
    /// load(path, /)
    /// --
    ///
    /// Load a WEVT template cache file (`.wevtcache`) produced by:
    /// `evtx_dump extract-wevt-templates --output cache.wevtcache ...`
    fn load(py: Python<'_>, path: Py<PyAny>) -> PyResult<Self> {
        let path = path_string_from_pyany(&path.bind(py))?.ok_or_else(|| {
            PyErr::new::<PyTypeError, _>("path must be a path (str or Path)")
        })?;
        let path_buf = PathBuf::from(&path);
        if path_buf.extension().and_then(|s| s.to_str()) != Some("wevtcache") {
            return Err(PyErr::new::<PyValueError, _>(
                "expected a `.wevtcache` file",
            ));
        }
        let mut cache = Self::new();
        cache.load_wevtcache_file(&path_buf)?;
        Ok(cache)
    }

    /// add_dll(self, path, /)
    /// --
    ///
    /// Parse a PE file (EXE/DLL/SYS) and add any `WEVT_TEMPLATE` resources to this cache.
    ///
    /// This is **strict**: failures to read/parse inputs raise exceptions.
    fn add_dll(&mut self, py: Python<'_>, path: Py<PyAny>) -> PyResult<usize> {
        let path = path_string_from_pyany(&path.bind(py))?
            .ok_or_else(|| PyErr::new::<PyTypeError, _>("path must be a path (str or Path)"))?;
        self.add_pe_file(PathBuf::from(path))
    }

    #[pyo3(signature = (path, recursive=true, extensions=None))]
    /// add_dir(self, path, recursive=True, extensions="exe,dll,sys", /)
    /// --
    ///
    /// Walk a directory and call `add_dll()` for all matching files.
    fn add_dir(
        &mut self,
        py: Python<'_>,
        path: Py<PyAny>,
        recursive: bool,
        extensions: Option<String>,
    ) -> PyResult<usize> {
        let path = path_string_from_pyany(&path.bind(py))?
            .ok_or_else(|| PyErr::new::<PyTypeError, _>("path must be a path (str or Path)"))?;

        let extensions = extensions.unwrap_or_else(|| "exe,dll,sys".to_string());
        let allowed_exts: std::collections::HashSet<String> = extensions
            .split(',')
            .map(|s| s.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        let mut files: Vec<PathBuf> = Vec::new();
        let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        collect_input_paths(
            &PathBuf::from(path),
            recursive,
            &allowed_exts,
            &mut seen,
            &mut files,
        )?;
        files.sort();

        let mut total = 0usize;
        for f in files {
            total = total.saturating_add(self.add_pe_file(f)?);
        }
        Ok(total)
    }

    #[pyo3(signature = (path, overwrite=false))]
    /// dump(self, path, overwrite=False, /)
    /// --
    ///
    /// Dump this in-memory cache to a single `.wevtcache` file.
    fn dump(&self, py: Python<'_>, path: Py<PyAny>, overwrite: bool) -> PyResult<()> {
        let path = path_string_from_pyany(&path.bind(py))?.ok_or_else(|| {
            PyErr::new::<PyTypeError, _>("path must be a path (str or Path)")
        })?;
        let path_buf = PathBuf::from(&path);
        if path_buf.extension().and_then(|s| s.to_str()) != Some("wevtcache") {
            return Err(PyErr::new::<PyValueError, _>(
                "expected a `.wevtcache` file",
            ));
        }
        self.dump_to_wevtcache_file(&path_buf, overwrite)
    }

    fn __repr__(&self) -> &'static str {
        "WevtCache(...)"
    }

    /// resolve_template_guid(self, provider_guid, event_id, version, /)
    /// --
    ///
    /// Resolve a template GUID using the cache index mapping of:
    /// (provider_guid, event_id, version) -> template_guid.
    fn resolve_template_guid(&self, provider_guid: String, event_id: u16, version: u8) -> PyResult<String> {
        let key = (normalize_guid(&provider_guid), event_id, version);
        self.event_to_template_guid
            .get(&key)
            .cloned()
            .ok_or_else(|| {
                PyErr::new::<PyKeyError, _>(format!(
                    "No template_guid found for provider_guid={} event_id={} version={}",
                    provider_guid, event_id, version
                ))
            })
    }

    #[pyo3(signature = (template_guid, substitutions, ansi_codec=None))]
    /// render_template_xml(self, template_guid, substitutions, ansi_codec=None, /)
    /// --
    ///
    /// Render a WEVT template to XML using substitution values (Python primitives).
    ///
    /// `substitutions` should be a list of values like: None/bool/int/float/str/bytes.
    fn render_template_xml(
        &self,
        template_guid: String,
        substitutions: &Bound<'_, PyAny>,
        ansi_codec: Option<String>,
    ) -> PyResult<String> {
        let codec = resolve_ansi_codec(ansi_codec)?;

        let bump = Bump::new();
        let values = binxml_values_from_py_list(substitutions, &bump)?;

        let guid = normalize_guid(&template_guid);
        let temp = self.temps_by_guid.get(&guid).cloned().ok_or_else(|| {
            PyErr::new::<PyKeyError, _>(format!("template GUID `{}` not found", guid))
        })?;

        Ok(render_temp_to_xml_with_values(temp.as_slice(), &values, codec, &bump).map_err(
            PyEvtxError,
        )?)
    }

    #[pyo3(signature = (
        evtx_path_or_file_like,
        record_id,
        template_instance_index=0,
        template_guid=None,
        provider_guid=None,
        event_id=None,
        version=None,
        ansi_codec=None
    ))]
    /// render_record_xml(self, evtx_path_or_file_like, record_id, template_instance_index=0, template_guid=None, provider_guid=None, event_id=None, version=None, ansi_codec=None, /)
    /// --
    ///
    /// End-to-end offline rendering:
    /// - Extract TemplateInstance substitution values from an EVTX record.
    /// - Resolve the template GUID (either directly, or from provider_guid/event_id/version).
    /// - Render the template to XML using the offline cache.
    fn render_record_xml(
        &self,
        evtx_path_or_file_like: Py<PyAny>,
        record_id: u64,
        template_instance_index: usize,
        template_guid: Option<String>,
        provider_guid: Option<String>,
        event_id: Option<u16>,
        version: Option<u8>,
        ansi_codec: Option<String>,
    ) -> PyResult<String> {
        let codec = resolve_ansi_codec(ansi_codec)?;

        // Resolve template GUID.
        let template_guid = if let Some(g) = template_guid {
            normalize_guid(&g)
        } else if let (Some(provider_guid), Some(event_id), Some(version)) =
            (provider_guid, event_id, version)
        {
            let key = (normalize_guid(&provider_guid), event_id, version);
            self.event_to_template_guid
                .get(&key)
                .cloned()
                .ok_or_else(|| {
                    PyErr::new::<PyKeyError, _>(format!(
                        "No template_guid found for provider_guid={} event_id={} version={}",
                        provider_guid, event_id, version
                    ))
                })?
        } else {
            return Err(PyErr::new::<PyValueError, _>(
                "Must provide template_guid, or (provider_guid, event_id, version)",
            ));
        };

        // Extract substitution values from the record.
        let file_or_file_like = FileOrFileLike::from_pyobject(evtx_path_or_file_like)?;

        let mut settings = ParserSettings::default().ansi_codec(codec);
        // Nudge deterministic behavior if any code path consults this.
        settings = settings.num_threads(1);

        let boxed_read_seek = match file_or_file_like {
            FileOrFileLike::File(s) => {
                let file = File::open(s)?;
                Box::new(file) as Box<dyn ReadSeek>
            }
            FileOrFileLike::FileLike(f) => Box::new(f) as Box<dyn ReadSeek>,
        };

        let mut parser = EvtxParser::from_read_seek(boxed_read_seek)
            .map_err(PyEvtxError)?
            .with_configuration(settings.clone());

        for (chunk_idx, chunk_res) in parser.chunks().enumerate() {
            let mut chunk_data = chunk_res.map_err(PyEvtxError)?;
            let mut chunk = chunk_data.parse(Arc::new(settings.clone())).map_err(|e| {
                PyEvtxError(evtx_rs::err::EvtxError::FailedToParseChunk {
                    chunk_id: chunk_idx as u64,
                    source: Box::new(e),
                })
            })?;

            for record_res in chunk.iter() {
                let record = record_res.map_err(PyEvtxError)?;
                if record.event_record_id != record_id {
                    continue;
                }

                let instances = record.template_instances().map_err(PyEvtxError)?;
                let instance = instances.get(template_instance_index).ok_or_else(|| {
                    PyErr::new::<PyValueError, _>(format!(
                        "Record {} has no TemplateInstance at index {}",
                        record.event_record_id, template_instance_index
                    ))
                })?;

                let temp = self
                    .temps_by_guid
                    .get(&template_guid)
                    .cloned()
                    .ok_or_else(|| {
                        PyErr::new::<PyKeyError, _>(format!(
                            "template GUID `{}` not found",
                            template_guid
                        ))
                    })?;

                return Ok(render_temp_to_xml_with_values(
                    temp.as_slice(),
                    &instance.values,
                    codec,
                    &record.chunk.arena,
                )
                .map_err(PyEvtxError)?);
            }
        }

        Err(PyErr::new::<PyValueError, _>(format!(
            "Record {} not found",
            record_id
        )))
    }
}

impl PyWevtCache {
    fn insert_temp(&mut self, template_guid: &str, temp_bytes: Arc<Vec<u8>>) {
        let guid = normalize_guid(template_guid);
        self.inner.insert_temp_bytes(&guid, Arc::clone(&temp_bytes));
        self.temps_by_guid.insert(guid, temp_bytes);
    }

    fn add_crim_blob(&mut self, data: Vec<u8>) -> PyResult<usize> {
        use evtx_rs::wevt_templates::manifest::CrimManifest;

        let data = Arc::new(data);
        let manifest =
            CrimManifest::parse(data.as_slice()).map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("{e}")))?;

        self.resources.push(PyWevtResource { data: Arc::clone(&data) });

        // Insert templates + event->template GUID mapping.
        let mut template_count = 0usize;

        for provider in &manifest.providers {
            let provider_guid = normalize_guid(&provider.guid.to_string());

            if let Some(evnt) = provider.wevt.elements.events.as_ref() {
                for ev in &evnt.events {
                    let template_guid = ev
                        .template_offset
                        .and_then(|off| provider.template_by_offset(off))
                        .map(|t| normalize_guid(&t.guid.to_string()));
                    if let Some(template_guid) = template_guid {
                        self.event_to_template_guid.insert(
                            (provider_guid.clone(), ev.identifier, ev.version),
                            template_guid,
                        );
                    }
                }
            }

            if let Some(ttbl) = provider.wevt.elements.templates.as_ref() {
                for tpl in &ttbl.templates {
                    let start = tpl.offset as usize;
                    let end = start.saturating_add(tpl.size as usize);
                    if end > data.len() {
                        return Err(PyErr::new::<PyRuntimeError, _>(
                            "TEMP slice out of bounds while building cache",
                        ));
                    }
                    let temp_bytes = data[start..end].to_vec();
                    let guid = tpl.guid.to_string();
                    self.insert_temp(&guid, Arc::new(temp_bytes));
                    template_count = template_count.saturating_add(1);
                }
            }
        }

        Ok(template_count)
    }

    fn add_pe_file(&mut self, path: PathBuf) -> PyResult<usize> {
        use evtx_rs::wevt_templates::extract_wevt_template_resources;

        let bytes = std::fs::read(&path).map_err(|e| py_err_from_io_err(&e))?;
        let resources =
            extract_wevt_template_resources(&bytes).map_err(py_err_from_wevt_extract_error)?;

        let mut template_count = 0usize;

        for res in resources {
            template_count = template_count.saturating_add(self.add_crim_blob(res.data)?);
        }

        Ok(template_count)
    }

    fn load_wevtcache_file(&mut self, path: &Path) -> PyResult<()> {
        use evtx_rs::wevt_templates::wevtcache::{EntryKind, WevtCacheReader};

        let mut reader =
            WevtCacheReader::open(path).map_err(py_err_from_wevt_cache_file_error)?;
        while let Some((kind, blob)) = reader
            .next_entry()
            .map_err(py_err_from_wevt_cache_file_error)?
        {
            match kind {
                EntryKind::Crim => {
                    let _ = self.add_crim_blob(blob)?;
                }
            }
        }
        Ok(())
    }

    fn dump_to_wevtcache_file(&self, path: &Path, overwrite: bool) -> PyResult<()> {
        use evtx_rs::wevt_templates::wevtcache::WevtCacheWriter;

        let mut writer =
            WevtCacheWriter::create(path, overwrite).map_err(py_err_from_wevt_cache_file_error)?;
        for r in &self.resources {
            writer
                .write_crim_blob(r.data.as_slice())
                .map_err(py_err_from_wevt_cache_file_error)?;
        }
        let _ = writer.finish().map_err(py_err_from_wevt_cache_file_error)?;
        Ok(())
    }
}

fn collect_input_paths(
    input: &Path,
    recursive: bool,
    allowed_exts: &std::collections::HashSet<String>,
    seen: &mut std::collections::HashSet<PathBuf>,
    out_files: &mut Vec<PathBuf>,
) -> PyResult<()> {
    use std::collections::VecDeque;

    if !input.exists() {
        return Ok(());
    }

    if input.is_file() {
        let p = input.to_path_buf();
        if seen.insert(p.clone()) {
            out_files.push(p);
        }
        return Ok(());
    }

    if input.is_dir() {
        let mut queue = VecDeque::new();
        queue.push_back(input.to_path_buf());

        while let Some(dir) = queue.pop_front() {
            let entries = std::fs::read_dir(&dir).map_err(|e| py_err_from_io_err(&e))?;
            for entry in entries {
                let entry = entry.map_err(|e| py_err_from_io_err(&e))?;
                let p = entry.path();
                if p.is_dir() {
                    if recursive {
                        queue.push_back(p);
                    }
                } else if p.is_file() && should_keep_file(&p, allowed_exts) && seen.insert(p.clone())
                {
                    out_files.push(p);
                }
            }
        }
    }

    Ok(())
}

fn should_keep_file(path: &Path, allowed_exts: &std::collections::HashSet<String>) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    allowed_exts.contains(&ext.to_ascii_lowercase())
}

pub(crate) fn wevt_cache_from_pyobject(obj: Py<PyAny>) -> PyResult<Arc<WevtCache>> {
    Python::attach(|py| {
        let bound = obj.bind(py);
        if let Some(path) = path_string_from_pyany(&bound)? {
            let path = PathBuf::from(&path);
            if path.extension().and_then(|s| s.to_str()) != Some("wevtcache") {
                return Err(PyErr::new::<PyValueError, _>(
                    "wevt_cache path must be a `.wevtcache` file",
                ));
            }

            let cache = Arc::new(WevtCache::new());
            {
                use evtx_rs::wevt_templates::wevtcache::{EntryKind, WevtCacheReader};

                let mut reader =
                    WevtCacheReader::open(&path).map_err(py_err_from_wevt_cache_file_error)?;
                while let Some((kind, blob)) = reader
                    .next_entry()
                    .map_err(py_err_from_wevt_cache_file_error)?
                {
                    match kind {
                        EntryKind::Crim => {
                            cache
                                .add_wevt_blob(Arc::new(blob))
                                .map_err(py_err_from_wevt_cache_error)?;
                        }
                    }
                }
            }
            return Ok(cache);
        }

        if let Ok(cache) = obj.downcast_bound::<PyWevtCache>(py) {
            return Ok(Arc::clone(&cache.borrow().inner));
        }

        Err(PyErr::new::<PyTypeError, _>(
            "wevt_cache must be a path (str or Path) or a WevtCache instance",
        ))
    })
}

fn resolve_ansi_codec(ansi_codec: Option<String>) -> PyResult<encoding::EncodingRef> {
    if let Some(codec) = ansi_codec {
        match encodings().iter().find(|c| c.name() == codec) {
            Some(encoding) => Ok(*encoding),
            None => Err(PyErr::new::<PyValueError, _>(format!(
                "Unknown encoding `[{}]`, see help for possible values",
                codec
            ))),
        }
    } else {
        Ok(ParserSettings::default().get_ansi_codec())
    }
}

fn binxml_values_from_py_list<'a>(
    substitutions: &Bound<'_, PyAny>,
    bump: &'a Bump,
) -> PyResult<Vec<BinXmlValue<'a>>> {
    let seq = substitutions.downcast::<PySequence>()?;
    let len = seq.len()?;
    let mut out: Vec<BinXmlValue<'a>> = Vec::with_capacity(len);

    for idx in 0..len {
        let item = seq.get_item(idx)?;

        if item.is_none() {
            out.push(BinXmlValue::NullType);
            continue;
        }

        if let Ok(b) = item.extract::<bool>() {
            out.push(BinXmlValue::BoolType(b));
            continue;
        }

        // Python ints are arbitrary precision; try i64 then u64.
        if let Ok(i) = item.extract::<i64>() {
            out.push(BinXmlValue::Int64Type(i));
            continue;
        }
        if let Ok(u) = item.extract::<u64>() {
            out.push(BinXmlValue::UInt64Type(u));
            continue;
        }

        if let Ok(f) = item.extract::<f64>() {
            out.push(BinXmlValue::Real64Type(f));
            continue;
        }

        if let Ok(s) = item.extract::<String>() {
            out.push(BinXmlValue::AnsiStringType(bump.alloc_str(&s)));
            continue;
        }

        if let Ok(b) = item.downcast::<PyBytes>() {
            let bytes = b.as_bytes();
            out.push(BinXmlValue::BinaryType(bump.alloc_slice_copy(bytes)));
            continue;
        }

        // Fallback: use `str(obj)` and treat as ANSI string.
        let s = item.str()?.to_string_lossy().to_string();
        out.push(BinXmlValue::AnsiStringType(bump.alloc_str(&s)));
    }

    Ok(out)
}

