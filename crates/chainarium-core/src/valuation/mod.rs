//! DEC-28: derived, bounded reference marks of committed native snapshots.
mod config;
mod extract;
mod rational;
use crate::{Amount, Block, Run, World};
pub use config::*;
pub use rational::Exact;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Mark {
    pub status: String,
    pub value: Option<Exact>,
    pub priced_part: Exact,
    pub reasons: BTreeSet<String>,
    pub truncated: bool,
}
impl Mark {
    fn priced(value: Exact) -> Self {
        Self {
            status: "Priced".into(),
            value: Some(value.clone()),
            priced_part: value,
            reasons: BTreeSet::new(),
            truncated: false,
        }
    }
    fn zero() -> Self {
        Self::priced(Exact::default())
    }
    fn fail(&mut self, reason: &str) {
        self.status = "Unpriced".into();
        self.value = None;
        self.reasons.insert(reason.into());
        self.truncated |= matches!(reason, "ArithmeticLimit" | "ComponentLimit");
    }
    fn add(&mut self, rhs: &Self, bits: u64) {
        match self.priced_part.add(&rhs.priced_part, bits) {
            Ok(n) => {
                self.priced_part = n.clone();
                if self.value.is_some() {
                    self.value = Some(n);
                }
            }
            Err(e) => self.fail(&e),
        }
        for e in &rhs.reasons {
            self.fail(e);
        }
        self.truncated |= rhs.truncated;
    }
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PriceUse {
    pub source_id: Option<String>,
    pub record: Option<Record>,
    pub age_seconds: Option<Amount>,
    pub mark: Mark,
}
pub fn price_at(p: &Policy, asset: &str, at: Amount) -> PriceUse {
    let mut out = PriceUse {
        source_id: None,
        record: None,
        age_seconds: None,
        mark: Mark::zero(),
    };
    if asset == p.numeraire {
        out.source_id = Some("numeraire-self".into());
        out.mark = Mark::priced(Exact::integer(1.into()));
        return out;
    }
    let Some(feed) = p.reference_feeds.get(asset) else {
        out.mark.fail("MissingPrice");
        return out;
    };
    out.source_id = Some(feed.source_id.clone());
    let end = feed.records.partition_point(|r| r.available_at <= at);
    let Some(record) = end.checked_sub(1).and_then(|i| feed.records.get(i)) else {
        out.mark.fail("NoAvailableRecord");
        return out;
    };
    out.record = Some(record.clone());
    let Ok(age) = at.checked_sub(record.observed_at) else {
        out.mark.fail("FutureObservation");
        return out;
    };
    out.age_seconds = Some(age);
    if age > p.max_age_seconds {
        out.mark.fail("Stale");
        return out;
    }
    out.mark = match Exact::ratio(
        record.price.numerator().big().into(),
        record.price.denominator().big(),
        p.limits.rational_bits,
    ) {
        Ok(v) => Mark::priced(v),
        Err(e) => {
            let mut m = Mark::zero();
            m.fail(&e);
            m
        }
    };
    out
}
fn marked(p: &Policy, asset: &str, at: Amount, quantity: &Exact) -> (Mark, Option<PriceUse>) {
    if quantity.is_zero() {
        return (Mark::zero(), None);
    }
    let price = price_at(p, asset, at);
    let mut mark = Mark::zero();
    if let Some(value) = &price.mark.value {
        match quantity.mul(value, p.limits.rational_bits) {
            Ok(v) => mark = Mark::priced(v),
            Err(e) => mark.fail(&e),
        }
    } else {
        for e in &price.mark.reasons {
            mark.fail(e);
        }
    }
    (mark, Some(price))
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub account: String,
    pub kind: String,
    pub protocol: String,
    pub position: String,
    pub asset: String,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct DepositCash {
    pub claim: Amount,
    pub reserve_cash: Amount,
    pub cash_bound: Amount,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Component {
    pub key: Key,
    pub quantity: Option<Exact>,
    pub mark: Mark,
    pub price: Option<PriceUse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deposit_cash: Option<DepositCash>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Snapshot {
    pub mark: Mark,
    pub components: Vec<Component>,
    pub by_asset: BTreeMap<String, Mark>,
    pub component_checks: usize,
}
pub fn snapshot(s: &crate::Scenario, w: &World, portfolio: &str) -> Result<Snapshot, String> {
    let p = s
        .valuation_policy
        .as_ref()
        .ok_or("valuation is not configured")?;
    let owners = p.portfolios.get(portfolio).ok_or("unknown portfolio")?;
    Ok(extract::snapshot(s, w, p, owners))
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Flow {
    pub portfolio: String,
    pub block: Amount,
    pub timestamp: Amount,
    pub tx_id: String,
    pub from: String,
    pub to: String,
    pub asset: String,
    pub amount: Amount,
    pub direction: String,
    pub mark: Mark,
    pub price: Option<PriceUse>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct TransferNotice {
    pub portfolio: String,
    pub tx_id: String,
    pub event: crate::Event,
    pub capital_label: bool,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PortfolioView {
    pub portfolio: String,
    pub accounts: Vec<String>,
    pub overlaps: Vec<String>,
    pub initial: Mark,
    pub current: Snapshot,
    pub inflows: Mark,
    pub outflows: Mark,
    pub pnl: Mark,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Boundary {
    pub run_id: String,
    pub model: String,
    pub numeraire: String,
    pub block: Amount,
    pub timestamp: Amount,
    pub portfolios: Vec<PortfolioView>,
    /// New capital flows in this block, never cumulative duplicates.
    pub capital_flows: Vec<Flow>,
    pub boundary_transfers: Vec<TransferNotice>,
}

fn flows(
    p: &Policy,
    block: &Block,
    seen: &mut BTreeSet<CapitalTransfer>,
) -> (Vec<Flow>, Vec<TransferNotice>) {
    let mut flows = vec![];
    let mut notices = vec![];
    let labels: BTreeSet<_> = p.capital_transfers.iter().collect();
    let mut receipts: Vec<_> = block
        .receipts
        .iter()
        .filter(|r| r.status == "Success")
        .collect();
    receipts.sort_by(|a, b| a.tx_id.cmp(&b.tx_id));
    for r in receipts {
        for e in r
            .events
            .iter()
            .filter(|e| e.kind == "Transfer" && !e.amount.is_zero())
        {
            for (id, portfolio) in &p.portfolios {
                let from_inside = portfolio.accounts.contains(&e.from);
                if from_inside == portfolio.accounts.contains(&e.to) {
                    continue;
                }
                let binding = CapitalTransfer {
                    portfolio: id.clone(),
                    tx_id: r.tx_id.clone(),
                };
                let labelled = labels.contains(&binding);
                notices.push(TransferNotice {
                    portfolio: id.clone(),
                    tx_id: r.tx_id.clone(),
                    event: e.clone(),
                    capital_label: labelled,
                });
                // Validation restricts labelled programs to one literal positive Transfer.
                // Only successful canonical receipts reach this point (never bundle probes).
                if !labelled || !seen.insert(binding) {
                    continue;
                }
                let (mark, price) = marked(p, &e.asset, block.timestamp, &Exact::integer(e.amount));
                flows.push(Flow {
                    portfolio: id.clone(),
                    block: block.number,
                    timestamp: block.timestamp,
                    tx_id: r.tx_id.clone(),
                    from: e.from.clone(),
                    to: e.to.clone(),
                    asset: e.asset.clone(),
                    amount: e.amount,
                    direction: if from_inside { "Outflow" } else { "Inflow" }.into(),
                    mark,
                    price,
                });
            }
        }
    }
    (flows, notices)
}
fn pnl(current: &Mark, initial: &Mark, incoming: &Mark, outgoing: &Mark, bits: u64) -> Mark {
    let mut result = Mark::zero();
    for (m, reason) in [
        (current, "CurrentUnpriced"),
        (initial, "InitialUnpriced"),
        (incoming, "HistoricalFlowUnpriced"),
        (outgoing, "HistoricalFlowUnpriced"),
    ] {
        if m.value.is_none() {
            result.fail(reason);
            result.truncated |= m.truncated;
        }
    }
    // priced_part is explicitly partial if any input is unavailable.
    let n = current
        .priced_part
        .sub(&initial.priced_part, bits)
        .and_then(|n| n.sub(&incoming.priced_part, bits))
        .and_then(|n| n.add(&outgoing.priced_part, bits));
    match n {
        Ok(n) => {
            result.priced_part = n.clone();
            if result.value.is_some() {
                result.value = Some(n);
            }
        }
        Err(e) => result.fail(&e),
    }
    result
}

/// No persistent cache: rebuild derived history from immutable committed inputs.
/// `only_last` avoids extracting intermediate worlds for a selected HTTP boundary.
pub fn visit(
    run: &Run,
    through: usize,
    only_last: bool,
    mut emit: impl FnMut(Boundary) -> Result<(), String>,
) -> Result<(), String> {
    let p = run
        .scenario
        .valuation_policy
        .as_ref()
        .ok_or("valuation is not configured")?;
    if through > run.blocks.len() {
        return Err("unknown committed valuation boundary".into());
    }
    let initial: BTreeMap<_, _> = p
        .portfolios
        .keys()
        .map(|id| {
            (
                id.clone(),
                extract::snapshot(&run.scenario, &run.initial, p, &p.portfolios[id]),
            )
        })
        .collect();
    let mut totals: BTreeMap<_, _> = p
        .portfolios
        .keys()
        .map(|id| (id.clone(), (Mark::zero(), Mark::zero())))
        .collect();
    let mut seen = BTreeSet::new();
    for index in 0..=through {
        let (world, capital_flows, boundary_transfers) = if index == 0 {
            (&run.initial, vec![], vec![])
        } else {
            let block = &run.blocks[index - 1];
            let (f, n) = flows(p, block, &mut seen);
            (&block.state, f, n)
        };
        for flow in &capital_flows {
            let (incoming, outgoing) = totals
                .get_mut(&flow.portfolio)
                .ok_or("unknown flow portfolio")?;
            if flow.direction == "Inflow" {
                incoming.add(&flow.mark, p.limits.rational_bits);
            } else {
                outgoing.add(&flow.mark, p.limits.rational_bits);
            }
        }
        if only_last && index != through {
            continue;
        }
        let mut portfolios = vec![];
        for (id, portfolio) in &p.portfolios {
            let current = if index == 0 {
                initial[id].clone()
            } else {
                extract::snapshot(&run.scenario, world, p, portfolio)
            };
            let (incoming, outgoing) = &totals[id];
            portfolios.push(PortfolioView {
                portfolio: id.clone(),
                accounts: portfolio.accounts.clone(),
                overlaps: p
                    .portfolios
                    .iter()
                    .filter(|(other, q)| {
                        *other != id && q.accounts.iter().any(|a| portfolio.accounts.contains(a))
                    })
                    .map(|(other, _)| other.clone())
                    .collect(),
                pnl: pnl(
                    &current.mark,
                    &initial[id].mark,
                    incoming,
                    outgoing,
                    p.limits.rational_bits,
                ),
                initial: initial[id].mark.clone(),
                current,
                inflows: incoming.clone(),
                outflows: outgoing.clone(),
            });
        }
        emit(Boundary {
            run_id: run.run_id.clone(),
            model: MODEL.into(),
            numeraire: p.numeraire.clone(),
            block: world.block,
            timestamp: world.timestamp,
            portfolios,
            capital_flows,
            boundary_transfers,
        })?;
    }
    Ok(())
}
pub fn at(run: &Run, boundary: usize) -> Result<Boundary, String> {
    let mut result = None;
    visit(run, boundary, true, |view| {
        result = Some(view);
        Ok(())
    })?;
    result.ok_or_else(|| "missing valuation boundary".into())
}
