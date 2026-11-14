//! DEC-30: immutable reference HODL, with canonical capital-flow feasibility.
mod config;
#[cfg(test)]
mod tests;
use crate::{
    Amount, Block, Run,
    valuation::{self, Component, Exact, Flow, Mark, PriceUse, Snapshot},
};
pub use config::*;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Cursor {
    pub block: Amount,
    pub timestamp: Amount,
    pub receipt_index: usize,
    pub event_index: usize,
    pub tx_id: String,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Point {
    Boundary { block: Amount, timestamp: Amount },
    Flow { cursor: Cursor },
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Holdings {
    pub at: Point,
    pub quantities: BTreeMap<String, Exact>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Failure {
    pub reason: String,
    pub at: Point,
    pub asset: Option<String>,
    pub requested: Option<Exact>,
    pub available: Option<Exact>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status")]
pub enum Ledger {
    Ready {
        current: Holdings,
    },
    Unavailable {
        failure: Failure,
        last_feasible: Option<Box<Holdings>>,
    },
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "status")]
pub enum Effect {
    Applied { before: Exact, after: Exact },
    Failed { reason: String },
    Skipped { reason: String },
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct FlowView {
    pub cursor: Cursor,
    pub flow: Flow,
    pub effect: Effect,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AssetView {
    pub quantity: Exact,
    pub mark: Mark,
    pub price: Option<PriceUse>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Valuation {
    pub mark: Mark,
    pub assets: BTreeMap<String, AssetView>,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct View {
    pub portfolio: String,
    pub accounts: Vec<String>,
    pub overlaps: Vec<String>,
    pub initial_components: Vec<Component>,
    pub initial_holdings: Option<Holdings>,
    pub initial: Mark,
    pub ledger: Ledger,
    pub flow_events: Amount,
    pub flows: Vec<FlowView>,
    pub actual: Mark,
    pub actual_pnl: Mark,
    pub hodl: Valuation,
    pub hodl_pnl: Mark,
    pub inflows: Mark,
    pub outflows: Mark,
    pub excess_pnl: Mark,
    pub value_gap: Mark,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Provenance {
    pub build_id: String,
    pub run_id: String,
    pub model: String,
    pub numeraire: String,
    pub policy_sha256: String,
    pub valuation_policy_sha256: String,
    pub reference_inputs_sha256: String,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Boundary {
    pub provenance: Provenance,
    pub block: Amount,
    pub timestamp: Amount,
    pub portfolios: Vec<View>,
}
fn hash(value: &impl Serialize) -> Result<String, String> {
    Ok(crate::run::digest(
        &serde_json::to_vec(value).map_err(|e| e.to_string())?,
    ))
}
pub fn provenance(run: &Run) -> Result<Provenance, String> {
    let p = run
        .scenario
        .hodl_policy
        .as_ref()
        .ok_or("HODL is not configured")?;
    let v = run
        .scenario
        .valuation_policy
        .as_ref()
        .ok_or("HODL requires reference MTM")?;
    Ok(Provenance {
        build_id: run.build_id.clone(),
        run_id: run.run_id.clone(),
        model: MODEL.into(),
        numeraire: v.numeraire.clone(),
        policy_sha256: hash(p)?,
        valuation_policy_sha256: hash(v)?,
        reference_inputs_sha256: hash(&v.reference_feeds)?,
    })
}
fn unavailable(reason: &str) -> Mark {
    let mut mark = Mark::zero();
    mark.fail(reason);
    mark.truncated |= matches!(reason, "AssetLimit" | "FlowLimit");
    mark
}
fn difference(a: &Mark, b: &Mark, bits: u64) -> Mark {
    let mut out = Mark::zero();
    if a.value.is_none() {
        out.fail("ActualUnpriced");
    }
    if b.value.is_none() {
        out.fail("HodlUnavailable");
    }
    for reason in a.reasons.iter().chain(&b.reasons) {
        out.fail(reason);
    }
    out.truncated |= a.truncated || b.truncated;
    match a.priced_part.sub(&b.priced_part, bits) {
        Ok(n) => {
            out.priced_part = n.clone();
            if out.value.is_some() {
                out.value = Some(n);
            }
        }
        Err(e) => out.fail(&e),
    }
    out
}
struct Shadow {
    initial: Snapshot,
    initial_holdings: Option<Holdings>,
    ledger: Ledger,
    flow_events: usize,
    inflows: Mark,
    outflows: Mark,
}
impl Shadow {
    fn new(initial: Snapshot, limits: &Limits) -> Self {
        let at = Point::Boundary {
            block: Amount::ZERO,
            timestamp: Amount::ZERO,
        };
        let mut quantities = BTreeMap::new();
        let mut failure = initial
            .mark
            .reasons
            .contains("ComponentLimit")
            .then(|| Failure {
                reason: "ComponentLimit".into(),
                at: at.clone(),
                asset: None,
                requested: None,
                available: None,
            });
        for c in &initial.components {
            if failure.is_some() {
                break;
            }
            let result = (|| -> Result<(), String> {
                let quantity = c.quantity.as_ref().ok_or("InitialQuantityUnavailable")?;
                if quantity < &Exact::default() || (c.key.kind == "Debt" && !quantity.is_zero()) {
                    return Err("InitialLiabilityUnsupported".into());
                }
                if quantity.is_zero() {
                    return Ok(());
                }
                let previous = quantities.get(&c.key.asset).cloned().unwrap_or_default();
                let next = quantity
                    .bounded(limits.rational_bits)?
                    .add(&previous, limits.rational_bits)?;
                if !quantities.contains_key(&c.key.asset)
                    && quantities.len() == limits.assets_per_portfolio
                {
                    return Err("AssetLimit".into());
                }
                quantities.insert(c.key.asset.clone(), next);
                Ok(())
            })();
            if let Err(reason) = result {
                failure = Some(Failure {
                    reason,
                    at: at.clone(),
                    asset: Some(c.key.asset.clone()),
                    requested: None,
                    available: None,
                });
            }
        }
        let (ledger, initial_holdings) = if let Some(failure) = failure {
            (
                Ledger::Unavailable {
                    failure,
                    last_feasible: None,
                },
                None,
            )
        } else {
            let current = Holdings { at, quantities };
            (
                Ledger::Ready {
                    current: current.clone(),
                },
                Some(current),
            )
        };
        Self {
            initial,
            initial_holdings,
            ledger,
            flow_events: 0,
            inflows: Mark::zero(),
            outflows: Mark::zero(),
        }
    }
    fn apply(&mut self, flow: &Flow, cursor: &Cursor, limits: &Limits) -> Effect {
        self.flow_events += 1; // DEC-28 bounds the number of unique capital bindings.
        let Ledger::Ready { current } = &self.ledger else {
            let Ledger::Unavailable { failure, .. } = &self.ledger else {
                unreachable!()
            };
            return Effect::Skipped {
                reason: failure.reason.clone(),
            };
        };
        let quantity = Exact::integer(flow.amount);
        let before = current
            .quantities
            .get(&flow.asset)
            .cloned()
            .unwrap_or_default();
        let next = (|| -> Result<Exact, String> {
            if self.flow_events > limits.flow_events_per_portfolio {
                return Err("FlowLimit".into());
            }
            if flow.direction == "Outflow" && before < quantity {
                return Err("InfeasibleOutflow".into());
            }
            let after = if flow.direction == "Inflow" {
                before.add(&quantity, limits.rational_bits)
            } else {
                before.sub(&quantity, limits.rational_bits)
            }?;
            if !after.is_zero()
                && !current.quantities.contains_key(&flow.asset)
                && current.quantities.len() == limits.assets_per_portfolio
            {
                return Err("AssetLimit".into());
            }
            Ok(after)
        })();
        match next {
            Ok(after) => {
                let Ledger::Ready { current } = &mut self.ledger else {
                    unreachable!()
                };
                if after.is_zero() {
                    current.quantities.remove(&flow.asset);
                } else {
                    current.quantities.insert(flow.asset.clone(), after.clone());
                }
                current.at = Point::Flow {
                    cursor: cursor.clone(),
                };
                Effect::Applied { before, after }
            }
            Err(reason) => {
                self.ledger = Ledger::Unavailable {
                    failure: Failure {
                        reason: reason.clone(),
                        at: Point::Flow {
                            cursor: cursor.clone(),
                        },
                        asset: Some(flow.asset.clone()),
                        requested: Some(quantity),
                        available: Some(before),
                    },
                    last_feasible: Some(Box::new(current.clone())),
                };
                Effect::Failed { reason }
            }
        }
    }
    fn mark(&self, p: &valuation::Policy, at: Amount, bits: u64) -> Valuation {
        let Ledger::Ready { current } = &self.ledger else {
            let Ledger::Unavailable { failure, .. } = &self.ledger else {
                unreachable!()
            };
            return Valuation {
                mark: unavailable(&failure.reason),
                assets: BTreeMap::new(),
            };
        };
        let mut assets = BTreeMap::new();
        let mut total = Mark::zero();
        for (asset, quantity) in &current.quantities {
            let price = valuation::price_at(p, asset, at);
            let mut mark = Mark::zero();
            if let Some(p) = &price.mark.value {
                match quantity.mul(p, bits) {
                    Ok(n) => mark = Mark::priced(n),
                    Err(e) => mark.fail(&e),
                }
            } else {
                mark = price.mark.clone();
            }
            total.add(&mark, bits);
            assets.insert(
                asset.clone(),
                AssetView {
                    quantity: quantity.clone(),
                    mark,
                    price: Some(price),
                },
            );
        }
        Valuation {
            mark: total,
            assets,
        }
    }
}
/// Resolve the validated single literal Transfer against its actual canonical
/// receipt/event cursor. DEC-28's TxId-sorted presentation order is not execution.
fn ordered_flows(block: &Block, flows: Vec<Flow>) -> Result<Vec<(Cursor, Flow)>, String> {
    let mut by_tx: BTreeMap<String, Vec<Flow>> = BTreeMap::new();
    for flow in flows {
        by_tx.entry(flow.tx_id.clone()).or_default().push(flow);
    }
    let mut out = vec![];
    for (receipt_index, r) in block.receipts.iter().enumerate() {
        if r.status != "Success" {
            continue;
        }
        let Some(fs) = by_tx.get(&r.tx_id) else {
            continue;
        };
        let event_index = r
            .events
            .iter()
            .position(|e| {
                e.kind == "Transfer"
                    && fs.iter().all(|f| {
                        e.asset == f.asset
                            && e.amount == f.amount
                            && e.from == f.from
                            && e.to == f.to
                    })
            })
            .ok_or("missing canonical HODL capital event")?;
        let cursor = Cursor {
            block: block.number,
            timestamp: block.timestamp,
            receipt_index,
            event_index,
            tx_id: r.tx_id.clone(),
        };
        for flow in by_tx.remove(&r.tx_id).ok_or("missing HODL flow")? {
            out.push((cursor.clone(), flow));
        }
    }
    if !by_tx.is_empty() {
        return Err("missing canonical HODL receipt".into());
    }
    Ok(out)
}
/// Stream derived boundaries. Selected-boundary queries scan capital history but
/// extract current portfolio claims only at the requested boundary. No Run cache.
pub fn visit(
    run: &Run,
    through: usize,
    only_last: bool,
    mut emit: impl FnMut(Boundary) -> Result<(), String>,
) -> Result<(), String> {
    let p = run
        .scenario
        .hodl_policy
        .as_ref()
        .ok_or("HODL is not configured")?;
    let v = run
        .scenario
        .valuation_policy
        .as_ref()
        .ok_or("HODL requires reference MTM")?;
    if through > run.blocks.len() {
        return Err("unknown committed HODL boundary".into());
    }
    let provenance = provenance(run)?;
    let mut shadows = BTreeMap::new();
    for id in &p.portfolios {
        shadows.insert(
            id.clone(),
            Shadow::new(
                valuation::snapshot(&run.scenario, &run.initial, id)?,
                &p.limits,
            ),
        );
    }
    let mut seen = BTreeSet::new();
    for index in 0..=through {
        let world = if index == 0 {
            &run.initial
        } else {
            &run.blocks[index - 1].state
        };
        let mut changes: BTreeMap<String, Vec<FlowView>> = BTreeMap::new();
        if index > 0 {
            let block = &run.blocks[index - 1];
            let (flows, _) = valuation::flows(v, block, &mut seen);
            // Match DEC-28's existing flow-value accumulation order and limits.
            // Only asset feasibility below uses canonical receipt order.
            for flow in &flows {
                if let Some(shadow) = shadows.get_mut(&flow.portfolio) {
                    if flow.direction == "Inflow" {
                        shadow.inflows.add(&flow.mark, v.limits.rational_bits);
                    } else {
                        shadow.outflows.add(&flow.mark, v.limits.rational_bits);
                    }
                }
            }
            for (cursor, flow) in ordered_flows(block, flows)? {
                if let Some(shadow) = shadows.get_mut(&flow.portfolio) {
                    let effect = shadow.apply(&flow, &cursor, &p.limits);
                    changes
                        .entry(flow.portfolio.clone())
                        .or_default()
                        .push(FlowView {
                            cursor,
                            flow,
                            effect,
                        });
                }
            }
        }
        for shadow in shadows.values_mut() {
            if let Ledger::Ready { current } = &mut shadow.ledger {
                current.at = Point::Boundary {
                    block: world.block,
                    timestamp: world.timestamp,
                };
            }
        }
        if only_last && index != through {
            continue;
        }
        let mut portfolios = vec![];
        for (id, shadow) in &shadows {
            let actual = if index == 0 {
                shadow.initial.mark.clone()
            } else {
                valuation::snapshot(&run.scenario, world, id)?.mark
            };
            let actual_pnl = valuation::pnl(
                &actual,
                &shadow.initial.mark,
                &shadow.inflows,
                &shadow.outflows,
                v.limits.rational_bits,
            );
            let hodl = shadow.mark(v, world.timestamp, p.limits.rational_bits);
            let hodl_pnl = valuation::pnl(
                &hodl.mark,
                &shadow.initial.mark,
                &shadow.inflows,
                &shadow.outflows,
                p.limits.rational_bits,
            );
            let accounts = &v.portfolios[id].accounts;
            portfolios.push(View {
                portfolio: id.clone(),
                accounts: accounts.clone(),
                overlaps: v
                    .portfolios
                    .iter()
                    .filter(|(other, c)| {
                        *other != id && c.accounts.iter().any(|a| accounts.contains(a))
                    })
                    .map(|(id, _)| id.clone())
                    .collect(),
                initial_components: shadow.initial.components.clone(),
                initial_holdings: shadow.initial_holdings.clone(),
                initial: shadow.initial.mark.clone(),
                ledger: shadow.ledger.clone(),
                flow_events: (shadow.flow_events as u64).into(),
                flows: changes.remove(id).unwrap_or_default(),
                value_gap: difference(&actual, &hodl.mark, p.limits.rational_bits),
                excess_pnl: difference(&actual_pnl, &hodl_pnl, p.limits.rational_bits),
                actual,
                actual_pnl,
                hodl,
                hodl_pnl,
                inflows: shadow.inflows.clone(),
                outflows: shadow.outflows.clone(),
            });
        }
        emit(Boundary {
            provenance: provenance.clone(),
            block: world.block,
            timestamp: world.timestamp,
            portfolios,
        })?;
    }
    Ok(())
}
pub fn at(run: &Run, boundary: usize) -> Result<Boundary, String> {
    let mut result = None;
    visit(run, boundary, true, |b| {
        result = Some(b);
        Ok(())
    })?;
    result.ok_or_else(|| "missing HODL boundary".into())
}
