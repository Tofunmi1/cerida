fn style(s: &str, code: &str) -> String {
    format!("\x1b[{code}m{s}\x1b[0m")
}

pub fn dim(s: &str) -> String    { style(s, "2") }
pub fn green(s: &str) -> String   { style(s, "32") }
pub fn cyan(s: &str) -> String    { style(s, "36") }
#[allow(dead_code)]
pub fn yellow(s: &str) -> String  { style(s, "33") }
pub fn red(s: &str) -> String     { style(s, "31") }
pub fn bold(s: &str) -> String    { style(s, "1") }

pub fn ok(msg: &str) {
    eprintln!("  {} {}", green("✓"), msg);
}

pub fn info(msg: &str) {
    eprintln!("  {} {}", dim("→"), msg);
}

pub fn value(label: &str, val: &str) {
    eprintln!("  {} {} {}", dim("•"), label, cyan(val));
}

pub fn header(msg: &str) {
    eprintln!("{}", bold(msg));
}
