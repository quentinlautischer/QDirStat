mod qdirstat;

// Set backtrace with export RUST_BACKTRACE=1(unix) or set RUST_BACKTRACE=1(cmd) or $Env:RUST_BACKTRACE=1(powershell)
fn main() {
    qdirstat::run();
}
