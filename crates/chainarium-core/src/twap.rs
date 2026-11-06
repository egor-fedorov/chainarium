//! DEC-17: bounded canonical price history, exact rational integral and one final floor.
use crate::{Amount, amount::Price, model::*};
use num_bigint::BigUint;
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

pub const MODEL: &str = "native-twap-v1";
const MAX_BITS: u64 = 131072;
fn bounded(n: BigUint) -> Result<BigUint, String> {
    if n.bits() > MAX_BITS {
        Err("TWAP rational exceeds 131072 bits".into())
    } else {
        Ok(n)
    }
}
fn gcd(mut a: BigUint, mut b: BigUint) -> BigUint {
    while !b.is_zero() {
        let r = &a % &b;
        a = b;
        b = r;
    }
    a
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RationalWire", into = "RationalWire")]
pub struct Rational {
    numerator: BigUint,
    denominator: BigUint,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RationalWire {
    numerator: String,
    denominator: String,
}
impl Rational {
    pub fn new(n: BigUint, d: BigUint) -> Result<Self, String> {
        let n = bounded(n)?;
        let d = bounded(d)?;
        if d.is_zero() {
            return Err("zero rational denominator".into());
        }
        let g = gcd(n.clone(), d.clone());
        Ok(Self {
            numerator: n / &g,
            denominator: d / g,
        })
    }
    pub fn zero() -> Self {
        Self {
            numerator: BigUint::zero(),
            denominator: BigUint::one(),
        }
    }
    pub fn from_price(p: &Price) -> Self {
        Self::new(p.numerator().big(), p.denominator().big()).expect("positive U256 price")
    }
    pub fn add(&self, r: &Self) -> Result<Self, String> {
        Self::new(
            bounded(
                bounded(&self.numerator * &r.denominator)?
                    + bounded(&r.numerator * &self.denominator)?,
            )?,
            bounded(&self.denominator * &r.denominator)?,
        )
    }
    pub fn sub(&self, r: &Self) -> Result<Self, String> {
        let l = bounded(&self.numerator * &r.denominator)?;
        let n = bounded(&r.numerator * &self.denominator)?;
        if l < n {
            return Err("negative integral".into());
        }
        Self::new(l - n, bounded(&self.denominator * &r.denominator)?)
    }
    pub fn mul(&self, n: Amount) -> Result<Self, String> {
        Self::new(
            bounded(&self.numerator * n.big())?,
            self.denominator.clone(),
        )
    }
    pub fn div(&self, d: Amount) -> Result<Self, String> {
        Self::new(
            self.numerator.clone(),
            bounded(&self.denominator * d.big())?,
        )
    }
    pub fn price(&self, base: &str, quote: &str) -> Result<Price, String> {
        Price::new(
            base.into(),
            quote.into(),
            Amount::from_big(self.numerator.clone()).map_err(|_| "PriceOutOfRange")?,
            Amount::from_big(self.denominator.clone()).map_err(|_| "PriceOutOfRange")?,
        )
        .map_err(|_| "PriceOutOfRange".into())
    }
    pub fn published_price(&self, base: &str, quote: &str) -> Result<Price, String> {
        let scale = BigUint::from(1_000_000_000_000_000_000u64);
        let n = bounded(&self.numerator * &scale)? / &self.denominator;
        Self::new(n, scale)?.price(base, quote)
    }
}
impl From<Rational> for RationalWire {
    fn from(r: Rational) -> Self {
        Self {
            numerator: r.numerator.to_string(),
            denominator: r.denominator.to_string(),
        }
    }
}
impl TryFrom<RationalWire> for Rational {
    type Error = String;
    fn try_from(w: RationalWire) -> Result<Self, String> {
        let parse = |s: &str| -> Result<BigUint, String> {
            if s.is_empty()
                || s.len() > 39457
                || (s.len() > 1 && s.starts_with('0'))
                || !s.bytes().all(|b| b.is_ascii_digit())
            {
                return Err("expected bounded canonical rational string".into());
            }
            bounded(BigUint::parse_bytes(s.as_bytes(), 10).ok_or("invalid rational")?)
        };
        let n = parse(&w.numerator)?;
        let d = parse(&w.denominator)?;
        let r = Self::new(n.clone(), d.clone())?;
        if r.numerator != n || r.denominator != d {
            return Err("rational must be reduced".into());
        }
        Ok(r)
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum FeedKind {
    #[default]
    External,
    NativeSpot {
        source: String,
    },
    NativeTwap {
        source: String,
        window: Amount,
    },
}
impl FeedKind {
    pub fn is_external(&self) -> bool {
        matches!(self, Self::External)
    }
    pub fn source(&self) -> Option<&str> {
        match self {
            Self::External => None,
            Self::NativeSpot { source } | Self::NativeTwap { source, .. } => Some(source),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    pub pool: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub history_capacity: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    pub start: Amount,
    pub price: Option<Rational>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct History {
    pub anchor: Amount,
    pub last_time: Amount,
    pub integral: Rational,
    pub segments: VecDeque<Segment>,
}
impl History {
    pub fn new(price: Option<Rational>) -> Self {
        Self {
            anchor: Amount::ZERO,
            last_time: Amount::ZERO,
            integral: Rational::zero(),
            segments: VecDeque::from([Segment {
                start: Amount::ZERO,
                price,
            }]),
        }
    }
    fn advance(&mut self, time: Amount) -> Result<(), String> {
        let dt = time.checked_sub(self.last_time)?;
        if let Some(p) = &self.segments.back().ok_or("empty price history")?.price {
            self.integral = self.integral.add(&p.mul(dt)?)?;
        }
        self.last_time = time;
        Ok(())
    }
    fn update(&mut self, price: Option<Rational>, capacity: usize) -> Result<bool, String> {
        let last = self.segments.back_mut().ok_or("empty price history")?;
        if last.price == price {
            return Ok(false);
        }
        if last.start == self.last_time {
            last.price = price;
            let n = self.segments.len();
            if n > 1 && self.segments[n - 2].price == self.segments[n - 1].price {
                self.segments.pop_back();
            }
        } else {
            self.segments.push_back(Segment {
                start: self.last_time,
                price,
            });
        }
        if self.segments.len() > capacity {
            let old = self.segments.pop_front().ok_or("empty price history")?;
            let next = self.segments.front().ok_or("empty price history")?.start;
            if let Some(p) = old.price {
                self.integral = self.integral.sub(&p.mul(next.checked_sub(old.start)?)?)?;
            }
            self.anchor = next;
        }
        Ok(true)
    }
    pub fn average<E: From<String>>(
        &self,
        end: Amount,
        window: Amount,
        charge: &mut impl FnMut() -> Result<(), E>,
        used: &mut Vec<WindowSegment>,
    ) -> Result<Rational, E> {
        if window.is_zero() {
            return Err("zero TWAP window".to_string().into());
        }
        if end > self.last_time {
            return Err("future TWAP end".to_string().into());
        }
        let start = end
            .checked_sub(window)
            .map_err(|_| "NotReady: insufficient history".to_string())?;
        if start < self.anchor {
            return Err("NotReady: window precedes anchor".to_string().into());
        }
        let mut sum = Rational::zero();
        for (i, s) in self.segments.iter().enumerate() {
            let stop = self.segments.get(i + 1).map_or(self.last_time, |s| s.start);
            let a = std::cmp::max(start, s.start);
            let b = std::cmp::min(end, stop);
            if a >= b {
                continue;
            }
            charge()?;
            used.push(WindowSegment {
                start: a,
                end: b,
                price: s.price.clone(),
            });
            let p = s
                .price
                .as_ref()
                .ok_or_else(|| "NotReady: price gap".to_string())?;
            sum = sum.add(&p.mul(b.checked_sub(a)?)?)?;
        }
        Ok(sum.div(window)?)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WindowSegment {
    pub start: Amount,
    pub end: Amount,
    pub price: Option<Rational>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QueryTrace {
    pub source: String,
    pub end: Amount,
    pub window: Amount,
    pub segments: Vec<WindowSegment>,
    pub exact_average: Option<Rational>,
    pub outcome: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceUpdate {
    pub source: String,
    pub timestamp: Amount,
    pub price: Option<Rational>,
}
pub fn source_price(s: &Scenario, w: &World, c: &SourceConfig) -> Result<Option<Rational>, String> {
    let p = s.protocols.get(&c.pool).ok_or("source must be a CPMM")?;
    let base = w.balance(&p.account, &c.base_asset);
    let quote = w.balance(&p.account, &c.quote_asset);
    if base.is_zero() || quote.is_zero() {
        Ok(None)
    } else {
        Ok(Some(Rational::new(quote.big(), base.big())?))
    }
}
pub fn validate(s: &Scenario) -> Result<(), String> {
    if s.price_sources.len() > 16 {
        return Err("more than 16 price sources".into());
    }
    if !s.price_sources.is_empty() && !s.model_versions.iter().any(|v| v == MODEL) {
        return Err("missing native TWAP model version".into());
    }
    for (id, c) in &s.price_sources {
        let p = s.protocols.get(&c.pool).ok_or("source must be a CPMM")?;
        if id.is_empty()
            || c.history_capacity != 128
            || !((c.base_asset == p.x && c.quote_asset == p.y)
                || (c.base_asset == p.y && c.quote_asset == p.x))
        {
            return Err("invalid CPMM price source".into());
        }
    }
    for c in s.feeds.values() {
        if let Some(source) = c.kind.source() {
            let p = s
                .price_sources
                .get(source)
                .ok_or("unknown native oracle source")?;
            if c.initial.is_some()
                || c.source_id != source
                || c.base_asset != p.base_asset
                || c.quote_asset != p.quote_asset
            {
                return Err("invalid native feed source/initial".into());
            }
        }
        if let FeedKind::NativeTwap { window, .. } = c.kind
            && window.is_zero()
        {
            return Err("zero TWAP window".into());
        }
    }
    Ok(())
}
pub fn genesis(s: &Scenario, w: &World) -> Result<BTreeMap<String, History>, String> {
    s.price_sources
        .iter()
        .map(|(id, c)| Ok((id.clone(), History::new(source_price(s, w, c)?))))
        .collect()
}
pub fn advance(s: &Scenario, w: &mut World) -> Result<Vec<Event>, String> {
    let mut events = vec![];
    for id in s.price_sources.keys() {
        let h = Arc::make_mut(&mut w.price_history)
            .get_mut(id)
            .ok_or("missing price source state")?;
        let elapsed = w.timestamp.checked_sub(h.last_time)?;
        h.advance(w.timestamp)?;
        if !elapsed.is_zero() {
            events.push(Event {
                kind: "PriceTimeAccrued".into(),
                from: id.clone(),
                to: "integral".into(),
                asset: "seconds".into(),
                amount: elapsed,
            });
        }
    }
    Ok(events)
}
pub fn sync_pool(s: &Scenario, w: &mut World, pool: &str) -> Result<Vec<SourceUpdate>, String> {
    let mut changed = vec![];
    for (id, c) in s.price_sources.iter().filter(|(_, c)| c.pool == pool) {
        let price = source_price(s, w, c)?;
        let h = Arc::make_mut(&mut w.price_history)
            .get_mut(id)
            .ok_or("missing price source state")?;
        if h.last_time != w.timestamp {
            return Err("price source clock mismatch".into());
        }
        if h.update(price.clone(), c.history_capacity as usize)? {
            changed.push(SourceUpdate {
                source: id.clone(),
                timestamp: w.timestamp,
                price,
            });
        }
    }
    Ok(changed)
}
pub fn invariant(s: &Scenario, w: &World) -> Result<(), String> {
    if s.price_sources.keys().ne(w.price_history.keys()) {
        return Err("price source registry invariant".into());
    }
    for (id, c) in &s.price_sources {
        let h = &w.price_history[id];
        if h.segments.is_empty()
            || h.segments.len() > c.history_capacity as usize
            || h.last_time != w.timestamp
            || h.segments[0].start != h.anchor
        {
            return Err("price history clock/capacity invariant".into());
        }
        let mut expected = Rational::zero();
        for (i, seg) in h.segments.iter().enumerate() {
            let end = h.segments.get(i + 1).map_or(h.last_time, |s| s.start);
            if seg.start > end || (i + 1 < h.segments.len() && seg.start == end) {
                return Err("price history order invariant".into());
            }
            if let Some(p) = &seg.price {
                p.price(&c.base_asset, &c.quote_asset)?;
                expected = expected.add(&p.mul(end.checked_sub(seg.start)?)?)?;
            }
        }
        if expected != h.integral || h.segments.back().unwrap().price != source_price(s, w, c)? {
            return Err("price integral/current price invariant".into());
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub struct SourceView {
    pub source: String,
    pub pool: String,
    pub spot: Option<Price>,
    pub anchor: Amount,
    pub last_time: Amount,
    pub segments: usize,
    pub capacity: u16,
}
#[derive(Serialize)]
pub struct FeedView {
    pub feed: String,
    pub kind: FeedKind,
    pub record: Option<crate::credit::OracleRecord>,
    pub observation_age: Option<Amount>,
    pub source: Option<SourceView>,
    pub query: Option<QueryTrace>,
    pub projected_price: Option<Price>,
}
pub fn views(s: &Scenario, w: &World) -> Result<Vec<FeedView>, String> {
    let mut views = vec![];
    for (id, c) in &s.feeds {
        let record = w.feeds[id].record.clone();
        let age = record
            .as_ref()
            .map(|r| w.timestamp.checked_sub(r.observed_at))
            .transpose()?;
        let mut source_view = None;
        let mut query = None;
        let mut projected_price = None;
        if let Some(source) = c.kind.source() {
            let config = &s.price_sources[source];
            let h = &w.price_history[source];
            let spot = h
                .segments
                .back()
                .and_then(|s| s.price.as_ref())
                .map(|p| p.price(&c.base_asset, &c.quote_asset))
                .transpose()?;
            source_view = Some(SourceView {
                source: source.into(),
                pool: config.pool.clone(),
                spot: spot.clone(),
                anchor: h.anchor,
                last_time: h.last_time,
                segments: h.segments.len(),
                capacity: config.history_capacity,
            });
            if let FeedKind::NativeTwap { window, .. } = c.kind {
                let mut segments = vec![];
                let mut exact = None;
                let result = (|| -> Result<Price, String> {
                    let avg = h.average(
                        w.timestamp,
                        window,
                        &mut || Ok::<(), String>(()),
                        &mut segments,
                    )?;
                    exact = Some(avg.clone());
                    avg.published_price(&c.base_asset, &c.quote_asset)
                })();
                let outcome = match result {
                    Ok(price) => {
                        projected_price = Some(price);
                        "Ready".into()
                    }
                    Err(e) => e,
                };
                query = Some(QueryTrace {
                    source: source.into(),
                    end: w.timestamp,
                    window,
                    segments,
                    exact_average: exact,
                    outcome,
                });
            } else {
                projected_price = spot;
            }
        }
        views.push(FeedView {
            feed: id.clone(),
            kind: c.kind.clone(),
            record,
            observation_age: age,
            source: source_view,
            query,
            projected_price,
        });
    }
    Ok(views)
}
