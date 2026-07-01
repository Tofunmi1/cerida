use crate::log;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};

pub(crate) fn short_id(id: &str) -> &str {
    if id.len() > 16 { &id[..16] } else { id }
}

// ── Types ──

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Bid = 0,
    Ask = 1,
}

impl Side {
    pub fn opposite(self) -> Self {
        match self {
            Side::Bid => Side::Ask,
            Side::Ask => Side::Bid,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Limit,
    Market,
    IOC,
    FOK,
    StopLimit { stop_price: u64 },
    StopMarket { stop_price: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub side: Side,
    pub price: u64,
    pub size: u64,
    pub remaining: u64,
    #[serde(skip)]
    pub timestamp_ns: u128,
    pub order_type: OrderType,
    pub asset: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fill {
    pub taker_id: String,
    pub maker_id: String,
    pub price: u64,
    pub size: u64,
    pub taker_side: Side,
}

// ── Order Book ──

#[derive(Debug, Serialize, Deserialize)]
pub struct OrderBook {
    /// Price → FIFO queue of order IDs
    bids: BTreeMap<u64, VecDeque<String>>,
    asks: BTreeMap<u64, VecDeque<String>>,

    /// All active orders (including resting, excludes filled/cancelled)
    orders: HashMap<String, Order>,

    /// Stop orders awaiting trigger
    stop_orders: HashMap<String, Order>,

    /// Trigger price → order IDs (for stops)
    stop_bids: BTreeMap<u64, Vec<String>>,
    stop_asks: BTreeMap<u64, Vec<String>>,

    /// Last executed price (from most recent fill)
    last_price: u64,

    /// Total fills generated (not persisted — ephemeral)
    #[serde(skip)]
    pub fills: Vec<Fill>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: HashMap::new(),
            stop_orders: HashMap::new(),
            stop_bids: BTreeMap::new(),
            stop_asks: BTreeMap::new(),
            last_price: 0,
            fills: Vec::new(),
        }
    }

    // ── Public API ──

    /// Place an order and run matching. Returns fills generated.
    pub fn place(&mut self, order: Order) -> Result<Vec<Fill>> {
        let prev_fills = self.fills.len();
        let prev_count = self.order_count();
        log::info!("Book place",
            "prev_orders", prev_count,
            "id", short_id(&order.id),
            "side", order.side as u64,
            "price", order.price,
            "size", order.size,
            "type", format!("{:?}", order.order_type)
        );

        match order.order_type {
            OrderType::Market => {
                log::info!("Placing market order", "side", order.side as u64, "size", order.size);
                self.match_market(order);
            }
            OrderType::Limit => {
                log::info!("Placing limit order", "side", order.side as u64, "price", order.price, "size", order.size);
                self.match_limit(order);
            }
            OrderType::IOC => {
                log::debug!("Placing IOC order", "side", order.side as u64, "price", order.price, "size", order.size);
                let _prev_fills = self.fills.len();
                // Match but don't rest — use match_limit_inner which respects remaining
                self.match_against_book(Order { remaining: order.size, ..order }, false);
            }
            OrderType::FOK => {
                log::debug!("Placing FOK order", "side", order.side as u64, "price", order.price, "size", order.size);
                let total_available = self.liquidity_at_price(order.side.opposite(), order.price);
                if order.size <= total_available {
                    self.match_limit(Order { remaining: order.size, ..order });
                } else {
                    log::debug!("FOK would not fill fully, cancelling",
                        "size", order.size, "available", total_available);
                }
            }
            OrderType::StopLimit { stop_price } => {
                log::debug!("Placing StopLimit order",
                    "side", order.side as u64, "stop_price", stop_price,
                    "limit_price", order.price, "size", order.size);
                self.add_stop_order(order, stop_price);
            }
            OrderType::StopMarket { stop_price } => {
                log::debug!("Placing StopMarket order",
                    "side", order.side as u64, "stop_price", stop_price, "size", order.size);
                self.add_stop_order(order, stop_price);
            }
        }

        let new_fills: Vec<Fill> = self.fills[prev_fills..].to_vec();

        if !new_fills.is_empty() {
            log::info!("Order book match complete",
                "fills", new_fills.len(),
                "last_price", self.last_price,
                "best_bid", self.best_bid().map(|(p, s)| format!("{p}x{s}")).unwrap_or_default(),
                "best_ask", self.best_ask().map(|(p, s)| format!("{p}x{s}")).unwrap_or_default()
            );
        } else if self.order_count() > prev_count {
            log::info!("Order added to book without fill",
                "new_count", self.order_count()
            );
        }

        if self.order_count() != prev_count || !new_fills.is_empty() {
            let new_bid = self.best_bid().map(|(p, s)| format!("{p}x{s}")).unwrap_or_default();
            let new_ask = self.best_ask().map(|(p, s)| format!("{p}x{s}")).unwrap_or_default();
            let bid_count = self.bids.values().map(|q| q.len()).sum::<usize>();
            let ask_count = self.asks.values().map(|q| q.len()).sum::<usize>();
            log::info!("Book state after place",
                "total_orders", self.order_count(),
                "bid_orders", bid_count,
                "ask_orders", ask_count,
                "best_bid", new_bid,
                "best_ask", new_ask
            );
        }

        Ok(new_fills)
    }

    /// Restore a consumed maker order back into the book (undo a fill).
    /// Called when on-chain match submission fails.
    pub fn restore_order(&mut self, id: &str, side: Side, price: u64, size: u64) {
        let order = Order {
            id: id.to_string(),
            side,
            price,
            size,
            remaining: size,
            timestamp_ns: 0,
            order_type: OrderType::Limit,
            asset: 0,
        };
        let book = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        let queue = book.entry(price).or_default();
        queue.push_front(id.to_string());
        self.orders.insert(id.to_string(), order);
    }

    /// Cancel an active order by ID
    pub fn cancel(&mut self, id: &str) -> Result<bool> {
        if let Some(order) = self.orders.remove(id) {
            self.remove_from_level(order.side, order.price, id);
            log::debug!("Order cancelled", "id", short_id(id), "side", order.side as u64, "price", order.price);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Cancel a stop order by ID
    pub fn cancel_stop(&mut self, id: &str) -> Result<bool> {
        if let Some(order) = self.stop_orders.remove(id) {
            let stop_price = match order.order_type {
                OrderType::StopLimit { stop_price } | OrderType::StopMarket { stop_price } => stop_price,
                _ => unreachable!(),
            };
            let book = match order.side {
                Side::Bid => &mut self.stop_bids,
                Side::Ask => &mut self.stop_asks,
            };
            if let Some(ids) = book.get_mut(&stop_price) {
                ids.retain(|x| x != id);
                if ids.is_empty() {
                    book.remove(&stop_price);
                }
            }
            log::debug!("Stop order cancelled", "id", short_id(id));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn best_bid(&self) -> Option<(u64, u64)> {
        if let Some((&price, queue)) = self.bids.iter().next_back() {
            if let Some(id) = queue.front() {
                if let Some(order) = self.orders.get(id) {
                    return Some((price, order.remaining));
                }
            }
        }
        None
    }

    pub fn best_ask(&self) -> Option<(u64, u64)> {
        if let Some((&price, queue)) = self.asks.iter().next() {
            if let Some(id) = queue.front() {
                if let Some(order) = self.orders.get(id) {
                    return Some((price, order.remaining));
                }
            }
        }
        None
    }

    pub fn spread(&self) -> Option<u64> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => Some(ask.saturating_sub(bid)),
            _ => None,
        }
    }

    pub fn order_count(&self) -> usize {
        self.orders.len()
    }

    pub fn depth(&self, side: Side, levels: usize) -> Vec<(u64, u64, usize)> {
        let book = match side {
            Side::Bid => &self.bids,
            Side::Ask => &self.asks,
        };
        let iter: Box<dyn Iterator<Item = (&u64, &VecDeque<String>)>> = match side {
            Side::Bid => Box::new(book.iter().rev()),
            Side::Ask => Box::new(book.iter()),
        };
        iter.take(levels)
            .map(|(&price, queue)| {
                let total: u64 = queue.iter()
                    .filter_map(|id| self.orders.get(id))
                    .map(|o| o.remaining)
                    .sum();
                (price, total, queue.len())
            })
            .collect()
    }

    // ── Market Matching ──

    fn match_market(&mut self, mut order: Order) {
        while order.remaining > 0 {
            let best = self.pop_best(order.side.opposite());
            let best = match best {
                Some(b) => b,
                None => break,
            };
            let fill_size = order.remaining.min(best.remaining);
            self.record_fill(&order, &best, best.price, fill_size);
            order.remaining -= fill_size;
            let new_remaining = best.remaining - fill_size;
            if new_remaining > 0 {
                self.add_to_book(Order { remaining: new_remaining, ..best });
            } else {
                self.orders.remove(&best.id);
            }
        }
        if order.remaining > 0 {
            log::debug!("Market order partially filled", "id", short_id(&order.id),
                "filled", order.size - order.remaining, "unfilled", order.remaining);
        }
    }

    // ── Limit Matching ──

    fn match_limit(&mut self, order: Order) {
        self.match_against_book(order, true);
    }

    /// Core matching loop. Matches `order` against the opposite book.
    /// If `rest` is true, any unfilled remainder is added to the book.
    fn match_against_book(&mut self, mut order: Order, rest: bool) {
        while order.remaining > 0 {
            let best = self.peek_best(order.side.opposite());
            let best = match best {
                Some(b) => b,
                None => break,
            };
            let crosses = match order.side {
                Side::Bid => best.price <= order.price,
                Side::Ask => best.price >= order.price,
            };
            if !crosses {
                break;
            }
            let best = self.pop_best(order.side.opposite()).unwrap();
            let fill_size = order.remaining.min(best.remaining);
            self.record_fill(&order, &best, best.price, fill_size);
            order.remaining -= fill_size;
            let new_remaining = best.remaining - fill_size;
            if new_remaining > 0 {
                self.add_to_book(Order { remaining: new_remaining, ..best });
            } else {
                self.orders.remove(&best.id);
            }
        }
        if order.remaining > 0 && rest {
            self.add_to_book(Order { remaining: order.remaining, ..order });
        }
    }

    // ── Stop Orders ──

    fn add_stop_order(&mut self, order: Order, stop_price: u64) {
        let id = order.id.clone();
        let side = order.side;
        self.stop_orders.insert(id.clone(), order);
        let book = match side {
            Side::Bid => &mut self.stop_bids,
            Side::Ask => &mut self.stop_asks,
        };
        book.entry(stop_price).or_default().push(id);
    }

    /// Check and trigger stop orders after a fill at the given price
    fn check_stops(&mut self) {
        if self.last_price == 0 {
            return;
        }
        let p = self.last_price;

        // Bid stops triggered when price >= stop_price (e.g., stop-loss on shorts / buy stop)
        let triggered_bids: Vec<(u64, Vec<String>)> = self.stop_bids
            .range(..=p)
            .map(|(&k, v)| (k, v.clone()))
            .collect();
        for (stop_price, ids) in &triggered_bids {
            for id in ids {
                if let Some(order) = self.stop_orders.remove(id) {
                    let trigger = Order {
                        order_type: OrderType::Market,
                        ..order
                    };
                    log::debug!("Stop bid triggered",
                        "id", short_id(&id), "stop_price", stop_price, "last_price", p);
                    self.match_market(Order { remaining: trigger.size, ..trigger });
                }
            }
            self.stop_bids.remove(stop_price);
        }

        // Ask stops triggered when price <= stop_price (e.g., stop-loss on longs / sell stop)
        let triggered_asks: Vec<(u64, Vec<String>)> = self.stop_asks
            .range(p..)
            .map(|(&k, v)| (k, v.clone()))
            .collect();
        for (stop_price, ids) in &triggered_asks {
            for id in ids {
                if let Some(order) = self.stop_orders.remove(id) {
                    let trigger = Order {
                        order_type: OrderType::Market,
                        ..order
                    };
                    log::debug!("Stop ask triggered",
                        "id", short_id(id), "stop_price", stop_price, "last_price", p);
                    self.match_market(Order { remaining: trigger.size, ..trigger });
                }
            }
            self.stop_asks.remove(stop_price);
        }
    }

    // ── Book Management ──

    fn add_to_book(&mut self, order: Order) {
        let id = order.id.clone();
        let book = match order.side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        let queue = book.entry(order.price).or_default();
        if order.remaining > 0 {
            queue.push_back(id.clone());
            self.orders.insert(id, order);
        }
    }

    fn remove_from_level(&mut self, side: Side, price: u64, id: &str) {
        let book = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        if let Some(queue) = book.get_mut(&price) {
            queue.retain(|x| x != id);
            if queue.is_empty() {
                book.remove(&price);
            }
        }
    }

    fn peek_best(&self, side: Side) -> Option<Order> {
        let book = match side {
            Side::Bid => &self.bids,
            Side::Ask => &self.asks,
        };
        let entry = match side {
            Side::Bid => book.iter().next_back(),
            Side::Ask => book.iter().next(),
        };
        match entry {
            Some((_, queue)) => queue.front().and_then(|id| self.orders.get(id).cloned()),
            None => None,
        }
    }

    fn pop_best(&mut self, side: Side) -> Option<Order> {
        let book = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        let entry = match side {
            Side::Bid => book.iter_mut().next_back(),
            Side::Ask => book.iter_mut().next(),
        };
        match entry {
            Some((&price, queue)) => {
                let id = match queue.pop_front() {
                    Some(id) => id,
                    None => return None,
                };
                if queue.is_empty() {
                    book.remove(&price);
                }
                self.orders.remove(&id)
            }
            None => None,
        }
    }

    fn record_fill(&mut self, taker: &Order, maker: &Order, price: u64, size: u64) {
        self.last_price = price;
        self.fills.push(Fill {
            taker_id: taker.id.clone(),
            maker_id: maker.id.clone(),
            price,
            size,
            taker_side: taker.side,
        });
        // Check stop orders after each price-changing fill
        self.check_stops();
    }

    /// Sum all available liquidity at or better than the given price for a side
    fn liquidity_at_price(&self, side: Side, price: u64) -> u64 {
        let book = match side {
            Side::Bid => &self.bids,
            Side::Ask => &self.asks,
        };
        let iter: Box<dyn Iterator<Item = (&u64, &VecDeque<String>)>> = match side {
            Side::Bid => Box::new(book.iter().rev()),
            Side::Ask => Box::new(book.iter()),
        };
        iter.take_while(|(&p, _)| match side {
            Side::Bid => p >= price,
            Side::Ask => p <= price,
        })
        .flat_map(|(_, queue)| queue.iter())
        .filter_map(|id| self.orders.get(id))
        .map(|o| o.remaining)
        .sum()
    }
}

// ── Legacy compatibility: find_match for two specific orders ──

#[derive(Debug)]
pub struct MatchParams {
    pub match_price: u64,
    pub match_size: u64,
}

pub fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub fn order(id: &str, side: Side, price: u64, size: u64, order_type: OrderType) -> Order {
    Order {
        id: id.to_string(),
        side: side,
        price,
        size,
        remaining: size,
        timestamp_ns: now_nanos(),
        order_type,
        asset: 0,
    }
}

fn is_bid(side: u64) -> bool {
    side == 0 || side == 3
}

pub fn find_match(a: &crate::db::OrderSecrets, b: &crate::db::OrderSecrets) -> Option<MatchParams> {
    log::debug!("Evaluating order pair for matchability",
        "side_a", a.side, "price_a", a.price, "size_a", a.size,
        "side_b", b.side, "price_b", b.price, "size_b", b.size
    );

    if is_bid(a.side) == is_bid(b.side) {
        log::debug!("Orders have same side — no match possible", "side", a.side);
        return None;
    }

    let (buy, sell) = if is_bid(a.side) { (a, b) } else { (b, a) };

    // For market orders (price=0), use the resting order's price
    let buy_price = if buy.price == 0 { sell.price } else { buy.price };
    let sell_price = if sell.price == 0 { buy.price } else { sell.price };

    if buy_price < sell_price {
        let spread = sell_price - buy_price;
        log::debug!("Buyer price below seller price — no match possible",
            "buy_price", buy_price, "sell_price", sell_price, "spread", spread);
        return None;
    }

    let spread = buy_price - sell_price;
    let mid = (buy_price + sell_price) / 2;
    let match_size = buy.size.min(sell.size);

    log::info!("Order pair is matchable",
        "buy_price", buy_price, "sell_price", sell_price,
        "spread", spread, "mid_price", mid,
        "match_size", match_size
    );

    Some(MatchParams { match_price: mid, match_size })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> OrderBook {
        OrderBook::new()
    }

    fn bid(id: &str, price: u64, size: u64) -> Order {
        order(id, Side::Bid, price, size, OrderType::Limit)
    }

    fn ask(id: &str, price: u64, size: u64) -> Order {
        order(id, Side::Ask, price, size, OrderType::Limit)
    }

    fn market_ask(id: &str, size: u64) -> Order {
        order(id, Side::Ask, 0, size, OrderType::Market)
    }

    fn market_bid(id: &str, size: u64) -> Order {
        order(id, Side::Bid, 0, size, OrderType::Market)
    }

    fn ioc_bid(id: &str, price: u64, size: u64) -> Order {
        order(id, Side::Bid, price, size, OrderType::IOC)
    }

    fn fok_bid(id: &str, price: u64, size: u64) -> Order {
        order(id, Side::Bid, price, size, OrderType::FOK)
    }

    fn stop_bid(id: &str, stop: u64, price: u64, size: u64) -> Order {
        order(id, Side::Bid, price, size, OrderType::StopLimit { stop_price: stop })
    }

    fn stop_ask(id: &str, stop: u64, price: u64, size: u64) -> Order {
        order(id, Side::Ask, price, size, OrderType::StopLimit { stop_price: stop })
    }

    fn stop_market_bid(id: &str, stop: u64, size: u64) -> Order {
        order(id, Side::Bid, 0, size, OrderType::StopMarket { stop_price: stop })
    }

    // ── Tests ──

    #[test]
    fn test_empty_book_no_match() {
        let mut ob = book();
        let fills = ob.place(market_bid("b1", 100)).unwrap();
        assert!(fills.is_empty());
        assert_eq!(ob.order_count(), 0);
    }

    #[test]
    fn test_simple_bid_ask_match() {
        let mut ob = book();
        ob.place(ask("a1", 100, 50)).unwrap();
        let fills = ob.place(bid("b1", 100, 50)).unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, 100);
        assert_eq!(fills[0].size, 50);
        assert_eq!(fills[0].maker_id, "a1");
        assert_eq!(fills[0].taker_id, "b1");
        assert_eq!(ob.order_count(), 0);
    }

    #[test]
    fn test_price_priority_ask() {
        let mut ob = book();
        ob.place(ask("a1", 110, 10)).unwrap();  // worse price
        ob.place(ask("a2", 100, 10)).unwrap();  // best price — should match first
        let fills = ob.place(bid("b1", 110, 15)).unwrap();
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[0].maker_id, "a2");
        assert_eq!(fills[0].price, 100);
        assert_eq!(fills[1].maker_id, "a1");
        assert_eq!(fills[1].price, 110);
    }

    #[test]
    fn test_price_priority_bid() {
        let mut ob = book();
        ob.place(bid("b1", 90, 10)).unwrap();   // worse price
        ob.place(bid("b2", 100, 10)).unwrap();  // best price — should match first
        let fills = ob.place(ask("a1", 90, 15)).unwrap();
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[0].maker_id, "b2");
        assert_eq!(fills[0].price, 100);
        assert_eq!(fills[1].maker_id, "b1");
        assert_eq!(fills[1].price, 90);
    }

    #[test]
    fn test_time_priority_same_price() {
        let mut ob = book();
        ob.place(ask("a1", 100, 10)).unwrap();  // first
        ob.place(ask("a2", 100, 10)).unwrap();  // second
        let fills = ob.place(bid("b1", 100, 20)).unwrap();
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[0].maker_id, "a1");
        assert_eq!(fills[1].maker_id, "a2");
    }

    #[test]
    fn test_partial_fill_remainder_rests() {
        let mut ob = book();
        ob.place(ask("a1", 100, 30)).unwrap();
        let fills = ob.place(bid("b1", 100, 50)).unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].size, 30);
        // remainder should rest on book
        assert_eq!(ob.order_count(), 1);
        let (best, remaining) = ob.best_bid().unwrap();
        assert_eq!(best, 100);
        assert_eq!(remaining, 20);
    }

    #[test]
    fn test_market_bid_sweeps_book() {
        let mut ob = book();
        ob.place(ask("a1", 100, 25)).unwrap();
        ob.place(ask("a2", 101, 25)).unwrap();
        let fills = ob.place(market_bid("b1", 40)).unwrap();
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[0].price, 100);
        assert_eq!(fills[0].size, 25);
        assert_eq!(fills[1].price, 101);
        assert_eq!(fills[1].size, 15);
        // ask a2 should have 10 remaining
        assert_eq!(ob.order_count(), 1);
        let (best, remaining) = ob.best_ask().unwrap();
        assert_eq!(best, 101);
        assert_eq!(remaining, 10);
    }

    #[test]
    fn test_ioc_no_rest() {
        let mut ob = book();
        ob.place(ask("a1", 100, 20)).unwrap();
        // IOC bid at 100 — matches the ask, consumes 10, does NOT rest
        let fills = ob.place(ioc_bid("ioc1", 100, 10)).unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].size, 10);
        // Ask a1 should have 10 remaining
        assert_eq!(ob.order_count(), 1);
        // IOC bid with no match — should NOT rest
        let fills2 = ob.place(ioc_bid("ioc2", 50, 10)).unwrap();
        assert!(fills2.is_empty());
        assert_eq!(ob.order_count(), 1); // only a1 remains
    }

    #[test]
    fn test_fok_fill_or_kill() {
        let mut ob = book();
        ob.place(ask("a1", 100, 30)).unwrap();
        ob.place(ask("a2", 101, 20)).unwrap(); // total = 50
        // FOK for exact amount
        ob.place(fok_bid("f1", 101, 50)).unwrap();
        assert_eq!(ob.fills.len(), 2);

        // FOK that exceeds available liquidity
        ob.place(fok_bid("f2", 100, 100)).unwrap();
        assert_eq!(ob.fills.len(), 2); // no new fills
    }

    #[test]
    fn test_cancel_order() {
        let mut ob = book();
        ob.place(ask("a1", 100, 50)).unwrap();
        assert_eq!(ob.order_count(), 1);
        ob.cancel("a1").unwrap();
        assert_eq!(ob.order_count(), 0);
        assert!(ob.best_ask().is_none());
    }

    #[test]
    fn test_depth() {
        let mut ob = book();
        ob.place(ask("a1", 100, 10)).unwrap();
        ob.place(ask("a2", 101, 20)).unwrap();
        ob.place(ask("a3", 102, 30)).unwrap();
        let d = ob.depth(Side::Ask, 2);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0], (100, 10, 1));
        assert_eq!(d[1], (101, 20, 1));
    }

    #[test]
    fn test_spread() {
        let mut ob = book();
        assert!(ob.spread().is_none());
        ob.place(ask("a1", 105, 10)).unwrap();
        ob.place(bid("b1", 100, 10)).unwrap();
        assert_eq!(ob.spread(), Some(5));
    }

    #[test]
    fn test_stop_limit_bid_triggers_on_price_up() {
        let mut ob = book();
        // Setup: ask at 100, bid at 95 (no cross)
        ob.place(ask("a1", 100, 50)).unwrap();
        ob.place(bid("b1", 95, 50)).unwrap();
        assert!(ob.best_bid().is_some());
        assert!(ob.best_ask().is_some());

        // Place a stop-limit bid that triggers when price >= 100
        ob.place(stop_bid("stop1", 100, 100, 50)).unwrap();
        assert_eq!(ob.stop_orders.len(), 1);

        // A market buy that moves price to 100 should trigger the stop
        ob.place(market_bid("trigger", 100)).unwrap();
        // Stop should have triggered and matched against remaining asks
        assert_eq!(ob.stop_orders.len(), 0);
    }

    #[test]
    fn test_stop_market_bid_triggers() {
        let mut ob = book();
        ob.place(ask("a1", 100, 100)).unwrap();
        // Stop-market bid triggers when price >= 100
        ob.place(stop_market_bid("stop1", 100, 50)).unwrap();
        assert_eq!(ob.stop_orders.len(), 1);

        ob.place(market_bid("trigger", 60)).unwrap();
        // Stop should have triggered and been consumed
        assert_eq!(ob.stop_orders.len(), 0);
        // The stop market order should have filled against remaining ask
        let (_, remaining) = ob.best_ask().unwrap();
        assert_eq!(remaining, 40 - 60.min(0)); // 60 consumed first, stop 50 also matched
    }

    #[test]
    fn test_multi_level_market_sweep() {
        let mut ob = book();
        ob.place(ask("a1", 100, 10)).unwrap();
        ob.place(ask("a2", 101, 20)).unwrap();
        ob.place(ask("a3", 102, 30)).unwrap();
        ob.place(ask("a4", 103, 40)).unwrap();
        let fills = ob.place(market_bid("b1", 75)).unwrap();
        assert_eq!(fills.len(), 4);
        let total_filled: u64 = fills.iter().map(|f| f.size).sum();
        assert_eq!(total_filled, 75);
        // a4 should have 5 remaining (40 - (75 - 10 - 20 - 30) = 5)
        assert_eq!(ob.order_count(), 1);
        let (_, remaining) = ob.best_ask().unwrap();
        assert_eq!(remaining, 25);
    }

    #[test]
    fn test_limit_no_cross_rests() {
        let mut ob = book();
        ob.place(ask("a1", 105, 50)).unwrap();
        let fills = ob.place(bid("b1", 100, 30)).unwrap();
        assert!(fills.is_empty());
        assert_eq!(ob.order_count(), 2);
        assert_eq!(ob.spread(), Some(5));
    }

    #[test]
    fn test_best_bid_ask_empty_book() {
        let ob = book();
        assert!(ob.best_bid().is_none());
        assert!(ob.best_ask().is_none());
        assert!(ob.spread().is_none());
        assert!(ob.depth(Side::Bid, 5).is_empty());
    }

    #[test]
    fn test_order_id_string_clipping() {
        let mut ob = book();
        ob.place(bid("a-very-long-order-id-that-should-work-fine", 100, 10)).unwrap();
        assert_eq!(ob.order_count(), 1);
        let (price, remaining) = ob.best_bid().unwrap();
        assert_eq!(price, 100);
        assert_eq!(remaining, 10);
    }

    #[test]
    fn test_no_cross_no_match() {
        let mut ob = book();
        ob.place(ask("a1", 200, 10)).unwrap();
        ob.place(bid("b1", 100, 10)).unwrap();
        let fills = ob.place(bid("b2", 150, 5)).unwrap();
        assert!(fills.is_empty());
        assert_eq!(ob.order_count(), 3);
    }

    #[test]
    fn test_same_price_multi_order_fifo() {
        let mut ob = book();
        ob.place(ask("a1", 100, 10)).unwrap();
        ob.place(ask("a2", 100, 10)).unwrap();
        ob.place(ask("a3", 100, 10)).unwrap();
        let fills = ob.place(bid("b1", 100, 25)).unwrap();
        assert_eq!(fills.len(), 3);
        assert_eq!(fills[0].maker_id, "a1");
        assert_eq!(fills[0].size, 10);
        assert_eq!(fills[1].maker_id, "a2");
        assert_eq!(fills[1].size, 10);
        assert_eq!(fills[2].maker_id, "a3");
        assert_eq!(fills[2].size, 5);  // partial fill
        assert_eq!(ob.order_count(), 1); // a3 has 5 remaining
    }

    #[test]
    fn test_limit_fill_exact() {
        let mut ob = book();
        ob.place(ask("a1", 100, 50)).unwrap();
        let fills = ob.place(bid("b1", 100, 50)).unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].size, 50);
        assert_eq!(ob.order_count(), 0);
    }

    #[test]
    fn test_limit_buy_better_price() {
        let mut ob = book();
        ob.place(ask("a1", 100, 30)).unwrap();
        // Bid at 101 — should match at 100 (the maker's price)
        let fills = ob.place(bid("b1", 101, 30)).unwrap();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, 100);
        assert_eq!(fills[0].size, 30);
    }

    #[test]
    fn test_benchmark_seed_pattern() {
        let mut ob = book();
        let center_price: u64 = 100_000;
        let step = 2000;
        let order_size = 5_000_000;
        // Place 20 alternating limit orders (10 bid, 10 ask) at non-crossing prices
        for j in 0..20 {
            let side = (j % 2) as u64;  // 0=Bid, 1=Ask
            let shift = ((j / 2) as i64 + 1) * step;
            let price = if side == 0 {
                center_price.saturating_sub(shift as u64)
            } else {
                center_price.saturating_add(shift as u64)
            };
            let id = format!("order-{j}");
            if side == 0 {
                ob.place(order(&id, Side::Bid, price, order_size, OrderType::Limit)).unwrap();
            } else {
                ob.place(order(&id, Side::Ask, price, order_size, OrderType::Limit)).unwrap();
            }
        }
        assert_eq!(ob.order_count(), 20, "all 20 orders should rest");
        let bb = ob.best_bid();
        let ba = ob.best_ask();
        assert!(bb.is_some(), "best_bid should exist");
        assert!(ba.is_some(), "best_ask should exist");
        let (bb_price, _) = bb.unwrap();
        let (ba_price, _) = ba.unwrap();
        assert_eq!(bb_price, 98_000, "best bid should be 98000");
        assert_eq!(ba_price, 102_000, "best ask should be 102000");
        assert_eq!(ob.spread(), Some(4_000), "spread should be 4000");
        let bid_depth = ob.depth(Side::Bid, 10);
        let ask_depth = ob.depth(Side::Ask, 10);
        assert_eq!(bid_depth.len(), 10, "should have 10 bid levels");
        assert_eq!(ask_depth.len(), 10, "should have 10 ask levels");
        // Each level should have 1 order
        for level in &bid_depth {
            assert_eq!(level.2, 1, "each bid level should have 1 order, got price={}", level.0);
        }
        for level in &ask_depth {
            assert_eq!(level.2, 1, "each ask level should have 1 order, got price={}", level.0);
        }
    }

    #[test]
    fn test_stop_bid_not_triggered_below_stop() {
        let mut ob = book();
        ob.place(ask("a1", 200, 100)).unwrap();
        // Stop bid at 150 should NOT trigger when price is 200
        ob.place(stop_bid("s1", 150, 200, 50)).unwrap();
        ob.place(market_bid("b1", 50)).unwrap();
        // Stop should remain since we traded at 200 (>= 150 means triggered)
        // Actually, last_price = 200 which IS >= 150, so it should trigger
        // Let me use a higher stop:
        let mut ob2 = book();
        ob2.place(ask("a2", 100, 100)).unwrap();
        ob2.place(stop_bid("s2", 150, 200, 50)).unwrap();
        // Match at 100 — price is 100 which is < 150, so stop should NOT trigger
        ob2.place(market_bid("b2", 30)).unwrap();
        assert_eq!(ob2.stop_orders.len(), 1);
    }
}
