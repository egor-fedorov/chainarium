use crate::{Amount, amount::Price, model::*};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub const CREDIT_MODELS: [&str; 2] = ["isolated-collateral-lending-v1", "oracle-observation-v1"];
pub fn scale() -> Amount {
    1_000_000_000_000_000_000u64.into()
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Rate {
    Constant {
        rate: Amount,
    },
    Kink {
        base: Amount,
        slope_low: Amount,
        slope_high: Amount,
        kink: Amount,
    },
}
impl Rate {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Constant { rate } => {
                if *rate > scale() {
                    Err("rate exceeds scale".into())
                } else {
                    Ok(())
                }
            }
            Self::Kink {
                base,
                slope_low,
                slope_high,
                kink,
            } => {
                if kink.is_zero()
                    || *kink >= scale()
                    || base.checked_add(*slope_low)?.checked_add(*slope_high)? > scale()
                {
                    Err("invalid kink rate".into())
                } else {
                    Ok(())
                }
            }
        }
    }
    pub fn rate(&self, cash: Amount, debt: Amount) -> Result<Amount, String> {
        self.validate()?;
        match self {
            Self::Constant { rate } => Ok(*rate),
            Self::Kink {
                base,
                slope_low,
                slope_high,
                kink,
            } => {
                let assets = cash.checked_add(debt)?;
                let u = if assets.is_zero() {
                    Amount::ZERO
                } else {
                    debt.mul_div(scale(), assets, false)?
                };
                if u <= *kink {
                    base.checked_add(slope_low.mul_div(u, *kink, false)?)
                } else {
                    base.checked_add(*slope_low)?
                        .checked_add(slope_high.mul_div(
                            u.checked_sub(*kink)?,
                            scale().checked_sub(*kink)?,
                            false,
                        )?)
                }
            }
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LendingConfig {
    pub cash_account: String,
    pub debt_asset: String,
    pub collateral_asset: String,
    pub feed: String,
    pub ltv_bps: u16,
    pub lt_bps: u16,
    pub close_bps: u16,
    pub bonus_bps: u16,
    pub reserve_factor: Amount,
    pub supply_cap: Amount,
    pub borrow_cap: Amount,
    pub max_age: Amount,
    pub rate: Rate,
    pub positions: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OracleRecord {
    pub price: Price,
    pub source_id: String,
    pub observed_at: Amount,
    pub published_at: Amount,
    pub sequence: Amount,
    pub update_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FeedConfig {
    #[serde(default, skip_serializing_if = "crate::twap::FeedKind::is_external")]
    pub kind: crate::twap::FeedKind,
    pub base_asset: String,
    pub quote_asset: String,
    pub source_id: String,
    pub publishers: BTreeSet<String>,
    pub initial: Option<OracleRecord>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FeedState {
    pub record: Option<OracleRecord>,
    pub seen_updates: BTreeSet<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub debt: Amount,
    pub remainder: Amount,
    pub collateral_enabled: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Loss {
    pub block: Amount,
    pub borrower: String,
    pub amount: Amount,
    pub equity_loss: Amount,
    pub depositor_loss: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LendingState {
    pub debt: Amount,
    pub claims: Amount,
    pub equity: Amount,
    pub total_shares: Amount,
    pub deposit_shares: BTreeMap<String, Amount>,
    pub positions: BTreeMap<String, Position>,
    pub last_accrual: Amount,
    pub losses: Vec<Loss>,
}
impl LendingState {
    pub fn new(cfg: &LendingConfig) -> Self {
        Self {
            debt: Amount::ZERO,
            claims: Amount::ZERO,
            equity: Amount::ZERO,
            total_shares: Amount::ZERO,
            deposit_shares: BTreeMap::new(),
            positions: cfg
                .positions
                .keys()
                .map(|k| {
                    (
                        k.clone(),
                        Position {
                            debt: Amount::ZERO,
                            remainder: Amount::ZERO,
                            collateral_enabled: true,
                        },
                    )
                })
                .collect(),
            last_accrual: Amount::ZERO,
            losses: vec![],
        }
    }
    pub fn bankrupt(&self) -> bool {
        self.claims.is_zero() && !self.total_shares.is_zero()
    }
}
pub fn validate<'a>(s: &'a Scenario, protected: &mut BTreeSet<&'a String>) -> Result<(), String> {
    if (!s.lending.is_empty() && !s.model_versions.iter().any(|v| v == CREDIT_MODELS[0]))
        || (!s.feeds.is_empty() && !s.model_versions.iter().any(|v| v == CREDIT_MODELS[1]))
    {
        return Err("missing credit/oracle model version".into());
    }
    for (id, cfg) in &s.feeds {
        if id.is_empty()
            || cfg.source_id.is_empty()
            || cfg.base_asset == cfg.quote_asset
            || !s.assets.contains_key(&cfg.base_asset)
            || !s.assets.contains_key(&cfg.quote_asset)
            || cfg.publishers.is_empty()
        {
            return Err("invalid oracle feed".into());
        }
        for p in &cfg.publishers {
            if !s.accounts.contains_key(p) || s.protected(p) {
                return Err("invalid oracle publisher".into());
            }
        }
        if let Some(r) = &cfg.initial
            && (r.price.base() != cfg.base_asset
                || r.price.quote() != cfg.quote_asset
                || r.source_id != cfg.source_id
                || !r.observed_at.is_zero()
                || !r.published_at.is_zero()
                || r.update_id.is_empty())
        {
            return Err("invalid genesis oracle".into());
        }
    }
    for (id, cfg) in &s.lending {
        if id.is_empty()
            || s.protocols.contains_key(id)
            || s.stable_pools.contains_key(id)
            || s.flash_providers.contains_key(id)
            || !s.assets.contains_key(&cfg.debt_asset)
            || !s.assets.contains_key(&cfg.collateral_asset)
            || cfg.debt_asset == cfg.collateral_asset
            || cfg.positions.len() > 1024
            || cfg.ltv_bps > cfg.lt_bps
            || cfg.lt_bps > 10000
            || cfg.close_bps == 0
            || cfg.close_bps > 10000
            || cfg.reserve_factor > scale()
            || cfg.supply_cap.is_zero()
            || cfg.borrow_cap.is_zero()
        {
            return Err("invalid lending configuration".into());
        }
        let feed = s.feeds.get(&cfg.feed).ok_or("missing lending feed")?;
        if feed.base_asset != cfg.collateral_asset || feed.quote_asset != cfg.debt_asset {
            return Err("lending feed unit mismatch".into());
        }
        cfg.rate.validate()?;
        for account in std::iter::once(&cfg.cash_account).chain(cfg.positions.values()) {
            if !protected.insert(account)
                || !s.accounts.contains_key(account)
                || s.accounts[account].balances.values().any(|b| !b.is_zero())
            {
                return Err("lending/escrow accounts must be distinct and empty at genesis".into());
            }
        }
        for owner in cfg.positions.keys() {
            if !s.accounts.contains_key(owner) || s.protected(owner) {
                return Err("invalid borrower owner".into());
            }
        }
    }
    Ok(())
}
pub fn invariant(s: &Scenario, w: &World) -> Result<(), String> {
    if w.lending.keys().ne(s.lending.keys()) || w.feeds.keys().ne(s.feeds.keys()) {
        return Err("credit registry invariant".into());
    }
    for (id, cfg) in &s.lending {
        let l = &w.lending[id];
        let c = w.balance(&cfg.cash_account, &cfg.debt_asset);
        if c.checked_add(l.debt)? != l.claims.checked_add(l.equity)? {
            return Err(format!("C+D=S+E: {id}"));
        }
        let shares = l
            .deposit_shares
            .iter()
            .try_fold(Amount::ZERO, |sum, (owner, n)| {
                if !s.accounts.contains_key(owner) || s.protected(owner) {
                    return Err("deposit owner invariant".into());
                }
                sum.checked_add(*n)
            })?;
        if shares != l.total_shares
            || (l.total_shares.is_zero() && !l.claims.is_zero())
            || l.last_accrual != w.timestamp
        {
            return Err("deposit shares/accrual invariant".into());
        }
        if l.positions.keys().ne(cfg.positions.keys()) {
            return Err("position registry invariant".into());
        }
        let mut debt = Amount::ZERO;
        for (owner, p) in &l.positions {
            debt = debt.checked_add(p.debt)?;
            if p.remainder >= scale()
                || (p.debt.is_zero() && !p.remainder.is_zero())
                || (!p.debt.is_zero()
                    && w.balance(&cfg.positions[owner], &cfg.collateral_asset)
                        .is_zero())
            {
                return Err("position debt/collateral invariant".into());
            }
        }
        if debt != l.debt {
            return Err("receivables invariant".into());
        }
    }
    for (id, f) in w.feeds.iter() {
        if let Some(r) = &f.record {
            let cfg = &s.feeds[id];
            if r.price.base() != cfg.base_asset
                || r.price.quote() != cfg.quote_asset
                || r.source_id != cfg.source_id
                || r.observed_at > r.published_at
                || r.published_at > w.timestamp
                || !f.seen_updates.contains(&r.update_id)
            {
                return Err("oracle record invariant".into());
            }
        }
    }
    Ok(())
}
pub fn fresh_price<'a>(cfg: &LendingConfig, w: &'a World) -> Result<&'a Price, String> {
    let r = w
        .feeds
        .get(&cfg.feed)
        .and_then(|f| f.record.as_ref())
        .ok_or("oracle unavailable")?;
    if w.timestamp.checked_sub(r.observed_at)? > cfg.max_age {
        return Err("oracle stale".into());
    }
    Ok(&r.price)
}
/// A diagnostic snapshot of the exact observation consulted by a consumer.
/// This is recorded even for rejected attempts, and never refreshes the feed.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OracleUse {
    pub kind: crate::twap::FeedKind,
    pub feed: String,
    pub record: Option<OracleRecord>,
    pub evaluated_at: Amount,
    pub max_age: Amount,
    pub age: Option<Amount>,
    pub status: String,
}
impl OracleUse {
    pub fn capture(s: &Scenario, cfg: &LendingConfig, w: &World) -> Self {
        let record = w.feeds.get(&cfg.feed).and_then(|f| f.record.clone());
        let age = record
            .as_ref()
            .and_then(|r| w.timestamp.checked_sub(r.observed_at).ok());
        let status = match age {
            None => "Unavailable",
            Some(a) if a > cfg.max_age => "Stale",
            Some(_) => "Fresh",
        };
        Self {
            kind: s.feeds[&cfg.feed].kind.clone(),
            feed: cfg.feed.clone(),
            record,
            evaluated_at: w.timestamp,
            max_age: cfg.max_age,
            age,
            status: status.into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PositionView {
    pub owner: String,
    pub escrow: String,
    pub collateral: Amount,
    pub debt: Amount,
    pub remainder: Amount,
    pub collateral_enabled: bool,
    pub health: String,
    // Valuation intermediates can exceed U256; keep exact decimal strings.
    pub health_numerator: Option<String>,
    pub borrow_capacity: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MarketView {
    pub market: String,
    pub cash: Amount,
    pub status: String,
    pub oracle: OracleUse,
    pub positions: Vec<PositionView>,
}
pub fn views(s: &Scenario, w: &World) -> Vec<MarketView> {
    s.lending
        .iter()
        .map(|(id, cfg)| {
            let l = &w.lending[id];
            let oracle = OracleUse::capture(s, cfg, w);
            let positions = l
                .positions
                .iter()
                .map(|(owner, p)| {
                    let escrow = cfg.positions[owner].clone();
                    let collateral = w.balance(&escrow, &cfg.collateral_asset);
                    let (hf, capacity) = match fresh_price(cfg, w) {
                        Ok(price) => {
                            let c = if p.collateral_enabled {
                                collateral
                            } else {
                                Amount::ZERO
                            };
                            (
                                Some(risk_value(c, price, cfg.lt_bps)),
                                Some(risk_value(c, price, cfg.ltv_bps)),
                            )
                        }
                        Err(_) => (None, None),
                    };
                    let health = if p.debt.is_zero() {
                        "NoDebt"
                    } else if let Some(h) = &hf {
                        if h < &p.debt.big() {
                            "Liquidatable"
                        } else {
                            "Healthy"
                        }
                    } else {
                        "Unknown"
                    };
                    PositionView {
                        owner: owner.clone(),
                        escrow,
                        collateral,
                        debt: p.debt,
                        remainder: p.remainder,
                        collateral_enabled: p.collateral_enabled,
                        health: health.into(),
                        health_numerator: hf.map(|v| v.to_string()),
                        borrow_capacity: capacity.map(|v| v.to_string()),
                    }
                })
                .collect();
            MarketView {
                market: id.clone(),
                cash: w.balance(&cfg.cash_account, &cfg.debt_asset),
                status: if l.bankrupt() { "Bankrupt" } else { "Active" }.into(),
                oracle,
                positions,
            }
        })
        .collect()
}
pub fn risk_value(collateral: Amount, price: &Price, bps: u16) -> BigUint {
    (collateral.big() * price.numerator().big() / price.denominator().big()) * BigUint::from(bps)
        / BigUint::from(10000u16)
}
pub fn accrue(s: &Scenario, w: &mut World) -> Result<Vec<Event>, String> {
    let mut events = vec![];
    for (id, cfg) in &s.lending {
        let mut l = w.lending[id].clone();
        let dt = w.timestamp.checked_sub(l.last_accrual)?;
        if dt.is_zero() {
            continue;
        }
        let r = cfg
            .rate
            .rate(w.balance(&cfg.cash_account, &cfg.debt_asset), l.debt)?;
        let mut interest = Amount::ZERO;
        for (owner, p) in &mut l.positions {
            let n = p.debt.big() * r.big() * dt.big() + p.remainder.big();
            let i = Amount::from_big(&n / scale().big())?;
            p.remainder = Amount::from_big(n % scale().big())?;
            p.debt = p.debt.checked_add(i)?;
            interest = interest.checked_add(i)?;
            if !i.is_zero() {
                events.push(Event {
                    kind: "InterestAccrued".into(),
                    from: id.clone(),
                    to: owner.clone(),
                    asset: cfg.debt_asset.clone(),
                    amount: i,
                });
            }
        }
        let equity = if l.total_shares.is_zero() {
            interest
        } else {
            interest.mul_div(cfg.reserve_factor, scale(), false)?
        };
        l.debt = l.debt.checked_add(interest)?;
        l.equity = l.equity.checked_add(equity)?;
        l.claims = l.claims.checked_add(interest.checked_sub(equity)?)?;
        l.last_accrual = w.timestamp;
        Arc::make_mut(&mut w.lending).insert(id.clone(), l);
    }
    Ok(events)
}
