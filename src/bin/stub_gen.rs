use pyo3_stub_gen::Result;

fn main() -> Result<()> {
    let stub = evtx::stub_info()?;
    stub.generate()?;
    Ok(())
}
