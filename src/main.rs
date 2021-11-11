extern crate utils;
mod qdirstat;

// Set backtrace with export RUST_BACKTRACE=1(unix) or set RUST_BACKTRACE=1(cmd) or $Env:RUST_BACKTRACE=1(powershell)
#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    qdirstat::run();

    Ok(())
}