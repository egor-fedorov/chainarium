use super::*;
use crate::valuation::{Exact, Mark};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Index {
    pub schema: String,
    pub a: Description,
    pub b: Option<Description>,
    pub times: Vec<Amount>,
    pub configuration: Option<ConfigDiff>,
}
pub fn index(a: &History, b: Option<&History>) -> Result<Index, String> {
    let end = b.map_or(a.run.state.timestamp, |b| {
        a.run.state.timestamp.min(b.run.state.timestamp)
    });
    let mut times = BTreeSet::from([Amount::ZERO]);
    for h in std::iter::once(a).chain(b) {
        times.extend(
            h.run
                .blocks
                .iter()
                .map(|b| b.timestamp)
                .filter(|t| *t <= end),
        );
    }
    Ok(Index {
        schema: MODEL.into(),
        a: a.description(),
        b: b.map(History::description),
        times: times.into_iter().collect(),
        configuration: b
            .map(|b| configuration_diff(&a.run.scenario, &b.run.scenario))
            .transpose()?,
    })
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Point {
    pub history: Description,
    pub block: usize,
    pub timestamp: Amount,
    pub age: Amount,
    pub receipts: usize,
    pub rejections: usize,
    pub used: Amount,
    pub native_fees: Exact,
    pub fee_asset: String,
}
fn point(h: &History, time: Amount) -> Result<Point, String> {
    let block = h.as_of(time)?;
    let world = h.world(block)?;
    let b = block.checked_sub(1).map(|i| &h.run.blocks[i]);
    let mut fees = Exact::default();
    if let Some(b) = b {
        for r in &b.receipts {
            fees = fees.add(&Exact::integer(r.fee), 32768)?;
        }
    }
    Ok(Point {
        history: h.description(),
        block,
        timestamp: world.timestamp,
        age: time.checked_sub(world.timestamp)?,
        receipts: b.map_or(0, |b| b.receipts.len()),
        rejections: b.map_or(0, |b| b.rejected.len()),
        used: b.map_or(Amount::ZERO, |b| b.used),
        native_fees: fees,
        fee_asset: h.run.scenario.chain.fee_asset.clone(),
    })
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct MetricPair {
    pub portfolio: String,
    pub metric: String,
    pub a: Option<Mark>,
    pub b: Option<Mark>,
    pub difference: Mark,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Comparison {
    pub schema: String,
    pub time: Amount,
    pub a: Point,
    pub b: Option<Point>,
    pub reasons: BTreeSet<String>,
    pub metrics: Vec<MetricPair>,
    pub configuration: Option<ConfigDiff>,
}
fn marks(h: &History, block: usize) -> Result<BTreeMap<(String, String), Mark>, String> {
    let mut marks = BTreeMap::new();
    if h.run.scenario.valuation_policy.is_some() {
        for p in crate::valuation::at(&h.run, block)?.portfolios {
            marks.insert((p.portfolio.clone(), "value".into()), p.current.mark);
            marks.insert((p.portfolio, "pnl".into()), p.pnl);
        }
    }
    if h.run.scenario.hodl_policy.is_some() {
        for p in crate::hodl::at(&h.run, block)?.portfolios {
            for (name, m) in [
                ("hodl_value", p.hodl.mark),
                ("hodl_pnl", p.hodl_pnl),
                ("excess_pnl", p.excess_pnl),
                ("value_gap", p.value_gap),
            ] {
                marks.insert((p.portfolio.clone(), name.into()), m);
            }
        }
    }
    Ok(marks)
}
fn difference(a: Option<&Mark>, b: Option<&Mark>, reasons: &BTreeSet<String>, bits: u64) -> Mark {
    let mut m = Mark::zero();
    for r in reasons {
        m.fail(r);
    }
    for (side, value) in [("Left", a), ("Right", b)] {
        match value {
            None => m.fail(&format!("Missing{side}Metric")),
            Some(v) if v.value.is_none() => {
                m.fail(&format!("Unpriced{side}"));
                for r in &v.reasons {
                    m.fail(r);
                }
                m.truncated |= v.truncated;
            }
            _ => {}
        }
    }
    if m.value.is_some()
        && let (Some(a), Some(b)) = (
            a.and_then(|v| v.value.as_ref()),
            b.and_then(|v| v.value.as_ref()),
        )
    {
        match b.sub(a, bits) {
            Ok(v) => m = Mark::priced(v),
            Err(e) => m.fail(&e),
        }
    }
    m
}
pub fn compare(a: &History, b: Option<&History>, time: Amount) -> Result<Comparison, String> {
    let idx = index(a, b)?;
    if idx.times.binary_search(&time).is_err() {
        return Err("unknown common committed time".into());
    }
    let pa = point(a, time)?;
    let pb = b.map(|b| point(b, time)).transpose()?;
    let mut reasons = BTreeSet::new();
    if let (Some(b), Some(pb)) = (b, &pb) {
        if pa.timestamp != pb.timestamp {
            reasons.insert("BoundaryTimeMismatch".into());
        }
        if a.run.scenario.chain.id != b.run.scenario.chain.id {
            reasons.insert("ChainMismatch".into());
        }
        if a.run.scenario.assets != b.run.scenario.assets {
            reasons.insert("AssetUnitsMismatch".into());
        }
        if a.run.scenario.valuation_policy != b.run.scenario.valuation_policy {
            reasons.insert("ValuationPolicyMismatch".into());
        }
        if a.run.scenario.valuation_policy.is_none() {
            reasons.insert("NoValuationPolicy".into());
        }
    } else {
        reasons.insert("NoRightHistory".into());
    }
    let am = marks(a, pa.block)?;
    let bm = match (b, &pb) {
        (Some(b), Some(pb)) => marks(b, pb.block)?,
        _ => BTreeMap::new(),
    };
    let mut metrics = vec![];
    for key in am.keys().chain(bm.keys()).collect::<BTreeSet<_>>() {
        let mut rs = reasons.clone();
        let hodl = key.1 != "value" && key.1 != "pnl";
        if hodl && b.is_some_and(|b| a.run.scenario.hodl_policy != b.run.scenario.hodl_policy) {
            rs.insert("HodlPolicyMismatch".into());
        }
        let bits = if hodl {
            a.run
                .scenario
                .hodl_policy
                .as_ref()
                .map(|p| p.limits.rational_bits)
        } else {
            a.run
                .scenario
                .valuation_policy
                .as_ref()
                .map(|p| p.limits.rational_bits)
        }
        .unwrap_or(32768);
        metrics.push(MetricPair {
            portfolio: key.0.clone(),
            metric: key.1.clone(),
            a: am.get(key).cloned(),
            b: bm.get(key).cloned(),
            difference: difference(am.get(key), bm.get(key), &rs, bits),
        });
    }
    Ok(Comparison {
        schema: MODEL.into(),
        time,
        a: pa,
        b: pb,
        reasons,
        metrics,
        configuration: idx.configuration,
    })
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct BalancePair {
    pub asset: String,
    pub a: Option<Amount>,
    pub b: Option<Amount>,
    pub difference: Mark,
}
pub fn balances(
    a: &History,
    b: Option<&History>,
    time: Amount,
    account: &str,
    offset: usize,
) -> Result<Page<BalancePair>, String> {
    if index(a, b)?.times.binary_search(&time).is_err() {
        return Err("unknown common committed time".into());
    }
    let aa = a.world(a.as_of(time)?)?.accounts.get(account);
    let ba = match b {
        Some(b) => b.world(b.as_of(time)?)?.accounts.get(account),
        None => None,
    };
    if aa.is_none() && ba.is_none() {
        return Err("unknown account on both sides".into());
    }
    let assets = aa
        .into_iter()
        .flat_map(|a| a.balances.keys())
        .chain(ba.into_iter().flat_map(|a| a.balances.keys()))
        .collect::<BTreeSet<_>>();
    let mut rows = vec![];
    for asset in assets {
        let mut reasons = BTreeSet::new();
        if aa.is_none() || ba.is_none() {
            reasons.insert("MissingAccount".into());
        }
        if let Some(b) = b {
            if a.run.scenario.chain.id != b.run.scenario.chain.id {
                reasons.insert("ChainMismatch".into());
            }
            if a.run.scenario.assets.get(asset) != b.run.scenario.assets.get(asset)
                || !a.run.scenario.assets.contains_key(asset)
            {
                reasons.insert("AssetUnitsMismatch".into());
            }
        } else {
            reasons.insert("NoRightHistory".into());
        }
        let av = aa.map(|a| a.balances.get(asset).copied().unwrap_or_default());
        let bv = ba.map(|a| a.balances.get(asset).copied().unwrap_or_default());
        rows.push(BalancePair {
            asset: asset.clone(),
            a: av,
            b: bv,
            difference: difference(
                av.map(|v| Mark::priced(Exact::integer(v))).as_ref(),
                bv.map(|v| Mark::priced(Exact::integer(v))).as_ref(),
                &reasons,
                512,
            ),
        });
    }
    page(rows, offset)
}
