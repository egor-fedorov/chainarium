//! Accepted DEC-27: a fixed flash principal, bounded closed-cycle search and net guard.
use super::DecisionAction;
use crate::{
    Amount, Call, Chain, Receipt, Scenario, Transaction, World,
    routing::{self, heterogeneous as h},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const MODEL: &str = "bounded-flash-arbitrage-v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub account: String,
    pub asset: String,
    pub provider: String,
    pub principal: Amount,
    pub minimum_net_profit: Amount,
    pub resource_limit: Amount,
    pub resource_price: Amount,
    pub lifetime: Amount,
    pub route_limits: Vec<h::DirectionalLimit>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub hops: usize,
    pub trials_per_policy: usize,
    pub trials_per_round: usize,
    pub venue_checks: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            hops: 3,
            trials_per_policy: 64,
            trials_per_round: 64,
            venue_checks: 4096,
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
pub struct ProviderView {
    pub provider: String,
    pub asset: String,
    pub cash: Amount,
    pub fee_bps: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub parent_block: Amount,
    pub parent_timestamp: Amount,
    pub market: routing::Observation,
    pub provider: ProviderView,
    pub fee_reserve: Amount,
    pub flash_fee: Option<Amount>,
    pub repayment: Option<Amount>,
}
impl Observation {
    pub(crate) fn capture(
        s: &Scenario,
        parent: &World,
        preview: &World,
        p: &Policy,
    ) -> Result<Self, String> {
        let provider = &s.flash_providers[&p.provider];
        let flash_fee = p
            .principal
            .mul_div((provider.fee_bps as u64).into(), 10000.into(), true)
            .ok();
        Ok(Self {
            parent_block: parent.block,
            parent_timestamp: parent.timestamp,
            market: routing::Observation::capture(s, preview, &p.account)?,
            provider: ProviderView {
                provider: p.provider.clone(),
                asset: provider.asset.clone(),
                cash: preview.balance(&provider.account, &provider.asset),
                fee_bps: provider.fee_bps,
            },
            fee_reserve: p.resource_limit.checked_mul(p.resource_price)?,
            flash_fee,
            repayment: flash_fee.and_then(|fee| p.principal.checked_add(fee).ok()),
        })
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub trials: usize,
    pub venue_checks: usize,
    pub used: Amount,
    pub truncated: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub hops: Vec<h::VenueHop>,
    pub outcome: String,
    pub net: Option<Amount>,
    pub receipt: Option<Receipt>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub agent: String,
    pub round: Amount,
    pub action: DecisionAction,
    pub reason: String,
    pub observation: Option<Observation>,
    pub candidates: Vec<Candidate>,
    pub selected: Option<usize>,
    pub truncated: bool,
    pub transaction: Option<Transaction>,
    pub submission: Option<crate::scheduler::SubmissionOutcome>,
}
impl Decision {
    pub(crate) fn new(agent: &str, round: Amount) -> Self {
        Self {
            agent: agent.into(),
            round,
            action: DecisionAction::Skip,
            reason: "NoRoute".into(),
            observation: None,
            candidates: vec![],
            selected: None,
            truncated: false,
            transaction: None,
            submission: None,
        }
    }
    pub(crate) fn skip(&mut self, reason: &str) {
        self.action = DecisionAction::Skip;
        self.reason = reason.into();
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub metrics: Metrics,
    pub decisions: Vec<Decision>,
}
pub(crate) fn validate(s: &Scenario, c: &crate::scheduler::Config) -> Result<(), String> {
    let has = |model: &str| s.model_versions.iter().any(|m| m == model);
    let Some(cfg) = &c.flash_arbitrage else {
        return if has(MODEL) {
            Err("flash arbitrage model without config".into())
        } else {
            Ok(())
        };
    };
    if !has(MODEL) || !has(h::MODEL) || !has("strict-flash-provider-v1") || cfg.policies.is_empty()
    {
        return Err("flash arbitrage requires models and policies".into());
    }
    let caps = Limits::default();
    for (value, cap) in [
        (cfg.limits.hops, caps.hops),
        (cfg.limits.trials_per_policy, caps.trials_per_policy),
        (cfg.limits.trials_per_round, caps.trials_per_round),
        (cfg.limits.venue_checks, caps.venue_checks),
    ] {
        if value == 0 || value > cap {
            return Err("flash arbitrage limit outside cap".into());
        }
    }
    for (agent, p) in &cfg.policies {
        if !s
            .agents
            .get(agent)
            .is_some_and(|a| a.controlled_accounts.contains(&p.account))
            || s.protected(&p.account)
            || p.account == s.chain.fee_account
            || s.policies.contains_key(agent)
            || s.lp_policies.contains_key(agent)
            || s.mm_policies.contains_key(agent)
            || s.baseline_policies.contains_key(agent)
            || c.reactive.contains_key(agent)
            || c.backrun
                .as_ref()
                .is_some_and(|b| b.policies.contains_key(agent))
            || c.liquidator
                .as_ref()
                .is_some_and(|l| l.policies.contains_key(agent))
            || p.principal.is_zero()
            || p.minimum_net_profit.is_zero()
            || p.lifetime.is_zero()
            || p.asset != s.chain.fee_asset
            || p.resource_limit < 100.into()
            || p.resource_limit > s.chain.block_limit
        {
            return Err("invalid/duplicate flash arbitrage policy".into());
        }
        let provider = s
            .flash_providers
            .get(&p.provider)
            .ok_or("unknown arbitrage flash provider")?;
        if provider.asset != p.asset {
            return Err("flash arbitrage asset mismatch".into());
        }
        h::validate_limits(&h::venues(s), &p.route_limits)?;
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
    }
    Ok(())
}
impl Policy {
    fn transaction(
        &self,
        agent: &str,
        chain: &Chain,
        o: &Observation,
        deadline: Amount,
        hops: &[h::VenueHop],
    ) -> Transaction {
        Transaction {
            id: format!("policy:flash-arbitrage:{agent}:{}", o.market.block),
            agent: agent.into(),
            sender: self.account.clone(),
            chain: chain.id.clone(),
            nonce: o.market.nonce,
            limit: self.resource_limit,
            price: self.resource_price,
            deadline,
            program: vec![
                Call::FlashLoan {
                    provider: self.provider.clone(),
                    receiver: self.account.clone(),
                    amount: self.principal,
                    callback: vec![Call::ExecuteRoute {
                        asset_in: self.asset.clone(),
                        amount: self.principal,
                        hops: hops.to_vec(),
                        min_out: 1.into(),
                        residual_policy: h::ResidualPolicy::RequireFull,
                    }],
                },
                Call::AssertTransactionNetGain {
                    asset: self.asset.clone(),
                    minimum: self.minimum_net_profit,
                },
            ],
        }
    }
    /// The simulation capability accepts only the fixed DEC-27 program shape/envelope.
    pub(crate) fn permits(
        &self,
        tx: &Transaction,
        agent: &str,
        chain: &Chain,
        o: &Observation,
        deadline: Amount,
        max_hops: usize,
    ) -> bool {
        let [Call::FlashLoan { callback, .. }, _] = tx.program.as_slice() else {
            return false;
        };
        let [Call::ExecuteRoute { hops, .. }] = callback.as_slice() else {
            return false;
        };
        if hops.is_empty() || hops.len() > max_hops {
            return false;
        }
        let mut asset = self.asset.as_str();
        let mut seen = std::collections::BTreeSet::new();
        for (index, hop) in hops.iter().enumerate() {
            let Some(venue) = o.market.route_venues.iter().find(|v| v.id == hop.venue()) else {
                return false;
            };
            if !seen.insert(hop.venue()) {
                return false;
            }
            let allowed = match (&venue.family, hop) {
                (h::Family::Cpmm | h::Family::StableSwap, h::VenueHop::Amm { .. }) => true,
                _ => self
                    .route_limits
                    .iter()
                    .any(|limit| limit.asset_in == asset && limit.hop == *hop),
            };
            if !allowed {
                return false;
            }
            asset = if asset == venue.x {
                &venue.y
            } else if asset == venue.y {
                &venue.x
            } else {
                return false;
            };
            if asset == self.asset && index + 1 != hops.len() {
                return false;
            }
        }
        asset == self.asset && *tx == self.transaction(agent, chain, o, deadline, hops)
    }
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
    pub observation: &'a Observation,
}
impl Search<'_> {
    pub(crate) fn run(
        &self,
        d: &mut Decision,
        metrics: &mut Metrics,
        simulate: &mut dyn FnMut(&Transaction) -> Result<Trial, String>,
    ) -> Result<(), String> {
        let o = self.observation;
        let p = self.policy;
        if o.repayment.is_none() {
            d.skip("FlashRepaymentOverflow");
            return Ok(());
        }
        if o.provider.cash < p.principal {
            d.skip("ProviderCash");
            return Ok(());
        }
        if o.market.balances.get(&p.asset).copied().unwrap_or_default() < o.fee_reserve {
            d.skip("OwnFeeBudget");
            return Ok(());
        }
        let deadline = o.parent_timestamp.checked_add(p.lifetime)?;
        let mut worker = Worker {
            search: self,
            decision: d,
            metrics,
            simulate,
            deadline,
            venues: o.market.route_venues.clone(),
        };
        worker.venues.sort_by(|a, b| a.id.cmp(&b.id));
        worker.visit(&p.asset, &mut vec![])?;
        if worker.decision.transaction.is_some() {
            worker.decision.action = DecisionAction::Submit;
            worker.decision.reason = "ProfitableFlashCycle".into();
        } else if worker.decision.truncated {
            worker.decision.skip("SearchLimit");
        } else if !worker.decision.candidates.is_empty() {
            worker.decision.skip("NoProfitableCycle");
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
            let venue = &self.venues[index];
            if path.iter().any(|hop| hop.venue() == venue.id) {
                continue;
            }
            let other = if asset == venue.x {
                venue.y.clone()
            } else if asset == venue.y {
                venue.x.clone()
            } else {
                continue;
            };
            let hop = match venue.family {
                h::Family::Cpmm | h::Family::StableSwap => h::VenueHop::Amm {
                    venue: venue.id.clone(),
                },
                _ => {
                    let Some(limit) = self
                        .search
                        .policy
                        .route_limits
                        .iter()
                        .find(|l| l.asset_in == asset && l.hop.venue() == venue.id)
                    else {
                        continue;
                    };
                    limit.hop.clone()
                }
            };
            path.push(hop);
            if other == self.search.policy.asset {
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
        let tx = p.transaction(
            &self.decision.agent,
            self.search.chain,
            self.search.observation,
            self.deadline,
            path,
        );
        let trial = (self.simulate)(&tx)?;
        let used = trial.receipt.as_ref().map_or(Amount::ZERO, |r| r.used);
        self.metrics.used = self.metrics.used.checked_add(used)?;
        let initial = self
            .search
            .observation
            .market
            .balances
            .get(&p.asset)
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
        let candidate = Candidate {
            hops: path.to_vec(),
            outcome: trial.outcome,
            net,
            receipt: trial.receipt,
        };
        let better = net.is_some()
            && self.decision.selected.is_none_or(|i| {
                let old = &self.decision.candidates[i];
                candidate
                    .net
                    .cmp(&old.net)
                    .reverse()
                    .then(used.cmp(&old.receipt.as_ref().map_or(Amount::ZERO, |r| r.used)))
                    .then_with(|| {
                        candidate
                            .hops
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
        self.decision.candidates.push(candidate);
        Ok(())
    }
}

#[cfg(test)]
mod capability_tests {
    use super::*;
    #[test]
    fn simulation_capability_rejects_foreign_reads_changed_envelopes_and_noncycles() {
        let s = Scenario::parse(include_bytes!(
            "../../../../scenarios/demos/sc-flash-arb-success.json"
        ))
        .unwrap();
        let p = &s
            .scheduler
            .as_ref()
            .unwrap()
            .flash_arbitrage
            .as_ref()
            .unwrap()
            .policies["trader"];
        let parent = World::genesis(&s).unwrap();
        let preview = crate::engine::begin_block(&s, &parent).unwrap().state;
        let o = Observation::capture(&s, &parent, &preview, p).unwrap();
        let deadline = 20.into();
        let path = vec![
            h::VenueHop::Amm {
                venue: "cpmm".into(),
            },
            h::VenueHop::Amm {
                venue: "second".into(),
            },
        ];
        let tx = p.transaction("trader", &s.chain, &o, deadline, &path);
        assert!(p.permits(&tx, "trader", &s.chain, &o, deadline, 3));
        assert!(!p.permits(&tx, "trader", &s.chain, &o, deadline, 1));
        for (field, value) in [
            ("/sender", serde_json::json!("bob")),
            ("/nonce", serde_json::json!("1")),
            ("/deadline", serde_json::json!("21")),
            ("/price", serde_json::json!("0")),
            ("/program/0/receiver", serde_json::json!("bob")),
            ("/program/0/amount", serde_json::json!("1")),
            ("/program/1/minimum", serde_json::json!("0")),
            (
                "/program/0/callback/0",
                serde_json::json!({"type":"AssertBalance","account":"bob","asset":"Q","minimum":"1"}),
            ),
            (
                "/program/0/callback/0/residual_policy",
                serde_json::json!("KeepResiduals"),
            ),
        ] {
            let mut raw = serde_json::to_value(&tx).unwrap();
            *raw.pointer_mut(field).unwrap() = value;
            let changed = serde_json::from_value(raw).unwrap();
            assert!(
                !p.permits(&changed, "trader", &s.chain, &o, deadline, 3),
                "{field}"
            );
        }
        for path in [
            vec![path[0].clone()],
            vec![path[0].clone(), path[0].clone()],
        ] {
            let tx = p.transaction("trader", &s.chain, &o, deadline, &path);
            assert!(!p.permits(&tx, "trader", &s.chain, &o, deadline, 3));
        }
    }
}
