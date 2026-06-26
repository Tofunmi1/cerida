use std::fmt::Display;

fn style(s: &str, code: &str) -> String {
    format!("\x1b[{code}m{s}\x1b[0m")
}

pub(crate) fn bold(s: &str) -> String   { style(s, "1") }
pub(crate) fn dim(s: &str) -> String    { style(s, "2") }
pub(crate) fn cyan(s: &str) -> String   { style(s, "36") }
pub(crate) fn yellow(s: &str) -> String { style(s, "33") }

pub(crate) fn bg_red() -> &'static str    { "\x1b[41m\x1b[37m" }
pub(crate) fn bg_yellow() -> &'static str { "\x1b[43m\x1b[30m" }
pub(crate) fn bg_green() -> &'static str  { "\x1b[42m\x1b[30m" }
pub(crate) fn bg_cyan() -> &'static str   { "\x1b[46m\x1b[30m" }

pub(crate) fn badge_bg(bg: &str, level: &str) -> String {
    format!("{bg}{level}\x1b[0m")
}

pub(crate) fn timestamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = now.as_secs();
    let millis = now.subsec_millis();
    let rem = total_secs % 86400;
    style(&format!("{:02}:{:02}:{:02}.{:03}", rem / 3600, (rem / 60) % 60, rem % 60, millis), "2")
}

pub fn kv<V: Display>(k: &str, v: V) -> String {
    format!(" {} {}", dim(&format!("{k}=")), cyan(&v.to_string()))
}

macro_rules! log {
    ($level:expr, $bg:expr, $msg_color:expr, $msg:expr $(, $k:expr, $v:expr)*) => {{
        let ts = $crate::log::timestamp();
        let badge = $crate::log::badge_bg($bg, $level);
        let msg = $msg_color($msg);
        #[allow(unused_mut)]
        let mut out = format!("{} {} {}", ts, badge, msg);
        $(
            out.push_str(&$crate::log::kv($k, $v));
        )*
        eprintln!("{out}");
    }};
}
pub(crate) use log;

macro_rules! error {
    ($msg:expr $(, $k:expr, $v:expr)*) => {
        $crate::log::log!("ERRO", $crate::log::bg_red(), $crate::log::bold, $msg $(, $k, $v)*)
    };
}
pub(crate) use error;

macro_rules! warning {
    ($msg:expr $(, $k:expr, $v:expr)*) => {
        $crate::log::log!("WARN", $crate::log::bg_yellow(), $crate::log::yellow, $msg $(, $k, $v)*)
    };
}
pub(crate) use warning;

macro_rules! info {
    ($msg:expr $(, $k:expr, $v:expr)*) => {
        $crate::log::log!("INFO", $crate::log::bg_green(), $crate::log::bold, $msg $(, $k, $v)*)
    };
}
pub(crate) use info;

macro_rules! debug {
    ($msg:expr $(, $k:expr, $v:expr)*) => {
        $crate::log::log!("DEBUG", $crate::log::bg_cyan(), $crate::log::dim, $msg $(, $k, $v)*)
    };
}
pub(crate) use debug;
