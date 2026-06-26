use crate::log;
use anyhow::Result;
use std::collections::{BTreeMap, HashMap, VecDeque};

// ── Types ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderType {
    Limit,
    Market,
    IOC,
    FOK,
    StopLimit { stop_price: u64 },
    StopMarket { stop_price: u64 },
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: String,
    pub side: Side,
    pub price: u64,
    pub size: u64,
    pub remaining: u64,
    pub timestamp_ns: u128,
    pub order_type: OrderType,
}

#[derive(Debug, Clone)]
pub struct Fill {
    pub taker_id: String,
    pub maker_id: String,
    pub price: u64,
    pub size: u64,
    pub taker_side: Side,
}

// ── Order Book ──

#[derive(Debug)]
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

    /// Total fills generated
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

        match order.order_type {
            OrderType::Market => {
                log::debug!("Placing market order", "side", order.side as u64, "size", order.size);
                self.match_market(order);
            }
            OrderType::Limit => {
                log::debug!("Placing limit order", "side", order.side as u64, "price", order.price, "size", order.size);
                self.match_limit(order);
            }
            OrderType::IOC => {
                log::debug!("Placing IOC order", "side", order.side as u64, "price", order.price, "size", order.size);
                self.match_limit(Order { remaining: order.size, ..order });
                // IOC never rests — anything left is cancelled
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
        }

        Ok(new_fills)
    }

    /// Cancel an active order by ID
    pub fn cancel(&mut self, id: &str) -> Result<bool> {
        if let Some(order) = self.orders.remove(id) {
            self.remove_from_level(order.side, order.price, id);
            log::debug!("Order cancelled", "id", &id[..16], "side", order.side as u64, "price", order.price);
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
            log::debug!("Stop order cancelled", "id", &id[..16]);
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
            log::debug!("Market order partially filled", "id", &order.id[..16],
                "filled", order.size - order.remaining, "unfilled", order.remaining);
        }
    }

    // ── Limit Matching ──

    fn match_limit(&mut self, mut order: Order) {
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
        if order.remaining > 0 {
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
                        "id", &id[..16], "stop_price", stop_price, "last_price", p);
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
                        "id", &id[..16], "stop_price", stop_price, "last_price", p);
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

pub fn find_match(a: &crate::db::OrderSecrets, b: &crate::db::OrderSecrets) -> Option<MatchParams> {
    log::debug!("Evaluating order pair for matchability",
        "side_a", a.side, "price_a", a.price, "size_a", a.size,
        "side_b", b.side, "price_b", b.price, "size_b", b.size
    );

    if a.side == b.side {
        log::debug!("Orders have same side — no match possible", "side", a.side);
        return None;
    }

    let (buy, sell) = if a.side == 0 { (a, b) } else { (b, a) };

    if buy.price < sell.price {
        let spread = sell.price - buy.price;
        log::debug!("Buyer price below seller price — no match possible",
            "buy_price", buy.price, "sell_price", sell.price, "spread", spread);
        return None;
    }

    let spread = buy.price - sell.price;
    let mid = (buy.price + sell.price) / 2;
    let match_size = buy.size.min(sell.size);

    log::info!("Order pair is matchable",
        "buy_price", buy.price, "sell_price", sell.price,
        "spread", spread, "mid_price", mid,
        "match_size", match_size
    );

    Some(MatchParams { match_price: mid, match_size })
}
