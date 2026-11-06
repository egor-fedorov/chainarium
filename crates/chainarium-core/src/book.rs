use crate::{Amount, Call, Receipt, Scenario, Transaction, World};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MODEL: &str = "native-book-v1";
pub const MAX_ORDERS: usize = 1024;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum TimeInForce {
    GTC,
    IOC,
    FOK,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub account: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub lot_atoms: Amount,
    pub quote_tick_atoms: Amount,
    pub taker_fee_bps: u16,
    pub fee_account: String,
}
impl Config {
    pub fn quote_per_lot(&self, ticks: Amount) -> Result<Amount, String> {
        ticks.checked_mul(self.quote_tick_atoms)
    }
    pub fn fee(&self, gross: Amount) -> Result<Amount, String> {
        gross.mul_div((self.taker_fee_bps as u64).into(), 10000.into(), true)
    }
    pub fn locked(&self, side: Side, ticks: Amount, lots: Amount) -> Result<Amount, String> {
        lots.checked_mul(match side {
            Side::Buy => self.quote_per_lot(ticks)?,
            Side::Sell => self.lot_atoms,
        })
    }
    pub fn reservation(&self, side: Side, ticks: Amount, lots: Amount) -> Result<Amount, String> {
        match side {
            Side::Buy => {
                let quote = self.quote_per_lot(ticks)?;
                lots.checked_mul(quote.checked_add(self.fee(quote)?)?)
            }
            Side::Sell => self.locked(side, ticks, lots),
        }
    }
    pub fn reserve_asset(&self, side: Side) -> &str {
        match side {
            Side::Buy => &self.quote_asset,
            Side::Sell => &self.base_asset,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Order {
    pub owner: String,
    pub side: Side,
    pub price_ticks: Amount,
    pub remaining_lots: Amount,
}
pub type Levels = BTreeMap<Amount, BTreeSet<Amount>>;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub next_id: Amount,
    pub orders: BTreeMap<Amount, Order>,
    pub bids: Levels,
    pub asks: Levels,
}
impl Default for State {
    fn default() -> Self {
        Self {
            next_id: 1.into(),
            orders: BTreeMap::new(),
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }
}
impl State {
    fn levels_mut(&mut self, side: Side) -> &mut Levels {
        match side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        }
    }
    pub(crate) fn insert(&mut self, id: Amount, order: Order) {
        self.levels_mut(order.side)
            .entry(order.price_ticks)
            .or_default()
            .insert(id);
        self.orders.insert(id, order);
    }
    pub(crate) fn remove(&mut self, id: Amount) -> Result<Order, String> {
        let order = self.orders.remove(&id).ok_or("missing indexed order")?;
        let levels = self.levels_mut(order.side);
        let ids = levels
            .get_mut(&order.price_ticks)
            .ok_or("missing price level")?;
        if !ids.remove(&id) {
            return Err("missing order index".into());
        }
        if ids.is_empty() {
            levels.remove(&order.price_ticks);
        }
        Ok(order)
    }
    pub fn best(&self, incoming: Side) -> Option<Amount> {
        let level = match incoming {
            Side::Buy => self.asks.first_key_value(),
            Side::Sell => self.bids.last_key_value(),
        };
        level.and_then(|(_, ids)| ids.first().copied())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Fill {
    pub maker_id: Amount,
    pub taker_id: Amount,
    pub maker: String,
    pub taker: String,
    pub price_ticks: Amount,
    pub lots: Amount,
    pub base: Amount,
    pub gross_quote: Amount,
    pub fee: Amount,
    pub taker_quote: Amount,
    pub maker_remaining: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OrderTrace {
    pub book: String,
    pub order_id: Amount,
    pub side: Side,
    pub price_ticks: Amount,
    pub requested_lots: Amount,
    pub remaining_lots: Amount,
    pub filled_lots: Amount,
    pub base_filled: Amount,
    pub gross_quote: Amount,
    pub fee: Amount,
    pub taker_quote: Amount,
    pub reserve_asset: String,
    pub reserved: Amount,
    pub refund: Amount,
    pub resting_lots: Amount,
    pub status: String,
    pub fills: Vec<Fill>,
}

pub fn validate<'a>(s: &'a Scenario, protected: &mut BTreeSet<&'a String>) -> Result<(), String> {
    if s.books.len() > 16 || (!s.books.is_empty() && !s.model_versions.iter().any(|v| v == MODEL)) {
        return Err("book registry/model limit".into());
    }
    for (id, c) in &s.books {
        if id.is_empty()
            || s.pool(id).is_some()
            || s.flash_providers.contains_key(id)
            || s.lending.contains_key(id)
            || c.base_asset == c.quote_asset
            || !s.assets.contains_key(&c.base_asset)
            || !s.assets.contains_key(&c.quote_asset)
            || c.lot_atoms.is_zero()
            || c.quote_tick_atoms.is_zero()
            || c.taker_fee_bps >= 10000
            || !s.accounts.contains_key(&c.account)
            || !s.accounts.contains_key(&c.fee_account)
            || s.protected(&c.fee_account)
            || !protected.insert(&c.account)
        {
            return Err("invalid order book config".into());
        }
        if s.accounts[&c.account]
            .balances
            .values()
            .any(|v| !v.is_zero())
        {
            return Err("book genesis escrow must be empty".into());
        }
    }
    Ok(())
}

pub fn invariant(s: &Scenario, w: &World) -> Result<(), String> {
    if s.books.keys().ne(w.books.keys()) {
        return Err("book registry invariant".into());
    }
    for (id, cfg) in &s.books {
        let state = &w.books[id];
        if state.orders.len() > MAX_ORDERS || state.next_id.is_zero() {
            return Err("book capacity/sequence invariant".into());
        }
        let mut indexed = State::default();
        let mut base = Amount::ZERO;
        let mut quote = Amount::ZERO;
        for (id, order) in &state.orders {
            if id.is_zero()
                || *id >= state.next_id
                || order.price_ticks.is_zero()
                || order.remaining_lots.is_zero()
                || !s.accounts.contains_key(&order.owner)
                || s.protected(&order.owner)
            {
                return Err("book order invariant".into());
            }
            cfg.quote_per_lot(order.price_ticks)?;
            order.remaining_lots.checked_mul(cfg.lot_atoms)?;
            let locked = cfg.locked(order.side, order.price_ticks, order.remaining_lots)?;
            match order.side {
                Side::Buy => quote = quote.checked_add(locked)?,
                Side::Sell => base = base.checked_add(locked)?,
            }
            indexed.insert(*id, order.clone());
        }
        if indexed.bids != state.bids || indexed.asks != state.asks {
            return Err("book price-time index invariant".into());
        }
        if let (Some((bid, _)), Some((ask, _))) =
            (state.bids.last_key_value(), state.asks.first_key_value())
            && bid >= ask
        {
            return Err("crossed book invariant".into());
        }
        for asset in s.assets.keys() {
            let expected = if asset == &cfg.base_asset {
                base
            } else if asset == &cfg.quote_asset {
                quote
            } else {
                Amount::ZERO
            };
            if w.balance(&cfg.account, asset) != expected {
                return Err("book escrow invariant".into());
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Quote {
    pub state_ref: String,
    pub block: Amount,
    pub receipt: Receipt,
}
/// Uses the real executor on a private snapshot at this exact timestamp. Sender
/// funding, fee reserve, nonce, matching, and resource limits are all checked.
pub fn quote(s: &Scenario, w: &World, tx: &Transaction) -> Result<Quote, String> {
    if tx.program.len() != 1 || !matches!(tx.program[0], Call::PlaceOrder { .. }) {
        return Err("book quote requires one PlaceOrder".into());
    }
    s.validate_tx(tx)?;
    let receipt = crate::engine::simulate_transaction(s, w, tx)?;
    Ok(Quote {
        state_ref: crate::run::digest(&serde_json::to_vec(&(s, w)).map_err(|e| e.to_string())?),
        block: w.block,
        receipt,
    })
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct OrderView {
    pub id: Amount,
    pub owner: String,
    pub remaining_lots: Amount,
    pub locked: Amount,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct LevelView {
    pub price_ticks: Amount,
    pub quote_per_lot: Amount,
    pub lots: Amount,
    pub orders: Vec<OrderView>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct View {
    pub book: String,
    pub config: Config,
    pub next_id: Amount,
    pub escrow_base: Amount,
    pub escrow_quote: Amount,
    pub bids: Vec<LevelView>,
    pub asks: Vec<LevelView>,
}
pub fn views(s: &Scenario, w: &World) -> Result<Vec<View>, String> {
    s.books
        .iter()
        .map(|(id, cfg)| {
            let state = &w.books[id];
            let levels = |index: &Levels| -> Result<Vec<LevelView>, String> {
                index
                    .iter()
                    .map(|(price, ids)| {
                        let mut lots = Amount::ZERO;
                        let mut orders = vec![];
                        for id in ids {
                            let o = &state.orders[id];
                            lots = lots.checked_add(o.remaining_lots)?;
                            orders.push(OrderView {
                                id: *id,
                                owner: o.owner.clone(),
                                remaining_lots: o.remaining_lots,
                                locked: cfg.locked(o.side, o.price_ticks, o.remaining_lots)?,
                            });
                        }
                        Ok(LevelView {
                            price_ticks: *price,
                            quote_per_lot: cfg.quote_per_lot(*price)?,
                            lots,
                            orders,
                        })
                    })
                    .collect()
            };
            let mut bids = levels(&state.bids)?;
            bids.reverse();
            Ok(View {
                book: id.clone(),
                config: cfg.clone(),
                next_id: state.next_id,
                escrow_base: w.balance(&cfg.account, &cfg.base_asset),
                escrow_quote: w.balance(&cfg.account, &cfg.quote_asset),
                bids,
                asks: levels(&state.asks)?,
            })
        })
        .collect()
}
