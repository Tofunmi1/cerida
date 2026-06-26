use crate::db::OrderSecrets;

#[derive(Debug)]
pub struct MatchParams {
    pub match_price: u64,
    pub match_size: u64,
}

pub fn find_match(a: &OrderSecrets, b: &OrderSecrets) -> Option<MatchParams> {
    if a.side == b.side {
        return None;
    }
    let (buy, sell) = if a.side == 0 { (a, b) } else { (b, a) };
    if buy.price < sell.price {
        return None;
    }
    let match_price = (buy.price + sell.price) / 2;
    let match_size = buy.size.min(sell.size);
    Some(MatchParams { match_price, match_size })
}
