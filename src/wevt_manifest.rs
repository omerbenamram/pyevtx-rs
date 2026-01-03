#![cfg(feature = "wevt_templates")]

use std::collections::HashMap;
use std::sync::Arc;

use encoding::all::encodings;
use encoding::EncodingRef;
use pyo3::exceptions::PyRuntimeError;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3_stub_gen::derive::*;

use evtx_rs::wevt_templates::manifest::CrimManifest;
use evtx_rs::wevt_templates::render_temp_to_xml;
use evtx_rs::wevt_templates::normalize_guid;
use evtx_rs::ParserSettings;

#[derive(Debug, Clone)]
struct CrimHeaderOwned {
    #[allow(dead_code)]
    size: u32,
    #[allow(dead_code)]
    major_version: u16,
    #[allow(dead_code)]
    minor_version: u16,
    #[allow(dead_code)]
    provider_count: u32,
}

#[derive(Debug, Clone)]
struct TemplateItemOwned {
    input_data_type: u8,
    output_data_type: u8,
    number_of_values: u16,
    value_data_size: u16,
    name: Option<String>,
}

#[derive(Debug, Clone)]
struct TemplateOwned {
    offset: u32,
    size: u32,
    identifier: String,
    items: Vec<TemplateItemOwned>,
}

#[derive(Debug, Clone)]
struct EventOwned {
    identifier: u16,
    version: u8,
    message_identifier: u32,
    template_offset: Option<u32>,
}

#[derive(Debug, Clone)]
struct ProviderOwned {
    identifier: String,
    #[allow(dead_code)]
    offset: u32,
    #[allow(dead_code)]
    message_identifier: Option<u32>,

    events: Vec<EventOwned>,
    templates: Vec<TemplateOwned>,
    template_index_by_offset: HashMap<u32, usize>,
}

#[derive(Debug)]
struct ManifestInner {
    #[allow(dead_code)]
    data: Arc<Vec<u8>>,
    #[allow(dead_code)]
    header: CrimHeaderOwned,
    providers: Vec<ProviderOwned>,
}

impl ManifestInner {
    fn provider(&self, provider_index: usize) -> &ProviderOwned {
        self.providers
            .get(provider_index)
            .expect("provider_index out of bounds")
    }
}

#[gen_stub_pyclass]
#[pyclass(name = "Manifest")]
pub struct PyWevtManifest {
    inner: Arc<ManifestInner>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyWevtManifest {
    #[staticmethod]
    /// parse(crim_blob, /)
    /// --
    ///
    /// Parse a CRIM manifest blob (the payload stored inside a `WEVT_TEMPLATE` resource).
    fn parse(crim_blob: &Bound<'_, PyBytes>) -> PyResult<Self> {
        let bytes = crim_blob.as_bytes().to_vec();
        let parsed = CrimManifest::parse(bytes.as_slice())
            .map_err(|e| PyErr::new::<PyValueError, _>(format!("{e}")))?;

        // Use the truncated slice (limited to CRIM.size) as the canonical blob.
        let data = Arc::new(parsed.data.to_vec());

        let header = CrimHeaderOwned {
            size: parsed.header.size,
            major_version: parsed.header.major_version,
            minor_version: parsed.header.minor_version,
            provider_count: parsed.header.provider_count,
        };

        let mut providers: Vec<ProviderOwned> = Vec::with_capacity(parsed.providers.len());

        for provider in &parsed.providers {
            let identifier = normalize_guid(&provider.guid.to_string());

            let mut events: Vec<EventOwned> = Vec::new();
            if let Some(evnt) = provider.wevt.elements.events.as_ref() {
                events.reserve(evnt.events.len());
                for ev in &evnt.events {
                    events.push(EventOwned {
                        identifier: ev.identifier,
                        version: ev.version,
                        message_identifier: ev.message_identifier,
                        template_offset: ev.template_offset,
                    });
                }
            }

            let mut templates: Vec<TemplateOwned> = Vec::new();
            if let Some(ttbl) = provider.wevt.elements.templates.as_ref() {
                templates.reserve(ttbl.templates.len());
                for tpl in &ttbl.templates {
                    let template_identifier = normalize_guid(&tpl.guid.to_string());
                    let items = tpl
                        .items
                        .iter()
                        .map(|item| TemplateItemOwned {
                            input_data_type: item.input_type,
                            output_data_type: item.output_type,
                            number_of_values: item.count,
                            value_data_size: item.length,
                            name: item.name.clone(),
                        })
                        .collect::<Vec<_>>();

                    templates.push(TemplateOwned {
                        offset: tpl.offset,
                        size: tpl.size,
                        identifier: template_identifier,
                        items,
                    });
                }
            }

            let template_index_by_offset = templates
                .iter()
                .enumerate()
                .map(|(i, t)| (t.offset, i))
                .collect::<HashMap<_, _>>();

            providers.push(ProviderOwned {
                identifier,
                offset: provider.offset,
                message_identifier: provider.wevt.message_identifier,
                events,
                templates,
                template_index_by_offset,
            });
        }

        Ok(Self {
            inner: Arc::new(ManifestInner {
                data,
                header,
                providers,
            }),
        })
    }

    #[getter]
    fn providers(&self) -> Vec<PyWevtProvider> {
        (0..self.inner.providers.len())
            .map(|provider_index| PyWevtProvider {
                inner: Arc::clone(&self.inner),
                provider_index,
            })
            .collect()
    }
}

#[gen_stub_pyclass]
#[pyclass(name = "Provider")]
pub struct PyWevtProvider {
    inner: Arc<ManifestInner>,
    provider_index: usize,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyWevtProvider {
    #[getter]
    fn identifier(&self) -> String {
        self.inner.provider(self.provider_index).identifier.clone()
    }

    #[getter]
    fn events(&self) -> Vec<PyWevtEvent> {
        let provider = self.inner.provider(self.provider_index);
        (0..provider.events.len())
            .map(|event_index| PyWevtEvent {
                inner: Arc::clone(&self.inner),
                provider_index: self.provider_index,
                event_index,
            })
            .collect()
    }

    #[getter]
    fn templates(&self) -> Vec<PyWevtTemplate> {
        let provider = self.inner.provider(self.provider_index);
        (0..provider.templates.len())
            .map(|template_index| PyWevtTemplate {
                inner: Arc::clone(&self.inner),
                provider_index: self.provider_index,
                template_index,
            })
            .collect()
    }

    /// get_template_by_offset(self, offset, /)
    /// --
    ///
    /// Retrieve a template by its offset (as stored in `Event.template_offset`).
    fn get_template_by_offset(&self, offset: u32) -> Option<PyWevtTemplate> {
        let provider = self.inner.provider(self.provider_index);
        let idx = provider.template_index_by_offset.get(&offset).copied()?;
        Some(PyWevtTemplate {
            inner: Arc::clone(&self.inner),
            provider_index: self.provider_index,
            template_index: idx,
        })
    }
}

#[gen_stub_pyclass]
#[pyclass(name = "Event")]
pub struct PyWevtEvent {
    inner: Arc<ManifestInner>,
    provider_index: usize,
    event_index: usize,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyWevtEvent {
    #[getter]
    fn identifier(&self) -> u16 {
        self.inner.provider(self.provider_index).events[self.event_index].identifier
    }

    #[getter]
    fn version(&self) -> u8 {
        self.inner.provider(self.provider_index).events[self.event_index].version
    }

    #[getter]
    fn message_identifier(&self) -> u32 {
        self.inner.provider(self.provider_index).events[self.event_index].message_identifier
    }

    #[getter]
    fn template_offset(&self) -> Option<u32> {
        self.inner.provider(self.provider_index).events[self.event_index].template_offset
    }
}

#[gen_stub_pyclass]
#[pyclass(name = "Template")]
pub struct PyWevtTemplate {
    inner: Arc<ManifestInner>,
    provider_index: usize,
    template_index: usize,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyWevtTemplate {
    #[getter]
    fn identifier(&self) -> String {
        self.inner.provider(self.provider_index).templates[self.template_index]
            .identifier
            .clone()
    }

    #[getter]
    fn items(&self) -> Vec<PyWevtTemplateItem> {
        let tpl = &self.inner.provider(self.provider_index).templates[self.template_index];
        (0..tpl.items.len())
            .map(|item_index| PyWevtTemplateItem {
                inner: Arc::clone(&self.inner),
                provider_index: self.provider_index,
                template_index: self.template_index,
                item_index,
            })
            .collect()
    }

    #[pyo3(signature = (ansi_codec=None))]
    /// to_xml(self, ansi_codec=None, /)
    /// --
    ///
    /// Render this template's TEMP BinXML to an XML string.
    ///
    /// This is intended for offline debugging/inspection (placeholder substitutions are rendered
    /// as `{sub:N}`).
    fn to_xml(&self, ansi_codec: Option<String>) -> PyResult<String> {
        let codec = resolve_ansi_codec(ansi_codec)?;

        let provider = self.inner.provider(self.provider_index);
        let tpl = &provider.templates[self.template_index];

        let data = self.inner.data.as_slice();
        let start = tpl.offset as usize;
        let end = start.saturating_add(tpl.size as usize);
        if end > data.len() {
            return Err(PyErr::new::<PyRuntimeError, _>(
                "TEMP slice out of bounds while rendering template",
            ));
        }

        let temp_bytes = &data[start..end];
        Ok(render_temp_to_xml(temp_bytes, codec).map_err(|e| PyErr::new::<PyRuntimeError, _>(format!("{e}")))?)
    }
}

#[gen_stub_pyclass]
#[pyclass(name = "TemplateItem")]
pub struct PyWevtTemplateItem {
    inner: Arc<ManifestInner>,
    provider_index: usize,
    template_index: usize,
    item_index: usize,
}

impl PyWevtTemplateItem {
    fn item(&self) -> &TemplateItemOwned {
        &self.inner.provider(self.provider_index).templates[self.template_index].items[self.item_index]
    }
}

fn resolve_ansi_codec(ansi_codec: Option<String>) -> PyResult<EncodingRef> {
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

#[gen_stub_pymethods]
#[pymethods]
impl PyWevtTemplateItem {
    #[getter]
    fn input_data_type(&self) -> u8 {
        self.item().input_data_type
    }

    #[getter]
    fn output_data_type(&self) -> u8 {
        self.item().output_data_type
    }

    #[getter]
    fn number_of_values(&self) -> u16 {
        self.item().number_of_values
    }

    #[getter]
    fn value_data_size(&self) -> u16 {
        self.item().value_data_size
    }

    #[getter]
    fn name(&self) -> Option<String> {
        self.item().name.clone()
    }
}

