//! Accepted DEC-24: periodic liquidation search over public protocol observations.
use crate::{
    Amount, Call, Chain, Receipt, Scenario, Transaction, World, credit,
    routing::{Observation, heterogeneous as h},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MODEL: &str = "bounded-liquidator-v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Funding {
    Own,
    Flash { provider: String },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub market: String,
    pub borrower: String,
    pub repay_sizes: Vec<Amount>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub account: String,
    pub targets: Vec<Target>,
    pub route_limits: Vec<h::DirectionalLimit>,
    pub minimum_net_profit: Amount,
    pub resource_limit: Amount,
    pub resource_price: Amount,
    pub lifetime: Amount,
    pub funding: Funding,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub targets: usize,
    pub sizes: usize,
    pub hops: usize,
    pub venue_checks: usize,
    pub trials_per_policy: usize,
    pub trials_per_round: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            targets: 16,
            sizes: 8,
            hops: 3,
            venue_checks: 4096,
            trials_per_policy: 64,
            trials_per_round: 64,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub limits: Limits,
    pub policies: BTreeMap<String, Policy>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TargetView {
    pub market: String,
    pub borrower: String,
    pub collateral_asset: String,
    pub debt_asset: String,
    pub collateral: Amount,
    pub debt: Amount,
    pub remainder: Amount,
    pub collateral_enabled: bool,
    pub ltv_bps: u16,
    pub lt_bps: u16,
    pub close_bps: u16,
    pub bonus_bps: u16,
    pub rate: credit::Rate,
    pub reserve_factor: Amount,
    pub last_accrual: Amount,
    pub oracle: credit::OracleUse,
    pub status: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FlashView {
    pub provider: String,
    pub asset: String,
    pub cash: Amount,
    pub fee_bps: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LiquidatorObservation {
    pub market: Observation,
    pub parent_block: Amount,
    pub parent_timestamp: Amount,
    pub targets: Vec<TargetView>,
    pub flash: Option<FlashView>,
}
impl LiquidatorObservation {
    pub(crate) fn capture(
        s: &Scenario,
        parent: &World,
        preview: &World,
        p: &Policy,
    ) -> Result<Self, String> {
        let mut targets = vec![];
        for t in &p.targets {
            let cfg = &s.lending[&t.market];
            let state = &preview.lending[&t.market];
            let position = &state.positions[&t.borrower];
            let collateral = preview.balance(&cfg.positions[&t.borrower], &cfg.collateral_asset);
            let oracle = credit::OracleUse::capture(s, cfg, preview);
            let status = if position.debt.is_zero() {
                "NoDebt"
            } else if oracle.status == "Unavailable" {
                "MissingOracle"
            } else if oracle.status != "Fresh" {
                "StaleOracle"
            } else if position.collateral_enabled
                && credit::risk_value(
                    collateral,
                    &oracle.record.as_ref().unwrap().price,
                    cfg.lt_bps,
                ) >= position.debt.big()
            {
                "Healthy"
            } else {
                "Liquidatable"
            };
            targets.push(TargetView {
                market: t.market.clone(),
                borrower: t.borrower.clone(),
                collateral_asset: cfg.collateral_asset.clone(),
                debt_asset: cfg.debt_asset.clone(),
                collateral,
                debt: position.debt,
                remainder: position.remainder,
                collateral_enabled: position.collateral_enabled,
                ltv_bps: cfg.ltv_bps,
                lt_bps: cfg.lt_bps,
                close_bps: cfg.close_bps,
                bonus_bps: cfg.bonus_bps,
                rate: cfg.rate.clone(),
                reserve_factor: cfg.reserve_factor,
                last_accrual: state.last_accrual,
                oracle,
                status: status.into(),
            });
        }
        targets.sort_by(|a, b| (&a.market, &a.borrower).cmp(&(&b.market, &b.borrower)));
        let flash = match &p.funding {
            Funding::Own => None,
            Funding::Flash { provider } => {
                let cfg = &s.flash_providers[provider];
                Some(FlashView {
                    provider: provider.clone(),
                    asset: cfg.asset.clone(),
                    cash: preview.balance(&cfg.account, &cfg.asset),
                    fee_bps: cfg.fee_bps,
                })
            }
        };
        Ok(Self {
            market: Observation::capture(s, preview, &p.account)?,
            parent_block: parent.block,
            parent_timestamp: parent.timestamp,
            targets,
            flash,
        })
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub targets: usize,
    pub trials: usize,
    pub venue_checks: usize,
    pub used: Amount,
    pub truncated: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub market: String,
    pub borrower: String,
    pub repayment: Amount,
    pub hops: Vec<h::VenueHop>,
    pub outcome: String,
    pub net: Option<Amount>,
    pub receipt: Option<Receipt>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub agent: String,
    pub account: String,
    pub parent_block: Amount,
    pub at: Amount,
    pub action: super::DecisionAction,
    pub reason: String,
    pub observation: Option<LiquidatorObservation>,
    pub candidates: Vec<Candidate>,
    pub selected: Option<usize>,
    pub truncated: bool,
    pub transaction: Option<Transaction>,
}
impl Decision {
    pub(crate) fn new(agent: &str, p: &Policy, parent: &World) -> Self {
        Self {
            agent: agent.into(),
            account: p.account.clone(),
            parent_block: parent.block,
            at: parent.timestamp,
            action: super::DecisionAction::Skip,
            reason: "NoProfitableCandidate".into(),
            observation: None,
            candidates: vec![],
            selected: None,
            truncated: false,
            transaction: None,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub metrics: Metrics,
    pub decisions: Vec<Decision>,
}
pub(crate) fn validate(s: &Scenario, c: &crate::scheduler::Config) -> Result<(), String> {
    let has = s.model_versions.iter().any(|m| m == MODEL);
    let Some(cfg) = &c.liquidator else {
        return if has {
            Err("liquidator model without config".into())
        } else {
            Ok(())
        };
    };
    if !has
        || !s
            .model_versions
            .iter()
            .any(|m| m == crate::liquidation::MODEL)
        || !s.model_versions.iter().any(|m| m == h::MODEL)
        || cfg.policies.is_empty()
    {
        return Err("liquidator requires models and policies".into());
    }
    let values = serde_json::to_value(&cfg.limits).map_err(|e| e.to_string())?;
    let caps = serde_json::to_value(Limits::default()).map_err(|e| e.to_string())?;
    for (key, value) in values.as_object().ok_or("liquidator limits")? {
        let n = value.as_u64().ok_or("liquidator limit")?;
        if n == 0 || n > caps[key].as_u64().ok_or("liquidator cap")? {
            return Err(format!("liquidator limit outside cap: {key}"));
        }
    }
    for (agent, p) in &cfg.policies {
        let a = s.agents.get(agent).ok_or("unknown liquidator agent")?;
        if !a.controlled_accounts.contains(&p.account)
            || p.account == s.chain.fee_account
            || p.minimum_net_profit.is_zero()
            || p.lifetime.is_zero()
            || s.policies.contains_key(agent)
            || s.lp_policies.contains_key(agent)
            || s.mm_policies.contains_key(agent)
            || c.reactive.contains_key(agent)
            || c.backrun
                .as_ref()
                .is_some_and(|b| b.policies.contains_key(agent))
            || p.targets.is_empty()
            || p.targets.len() > cfg.limits.targets
        {
            return Err("invalid/duplicate liquidator policy".into());
        }
        let mut seen = BTreeSet::new();
        for t in &p.targets {
            let market = s
                .lending
                .get(&t.market)
                .ok_or("unknown liquidator market")?;
            if !market.positions.contains_key(&t.borrower)
                || a.controlled_accounts.contains(&t.borrower)
                || market.debt_asset != s.chain.fee_asset
                || !seen.insert((&t.market, &t.borrower))
                || t.repay_sizes.is_empty()
                || t.repay_sizes.len() > cfg.limits.sizes
                || t.repay_sizes.iter().any(|n| n.is_zero())
                || t.repay_sizes.iter().collect::<BTreeSet<_>>().len() != t.repay_sizes.len()
            {
                return Err("invalid liquidator target/sizes".into());
            }
        }
        h::validate_limits(&h::venues(s), &p.route_limits)?;
        if let Funding::Flash { provider } = &p.funding {
            let flash = s
                .flash_providers
                .get(provider)
                .ok_or("unknown liquidator flash provider")?;
            if flash.asset != s.chain.fee_asset {
                return Err("liquidator flash asset mismatch".into());
            }
        }
        s.validate_tx(&Transaction {
            id: "validation".into(),
            agent: agent.clone(),
            sender: p.account.clone(),
            chain: s.chain.id.clone(),
            nonce: Amount::ZERO,
            limit: p.resource_limit,
            price: p.resource_price,
            deadline: Amount::ZERO,
            program: vec![],
        })?;
        if p.resource_limit > s.chain.block_limit {
            return Err("unfit liquidator declared limit".into());
        }
    }
    Ok(())
}

pub(crate) struct Trial {
    pub receipt: Option<Receipt>,
    pub outcome: String,
    pub final_balance: Amount,
}
pub(crate) struct Search<'a> {
    pub policy: &'a Policy,
    pub chain: &'a Chain,
    pub limits: &'a Limits,
    pub observation: &'a LiquidatorObservation,
}
impl Search<'_> {
    pub(crate) fn run(
        &self,
        d: &mut Decision,
        metrics: &mut Metrics,
        simulate: &mut dyn FnMut(&Transaction) -> Result<Trial, String>,
    ) -> Result<(), String> {
        if self.observation.market.nonce.checked_add(1.into()).is_err() {
            d.reason = "NonceExhausted".into();
            return Ok(());
        }
        let Ok(deadline) = d.at.checked_add(self.policy.lifetime) else {
            d.reason = "DeadlineOverflow".into();
            return Ok(());
        };
        let mut worker = Worker {
            search: self,
            decision: d,
            metrics,
            simulate,
            deadline,
            repayment: Amount::ZERO,
            target: 0,
            venues: self.observation.market.route_venues.clone(),
        };
        worker.venues.sort_by(|a, b| a.id.cmp(&b.id));
        for (index, target) in self.observation.targets.iter().enumerate() {
            worker.metrics.targets += 1;
            if target.status != "Liquidatable" {
                continue;
            }
            worker.target = index;
            let cfg = self
                .policy
                .targets
                .iter()
                .find(|t| t.market == target.market && t.borrower == target.borrower)
                .ok_or("missing observed target")?;
            let mut sizes = cfg.repay_sizes.clone();
            sizes.sort();
            for size in sizes {
                worker.repayment = size;
                worker.visit(&target.collateral_asset, &mut vec![])?;
                if worker.decision.truncated {
                    break;
                }
            }
            if worker.decision.truncated {
                break;
            }
        }
        if worker.decision.transaction.is_some() {
            worker.decision.action = super::DecisionAction::Submit;
            worker.decision.reason = "ProfitableLiquidation".into();
        } else if worker.decision.truncated {
            worker.decision.reason = "SearchLimit".into();
        } else if self
            .observation
            .targets
            .iter()
            .all(|t| t.status != "Liquidatable")
        {
            worker.decision.reason = "NoLiquidatableTarget".into();
        }
        Ok(())
    }
}
struct Worker<'a, 'b> {
    search: &'a Search<'a>,
    decision: &'b mut Decision,
    metrics: &'b mut Metrics,
    simulate: &'b mut dyn FnMut(&Transaction) -> Result<Trial, String>,
    deadline: Amount,
    repayment: Amount,
    target: usize,
    venues: Vec<h::Venue>,
}
impl Worker<'_, '_> {
    fn stop(&mut self) {
        self.decision.truncated = true;
        self.metrics.truncated = true;
    }
    fn visit(&mut self, asset: &str, path: &mut Vec<h::VenueHop>) -> Result<(), String> {
        if path.len() >= self.search.limits.hops {
            return Ok(());
        }
        for index in 0..self.venues.len() {
            if self.metrics.venue_checks >= self.search.limits.venue_checks {
                self.stop();
                break;
            }
            self.metrics.venue_checks += 1;
            let v = &self.venues[index];
            if path.iter().any(|h| h.venue() == v.id) {
                continue;
            }
            let other = if asset == v.x {
                v.y.clone()
            } else if asset == v.y {
                v.x.clone()
            } else {
                continue;
            };
            let hop = match v.family {
                h::Family::Cpmm | h::Family::StableSwap => h::VenueHop::Amm {
                    venue: v.id.clone(),
                },
                _ => {
                    let Some(l) = self
                        .search
                        .policy
                        .route_limits
                        .iter()
                        .find(|l| l.asset_in == asset && l.hop.venue() == v.id)
                    else {
                        continue;
                    };
                    l.hop.clone()
                }
            };
            path.push(hop);
            if other == self.search.chain.fee_asset {
                self.evaluate(path)?;
            } else {
                self.visit(&other, path)?;
            }
            path.pop();
            if self.decision.truncated {
                break;
            }
        }
        Ok(())
    }
    fn evaluate(&mut self, path: &[h::VenueHop]) -> Result<(), String> {
        if self.metrics.trials >= self.search.limits.trials_per_round
            || self.decision.candidates.len() >= self.search.limits.trials_per_policy
        {
            self.stop();
            return Ok(());
        }
        self.metrics.trials += 1;
        let p = self.search.policy;
        let target = &self.search.observation.targets[self.target];
        let sale = Call::LiquidateAndSell {
            market: target.market.clone(),
            borrower: target.borrower.clone(),
            max_repay: self.repayment,
            min_collateral: 1.into(),
            hops: path.to_vec(),
            min_out: 1.into(),
        };
        let action = match &p.funding {
            Funding::Own => sale,
            Funding::Flash { provider } => Call::FlashLoan {
                provider: provider.clone(),
                receiver: p.account.clone(),
                amount: self.repayment,
                callback: vec![sale],
            },
        };
        let tx = Transaction {
            id: format!(
                "policy:liquidator:{}:{}",
                self.decision.agent, self.search.observation.market.block
            ),
            agent: self.decision.agent.clone(),
            sender: p.account.clone(),
            chain: self.search.chain.id.clone(),
            nonce: self.search.observation.market.nonce,
            limit: p.resource_limit,
            price: p.resource_price,
            deadline: self.deadline,
            program: vec![
                action,
                Call::AssertTransactionNetGain {
                    asset: self.search.chain.fee_asset.clone(),
                    minimum: p.minimum_net_profit,
                },
            ],
        };
        let trial = (self.simulate)(&tx)?;
        let used = trial.receipt.as_ref().map_or(Amount::ZERO, |r| r.used);
        self.metrics.used = self.metrics.used.checked_add(used)?;
        let initial = self
            .search
            .observation
            .market
            .balances
            .get(&self.search.chain.fee_asset)
            .copied()
            .unwrap_or_default();
        let net = if trial.outcome == "Success" {
            trial
                .final_balance
                .checked_sub(initial)
                .ok()
                .filter(|n| *n >= p.minimum_net_profit)
        } else {
            None
        };
        let c = Candidate {
            market: target.market.clone(),
            borrower: target.borrower.clone(),
            repayment: self.repayment,
            hops: path.to_vec(),
            outcome: trial.outcome,
            net,
            receipt: trial.receipt,
        };
        let better = net.is_some()
            && self.decision.selected.is_none_or(|i| {
                let old = &self.decision.candidates[i];
                c.net
                    .cmp(&old.net)
                    .reverse()
                    .then(used.cmp(&old.receipt.as_ref().map_or(Amount::ZERO, |r| r.used)))
                    .then((&c.market, &c.borrower, c.repayment).cmp(&(
                        &old.market,
                        &old.borrower,
                        old.repayment,
                    )))
                    .then_with(|| {
                        c.hops
                            .iter()
                            .map(h::VenueHop::venue)
                            .cmp(old.hops.iter().map(h::VenueHop::venue))
                    })
                    .is_lt()
            });
        if better {
            self.decision.selected = Some(self.decision.candidates.len());
            self.decision.transaction = Some(tx);
        }
        self.decision.candidates.push(c);
        Ok(())
    }
}

#[cfg(test)]
mod observation_tests {
    use super::*;
    #[test]
    fn only_requested_positions_are_exposed_and_health_equality_is_not_liquidatable() {
        let s = Scenario::parse(include_bytes!(
            "../../../../scenarios/demos/sc-liquidator-own.json"
        ))
        .unwrap();
        let mut run = crate::Run::new(s).unwrap();
        run.step().unwrap();
        run.step().unwrap();
        let s = &run.scenario;
        let p = &s
            .scheduler
            .as_ref()
            .unwrap()
            .liquidator
            .as_ref()
            .unwrap()
            .policies["carol"];
        let parent = run.state.clone();
        let mut preview = crate::engine::begin_block(s, &parent).unwrap().state;
        let initial = LiquidatorObservation::capture(s, &parent, &preview, p).unwrap();
        std::sync::Arc::make_mut(&mut preview.accounts)
            .get_mut("bob")
            .unwrap()
            .balances
            .insert("Q".into(), 999999.into());
        let hidden = preview.lending["credit"].positions["bob"].clone();
        std::sync::Arc::make_mut(&mut preview.lending)
            .get_mut("credit")
            .unwrap()
            .positions
            .insert("hidden-position".into(), hidden);
        assert_eq!(
            LiquidatorObservation::capture(s, &parent, &preview, p).unwrap(),
            initial
        );
        let record = std::sync::Arc::make_mut(&mut preview.feeds)
            .get_mut("feed")
            .unwrap()
            .record
            .as_mut()
            .unwrap();
        record.price =
            crate::amount::Price::new("X".into(), "Q".into(), 50.into(), 3.into()).unwrap();
        assert_eq!(
            LiquidatorObservation::capture(s, &parent, &preview, p)
                .unwrap()
                .targets[0]
                .status,
            "Healthy"
        ); // 6 * (50/3) * 80% == 80
        std::sync::Arc::make_mut(&mut preview.feeds)
            .get_mut("feed")
            .unwrap()
            .record = None;
        assert_eq!(
            LiquidatorObservation::capture(s, &parent, &preview, p)
                .unwrap()
                .targets[0]
                .status,
            "MissingOracle"
        );
    }
}
