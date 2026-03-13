use anyhow::Result;

fn main() -> Result<()> {
    println!("humu v{}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
