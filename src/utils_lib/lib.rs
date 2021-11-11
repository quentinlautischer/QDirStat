use std::io::{Write};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

fn color_log(msg: &str, color: Color) -> std::io::Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    stdout.set_color(ColorSpec::new().set_fg(Some(color)))?;
    writeln!(&mut stdout, "{}", msg)
}

fn reset_color() -> std::io::Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::White)))
}

pub fn log(msg: &str) {
    color_log(msg, Color::White).expect("Failed to log");
    reset_color().expect("Failed to reset log");
}

pub fn log_i(msg: &str) {
    color_log(msg, Color::Blue).expect("Failed to log");
    reset_color().expect("Failed to reset log");
}

pub fn log_s(msg: &str) {
    color_log(msg, Color::Green).expect("Failed to log");
    reset_color().expect("Failed to reset log");
}

pub fn log_w(msg: &str) {
    color_log(msg, Color::Yellow).expect("Failed to log");
    reset_color().expect("Failed to reset log");
}

pub fn log_e(msg: &str) {
    color_log(msg, Color::Red).expect("Failed to log");
    reset_color().expect("Failed to reset log");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_methods() {
        log("Normal");
        log_i("Important");
        log_s("Success");
        log_w("Warning");
        log_e("Error");
    }
}
