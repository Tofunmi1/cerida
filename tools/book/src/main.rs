use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};
use ratatui::Frame;

use serde::Deserialize;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

#[derive(Deserialize, Clone, Default)]
struct LevelJson {
    price: u64,
    size: u64,
    orders: usize,
}

#[derive(Deserialize, Default)]
struct BookResponse {
    ok: bool,
    bids: Option<Vec<LevelJson>>,
    asks: Option<Vec<LevelJson>>,
    spread: Option<u64>,
    order_count: Option<usize>,
    error: Option<String>,
}

struct App {
    addr: String,
    asks: Vec<LevelJson>,
    bids: Vec<LevelJson>,
    spread: u64,
    order_count: usize,
    max_size: u64,
    error: Option<String>,
    last_updated: Instant,
}

impl App {
    fn new(addr: &str) -> Self {
        Self {
            addr: addr.to_string(),
            asks: Vec::new(),
            bids: Vec::new(),
            spread: 0,
            order_count: 0,
            max_size: 1,
            error: None,
            last_updated: Instant::now(),
        }
    }

    fn refresh(&mut self) {
        let mut stream = match TcpStream::connect_timeout(
            &self.addr.parse().unwrap(),
            Duration::from_secs(2),
        ) {
            Ok(s) => {
                s.set_read_timeout(Some(Duration::from_secs(2))).ok();
                s
            }
            Err(e) => {
                self.error = Some(format!("connect: {e}"));
                return;
            }
        };

        if writeln!(stream, r#"{{"cmd":"get_market"}}"#).is_err() {
            self.error = Some("write failed".into());
            return;
        }

        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => {
                self.error = Some("no response from server".into());
                return;
            }
            Ok(_) => {}
        }

        let resp: BookResponse = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(format!("parse: {e}"));
                return;
            }
        };

        if !resp.ok {
            self.error = Some(resp.error.unwrap_or_else(|| "unknown error".into()));
            return;
        }

        let bids = resp.bids.unwrap_or_default();
        let mut asks = resp.asks.unwrap_or_default();
        asks.reverse();

        let max_size = bids.iter().chain(asks.iter()).map(|l| l.size).max().unwrap_or(1);

        self.asks = asks;
        self.bids = bids;
        self.spread = resp.spread.unwrap_or(0);
        self.order_count = resp.order_count.unwrap_or(0);
        self.max_size = max_size.max(1);
        self.error = None;
        self.last_updated = Instant::now();
    }
}

fn main() -> Result<()> {
    let addr = std::env::args().nth(1).unwrap_or_else(|| "127.0.0.1:9720".into());

    let mut terminal = ratatui::try_init()
        .map_err(|e| anyhow::anyhow!("Failed to initialize terminal (run in a real terminal, not a headless environment): {e}"))?;
    let mut app = App::new(&addr);
    let tick = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        // Poll for refresh
        if last_tick.elapsed() >= tick {
            app.refresh();
            last_tick = Instant::now();
        }

        terminal.draw(|f| ui(f, &app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                    break;
                }
            }
        }
    }

    ratatui::restore();
    Ok(())
}

fn ui(f: &mut Frame, app: &App) {
    let area = f.area();
    let _total_rows = area.height as usize;

    // Reserve a few lines for header + spread + footer
    let header_h = 3u16;
    let spread_h = 3u16;
    let footer_h = 1u16;
    let side_h = (area.height.saturating_sub(header_h + spread_h + footer_h)) / 2;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(header_h),
            Constraint::Min(side_h),
            Constraint::Length(spread_h),
            Constraint::Min(side_h),
            Constraint::Length(footer_h),
        ])
        .split(area);

    render_header(f, chunks[0], app);
    render_side(f, chunks[1], &app.asks, true, app.max_size);
    render_spread(f, chunks[2], app);
    render_side(f, chunks[3], &app.bids, false, app.max_size);
    render_footer(f, chunks[4], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let last = app.last_updated.elapsed().as_millis();
    let title = Line::from(vec![
        Span::styled(" CLOB Orderbook ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(format!("| {} orders ", app.order_count), Style::default().fg(Color::Yellow)),
        Span::styled(format!("| {}ms ago", last), Style::default().fg(Color::DarkGray)),
    ]);
    let block = Block::default()
        .title(title)
        .borders(Borders::BOTTOM)
        .border_set(border::THICK);
    f.render_widget(block, area);
}

fn render_side(f: &mut Frame, area: Rect, levels: &[LevelJson], is_ask: bool, max_size: u64) {
    let bar_max = area.width.saturating_sub(42) as usize;

    let rows: Vec<Row> = levels.iter().map(|l| {
        let pct = if max_size > 0 { l.size as f64 / max_size as f64 } else { 0.0 };
        let bar_len = (pct * bar_max as f64).round() as usize;
        let bar_fill = "█".repeat(bar_len.min(bar_max));
        let bar_empty = " ".repeat(bar_max.saturating_sub(bar_len));

        let price_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
        let size_style = Style::default().fg(Color::Cyan);
        let order_style = Style::default().fg(Color::DarkGray);
        let bar_color = if is_ask { Color::Red } else { Color::Green };

        let line = if is_ask {
            Line::from(vec![
                Span::styled(bar_fill, Style::default().fg(bar_color)),
                Span::styled(bar_empty, Style::default().bg(Color::Black)),
                Span::raw("│"),
                Span::styled(format!("{:>12}", l.price), price_style),
                Span::raw(" "),
                Span::styled(format_size(l.size), size_style),
                Span::raw(" "),
                Span::styled(format!("{:>3}", l.orders), order_style),
            ])
        } else {
            Line::from(vec![
                Span::styled(format!("{:>12}", l.price), price_style),
                Span::raw(" "),
                Span::styled(format_size(l.size), size_style),
                Span::raw(" "),
                Span::styled(format!("{:>3}", l.orders), order_style),
                Span::raw("│"),
                Span::styled(bar_empty, Style::default().bg(Color::Black)),
                Span::styled(bar_fill, Style::default().fg(bar_color)),
            ])
        };
        Row::new(vec![line])
    }).collect();

    let title = format!(" {} (depth) ", if is_ask { "ASKS" } else { "BIDS" });
    let title_color = if is_ask { Color::Red } else { Color::Green };
    let block = Block::default()
        .title(Line::from(Span::styled(title, Style::default().fg(title_color).add_modifier(Modifier::BOLD))))
        .borders(Borders::NONE);

    let table = Table::new(rows, [Constraint::Fill(1)])
        .block(block);
    f.render_widget(table, area);
}

fn render_spread(f: &mut Frame, area: Rect, app: &App) {
    let text = format!("  spread: {}  ", app.spread);
    let block = Block::default()
        .title(Line::from(Span::styled(text, Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))))
        .borders(Borders::TOP)
        .border_set(border::THICK);
    f.render_widget(block, area);
}

fn render_footer(f: &mut Frame, area: Rect, _app: &App) {
    let text = Line::from(Span::styled(" [q] quit ", Style::default().fg(Color::DarkGray)));
    f.render_widget(Paragraph::new(text), area);
}

fn format_size(s: u64) -> String {
    if s >= 1_000_000 {
        format!("{:>6.1}M", s as f64 / 1_000_000.0)
    } else if s >= 1_000 {
        format!("{:>6.1}K", s as f64 / 1_000.0)
    } else {
        format!("{:>6}", s)
    }
}
