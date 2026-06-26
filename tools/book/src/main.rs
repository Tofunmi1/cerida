use anyhow::Result;
use serde::Deserialize;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

#[derive(Deserialize, Clone)]
struct LevelJson {
    price: u64,
    size: u64,
    orders: usize,
}

#[derive(Deserialize)]
struct Response {
    ok: bool,
    best_bid: Option<String>,
    best_ask: Option<String>,
    spread: Option<u64>,
    order_count: Option<usize>,
    bids: Option<Vec<LevelJson>>,
    asks: Option<Vec<LevelJson>>,
    error: Option<String>,
}

fn main() -> Result<()> {
    let addr = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:9720".into());

    let mut stream = TcpStream::connect(&addr)?;
    let req = r#"{"cmd":"get_market"}"#;
    writeln!(stream, "{req}")?;

    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let resp: Response = serde_json::from_str(&line)?;
    if !resp.ok {
        eprintln!("Error: {}", resp.error.unwrap_or_default());
        std::process::exit(1);
    }

    let max_size = resp.bids.as_ref().iter().chain(resp.asks.as_ref().iter())
        .flat_map(|v| v.iter())
        .map(|l| l.size)
        .max()
        .unwrap_or(1);

    let order_count = resp.order_count.unwrap_or(0);
    let spread = resp.spread.unwrap_or(0);

    // ── ASKS (highest to lowest, reversed since depth returns sorted) ──
    if let Some(ref asks) = resp.asks {
        let mut asks: Vec<_> = asks.clone();
        asks.reverse();
        print_sidebar(&asks, true, max_size);
    }

    // ── Spread ──
    let mid = format!("  {order_count} orders  ·  spread: {spread}");
    let pad = " ".repeat(18);
    println!("{}┌{}┐{}", pad, "─".repeat(mid.len()), pad);
    println!("{pad}│\x1b[1m{mid}\x1b[0m│{pad}");
    println!("{}└{}┘{}", pad, "─".repeat(mid.len()), pad);

    // ── BIDS (highest to lowest) ──
    if let Some(ref bids) = resp.bids {
        print_sidebar(bids, false, max_size);
    }

    Ok(())
}

fn print_sidebar(levels: &[LevelJson], is_ask: bool, max_size: u64) {
    let bar_w = 30u64;

    for level in levels {
        let bar_len = (level.size as f64 / max_size as f64 * bar_w as f64).round() as usize;
        let bar_str = "█".repeat(bar_len);
        let empty = " ".repeat((bar_w as usize).saturating_sub(bar_len));

        let price_str = format!("{}", level.price);
        let size_str = format_size(level.size);
        let order_str = format!("({})", level.orders);

        if is_ask {
            // Asks: right-aligned, red bar on left
            println!("\x1b[31m{bar_str}{empty}\x1b[0m  \x1b[33m{:>12}\x1b[0m  \x1b[36m{:>10}\x1b[0m  \x1b[90m{:>4}\x1b[0m",
                price_str, size_str, order_str);
        } else {
            // Bids: left-aligned, green bar on right
            println!("  \x1b[33m{:>12}\x1b[0m  \x1b[36m{:>10}\x1b[0m  \x1b[90m{:>4}\x1b[0m  \x1b[32m{empty}{bar_str}\x1b[0m",
                price_str, size_str, order_str);
        }
    }

    if levels.is_empty() {
        println!("  \x1b[90m(empty)\x1b[0m");
    }
}

fn format_size(s: u64) -> String {
    if s >= 1_000_000 {
        format!("{:.1}M", s as f64 / 1_000_000.0)
    } else if s >= 1_000 {
        format!("{:.1}K", s as f64 / 1_000.0)
    } else {
        s.to_string()
    }
}

fn spread_str(s: u64) -> String {
    if s == 0 { "0".into() } else { s.to_string() }
}
