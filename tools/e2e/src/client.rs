use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Instant;

#[derive(Serialize, Default)]
pub struct Request {
    cmd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    side: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    leverage: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    asset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nonce: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    secret: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cmt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    out: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cmt_a: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cmt_b: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    perp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_type: Option<String>,
}

#[derive(Deserialize)]
struct Response {
    ok: bool,
    commitment: Option<String>,
    match_price: Option<String>,
    match_size: Option<String>,
    nullifier_a: Option<String>,
    nullifier_b: Option<String>,
    fills: Option<Vec<FillJson>>,
    best_bid: Option<String>,
    best_ask: Option<String>,
    spread: Option<u64>,
    order_count: Option<usize>,
    depth: Option<Vec<LevelJson>>,
    bids: Option<Vec<LevelJson>>,
    asks: Option<Vec<LevelJson>>,
    error: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct FillJson {
    pub maker_id: String,
    pub price: u64,
    pub size: u64,
    pub match_price: Option<String>,
    pub match_size: Option<String>,
    pub nullifier_a: Option<String>,
    pub nullifier_b: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct LevelJson {
    pub price: u64,
    pub size: u64,
    pub orders: usize,
}

pub struct MarketResponse {
    pub best_bid: Option<String>,
    pub best_ask: Option<String>,
    pub spread: Option<u64>,
    pub order_count: usize,
    pub fills: Option<Vec<FillJson>>,
    pub depth: Option<Vec<LevelJson>>,
    pub bids: Option<Vec<LevelJson>>,
    pub asks: Option<Vec<LevelJson>>,
}

pub struct MatchResult {
    pub match_price: String,
    pub match_size: String,
    pub nullifier_a: String,
    pub nullifier_b: String,
}

pub struct ServerClient {
    addr: String,
    pub perp: Option<String>,
    pub source: Option<String>,
}

impl ServerClient {
    pub fn new(addr: &str) -> Self {
        eprintln!("  [client] Connecting to tee-match server at {addr}");
        Self { addr: addr.to_string(), perp: None, source: None }
    }

    pub fn set_onchain(&mut self, perp: &str, source: &str) {
        self.perp = Some(perp.to_string());
        self.source = Some(source.to_string());
    }

    fn send(&self, req: &Request) -> Result<Response> {
        let start = Instant::now();
        eprintln!("  [client] → sending cmd={} to server…", req.cmd);

        let mut stream = TcpStream::connect(&self.addr)?;
        let json = serde_json::to_string(req)?;
        eprintln!("  [client]   request: {} bytes", json.len());

        write!(stream, "{json}\n")?;

        let mut line = String::new();
        let mut reader = BufReader::new(&stream);
        reader.read_line(&mut line)?;

        let elapsed = start.elapsed();
        eprintln!("  [client] ← response received ({} ms, {} bytes):",
            elapsed.as_millis(), line.len());
        if line.len() > 200 {
            eprintln!("  [client]   preview: {}…", &line[..200]);
        }

        let resp: Response = serde_json::from_str(&line)?;
        if !resp.ok {
            eprintln!("  [client] ✗ server returned error: {}", resp.error.as_deref().unwrap_or("unknown"));
            anyhow::bail!("server error: {}", resp.error.as_deref().unwrap_or("unknown"));
        }

        eprintln!("  [client] ✓ cmd={} succeeded ({} ms)", req.cmd, elapsed.as_millis());
        Ok(resp)
    }

    pub fn init(
        &self,
        side: u64, price: u64, size: u64, leverage: u64, asset: u64,
        nonce: u64, secret: u64,
    ) -> Result<String> {
        eprintln!("  [client] init: side={} price={} size={} leverage={} nonce={}",
            side, price, size, leverage, nonce);
        let req = Request {
            cmd: "init".to_string(),
            side: Some(side), price: Some(price), size: Some(size),
            leverage: Some(leverage), asset: Some(asset),
            nonce: Some(nonce), secret: Some(secret),
            ..Default::default()
        };
        let resp = self.send(&req)?;
        let cmt = resp.commitment.ok_or_else(|| anyhow::anyhow!("no commitment in response"))?;
        eprintln!("  [client] commitment: {} ({} hex chars)", &cmt[..16], cmt.len());
        Ok(cmt)
    }

    pub fn commit_proof(&self, cmt: &str, out: &Path) -> Result<()> {
        eprintln!("  [client] commit-proof: cmt={}… → {}",
            &cmt[..16], out.display());
        let req = Request {
            cmd: "commit-proof".to_string(),
            cmt: Some(cmt.to_string()),
            out: Some(out.to_string_lossy().to_string()),
            ..Default::default()
        };
        self.send(&req)?;
        // Verify file was written
        let meta = std::fs::metadata(out).ok();
        if let Some(m) = meta {
            eprintln!("  [client] proof file on disk: {} bytes", m.len());
        }
        Ok(())
    }

    /// Init without verbose logging (for batch operations)
    pub fn init_raw(
        &self,
        side: u64, price: u64, size: u64, leverage: u64, asset: u64,
        nonce: u64, secret: u64,
    ) -> Result<String> {
        let req = Request {
            cmd: "init".to_string(),
            side: Some(side), price: Some(price), size: Some(size),
            leverage: Some(leverage), asset: Some(asset),
            nonce: Some(nonce), secret: Some(secret),
            ..Default::default()
        };
        let resp = self.send(&req)?;
        resp.commitment.ok_or_else(|| anyhow::anyhow!("no commitment in response"))
    }

    /// Place a limit order in the CLOB engine via server
    pub fn place_order(&self, cmt: &str, order_type: &str, price: u64, size: u64) -> Result<MarketResponse> {
        let req = Request {
            cmd: "place".to_string(),
            cmt: Some(cmt.to_string()),
            order_type: Some(order_type.to_string()),
            price: Some(price),
            size: Some(size),
            perp: self.perp.clone(),
            source: self.source.clone(),
            ..Default::default()
        };
        let resp = self.send(&req)?;
        Ok(MarketResponse {
            best_bid: resp.best_bid,
            best_ask: resp.best_ask,
            spread: resp.spread,
            order_count: resp.order_count.unwrap_or(0),
            fills: resp.fills,
            depth: resp.depth,
            bids: resp.bids,
            asks: resp.asks,
        })
    }

    /// Place a market order in the CLOB engine via server
    pub fn place_market(&self, cmt: &str, size: u64) -> Result<MarketResponse> {
        let req = Request {
            cmd: "market".to_string(),
            cmt: Some(cmt.to_string()),
            size: Some(size),
            perp: self.perp.clone(),
            source: self.source.clone(),
            ..Default::default()
        };
        let resp = self.send(&req)?;
        Ok(MarketResponse {
            best_bid: resp.best_bid,
            best_ask: resp.best_ask,
            spread: resp.spread,
            order_count: resp.order_count.unwrap_or(0),
            fills: resp.fills,
            depth: resp.depth,
            bids: resp.bids,
            asks: resp.asks,
        })
    }

    /// Get current market state from CLOB engine
    pub fn get_market(&self) -> Result<MarketResponse> {
        let req = Request {
            cmd: "get_market".to_string(),
            ..Default::default()
        };
        let resp = self.send(&req)?;
        Ok(MarketResponse {
            best_bid: resp.best_bid,
            best_ask: resp.best_ask,
            spread: resp.spread,
            order_count: resp.order_count.unwrap_or(0),
            fills: resp.fills,
            depth: resp.depth,
            bids: resp.bids,
            asks: resp.asks,
        })
    }

    pub fn match_orders(&self, cmt_a: &str, cmt_b: &str, perp: &str, source: &str) -> Result<MatchResult> {
        eprintln!("  [client] match: cmt_a={}… cmt_b={}… perp={} source={}",
            &cmt_a[..16], &cmt_b[..16], &perp[..8], source);
        let req = Request {
            cmd: "match".to_string(),
            cmt_a: Some(cmt_a.to_string()),
            cmt_b: Some(cmt_b.to_string()),
            perp: Some(perp.to_string()),
            source: Some(source.to_string()),
            ..Default::default()
        };
        let resp = self.send(&req)?;
        let result = MatchResult {
            match_price: resp.match_price.ok_or_else(|| anyhow::anyhow!("no match_price in response"))?,
            match_size: resp.match_size.ok_or_else(|| anyhow::anyhow!("no match_size in response"))?,
            nullifier_a: resp.nullifier_a.ok_or_else(|| anyhow::anyhow!("no nullifier_a in response"))?,
            nullifier_b: resp.nullifier_b.ok_or_else(|| anyhow::anyhow!("no nullifier_b in response"))?,
        };
        eprintln!("  [client] match result: price={} size={} nf_a={}… nf_b={}…",
            &result.match_price[..16], &result.match_size[..16],
            &result.nullifier_a[..16], &result.nullifier_b[..16]);
        Ok(result)
    }
}
