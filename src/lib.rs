#![allow(clippy::new_ret_no_self)]
#![deny(unused_must_use)]
#![cfg_attr(not(debug_assertions), deny(clippy::dbg_macro))]

mod doc;
mod file_like;
mod parser;
mod py_err;
mod records;

#[cfg(feature = "wevt_templates")]
mod wevt_cache;

use pyo3::prelude::*;
use pyo3_stub_gen::define_stub_info_gatherer;

#[pymodule]
fn evtx(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<crate::parser::PyEvtxParser>()?;
    m.add_class::<crate::records::PyRecordsIterator>()?;
    #[cfg(feature = "wevt_templates")]
    m.add_class::<crate::wevt_cache::PyWevtCache>()?;
    Ok(())
}

define_stub_info_gatherer!(stub_info);
