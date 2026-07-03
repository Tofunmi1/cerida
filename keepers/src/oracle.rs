// ── Pyth Network Oracle ───────────────────────────────────────────
// Fetches real-time prices from Pyth Hermes REST API.
// Maps Pyth feed IDs to our 7-decimal price scale (1e7 = $1).
// ─────────────────────────────────────────────────────────────────

use anyhow::{anyhow, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

pub const PRICE_SCALE: f64 = 1e7;
const HERMES: &str = "https://hermes.pyth.network/v2/updates/price/latest";

#[derive(Deserialize)]
struct HermesResp {
    parsed: Vec<ParsedFeed>,
}

#[derive(Deserialize)]
struct ParsedFeed {
    id: String,
    price: FeedPrice,
}

#[derive(Deserialize)]
struct FeedPrice {
    price: String,
    expo: i32,
    publish_time: u64,
}

#[derive(Debug, Clone)]
pub struct PricePoint {
    pub usd: f64,
    pub scaled: u64,
    pub publish_time: u64,
}

/// Fetch prices for a list of Pyth feed IDs.
/// Returns map of feed_id (hex without 0x) → PricePoint.
pub fn fetch(pyth_ids: &[&str]) -> Result<HashMap<String, PricePoint>> {
    let valid: Vec<&str> = pyth_ids.iter().copied().filter(|id| !id.is_empty()).collect();
    if valid.is_empty() {
        return Ok(HashMap::new());
    }

    let query: String = valid.iter()
        .map(|id| format!("ids[]={}", id))
        .collect::<Vec<_>>()
        .join("&");

    let url = format!("{}?{}", HERMES, query);
    let client = Client::builder().timeout(Duration::from_secs(10)).build()?;
    let resp = client.get(&url).send()?.error_for_status()?;
    let data: HermesResp = resp.json()?;

    let mut out = HashMap::new();
    for feed in data.parsed {
        let raw: i64 = feed.price.price.parse()
            .map_err(|_| anyhow!("bad price string: {}", feed.price.price))?;
        let price_usd = raw as f64 * 10f64.powi(feed.price.expo);
        if price_usd <= 0.0 {
            continue;
        }
        let scaled = (price_usd * PRICE_SCALE) as u64;
        out.insert(feed.id, PricePoint { usd: price_usd, scaled, publish_time: feed.price.publish_time });
    }

    Ok(out)
}
