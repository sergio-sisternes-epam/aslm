use pyo3::prelude::*;

/// Python bindings for the SML parser.
/// Placeholder — full implementation in Phase 4.
#[pyfunction]
fn parse_sml(input: &str) -> PyResult<String> {
    match sml_core::parse(input) {
        Ok(doc) => Ok(format!("{doc:?}")),
        Err(e) => Err(pyo3::exceptions::PyValueError::new_err(e.to_string())),
    }
}

#[pymodule]
fn sml(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_sml, m)?)?;
    Ok(())
}
