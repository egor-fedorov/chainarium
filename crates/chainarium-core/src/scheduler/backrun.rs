//! Accepted DEC-23: a bounded policy over an authorized trigger preview.
//! The controller owns snapshots; this module receives no World or Scenario in search.
use super::*;
use crate::{Receipt, routing::heterogeneous as h};

pub const MODEL: &str = "bounded-backrun-v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub account: String,
    pub inputs: Vec<Amount>,
    pub minimum_net_profit: Amount,
    pub bid: Amount,
    pub resource_limit: Amount,
    pub resource_price: Amount,
    pub payment_limit: Amount,
    pub payment_price: Amount,
    pub reaction_delay: Amount,
    pub lifetime: Amount,
    pub route_limits: Vec<h::DirectionalLimit>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub inputs: usize,
    pub hops: usize,
    pub venue_checks: usize,
    pub candidates_per_reaction: usize,
    pub candidates_per_round: usize,
    pub member_attempts: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            inputs: 8,
            hops: 3,
            venue_checks: 4096,
            candidates_per_reaction: 64,
            candidates_per_round: 64,
            member_attempts: 256,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub limits: Limits,
    pub policies: BTreeMap<String, Policy>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Metrics {
    pub prefixes: usize,
    pub candidates: usize,
    pub members: usize,
    pub venue_checks: usize,
    pub used: Amount,
    pub truncated: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub input: Amount,
    pub hops: Vec<h::VenueHop>,
    pub outcome: String,
    pub net: Option<Amount>,
    pub used: Amount,
    pub fees: Amount,
    pub attempts: usize,
    pub receipts: Vec<Receipt>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub agent: String,
    pub trigger: String,
    pub observed_block: Amount,
    pub at: Amount,
    pub action: crate::policy::DecisionAction,
    pub reason: String,
    pub prefix: Option<Receipt>,
    pub candidates: Vec<Candidate>,
    pub selected: Option<usize>,
    pub truncated: bool,
    pub bundle: Option<bundles::Bundle>,
}
impl Decision {
    pub(super) fn new(agent: &str, trigger: &str, block: Amount, at: Amount) -> Self {
        Self {
            agent: agent.into(),
            trigger: trigger.into(),
            observed_block: block,
            at,
            action: crate::policy::DecisionAction::Skip,
            reason: "NoProfitableCandidate".into(),
            prefix: None,
            candidates: vec![],
            selected: None,
            truncated: false,
            bundle: None,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub metrics: Metrics,
    pub decisions: Vec<Decision>,
}

pub(super) fn validate(s: &Scenario, c: &super::Config) -> Result<(), String> {
    let has = s.model_versions.iter().any(|m| m == MODEL);
    let Some(cfg) = &c.backrun else {
        return if has {
            Err("backrun model without config".into())
        } else {
            Ok(())
        };
    };
    if !has
        || c.bundles.is_none()
        || cfg.policies.is_empty()
        || !s.model_versions.iter().any(|m| m == h::MODEL)
    {
        return Err("backrun requires model, policies, bundles and heterogeneous routes".into());
    }
    let values = serde_json::to_value(&cfg.limits).map_err(|e| e.to_string())?;
    let caps = serde_json::to_value(Limits::default()).map_err(|e| e.to_string())?;
    for (key, value) in values.as_object().ok_or("backrun limits")? {
        let n = value.as_u64().ok_or("backrun limit")?;
        if n == 0 || n > caps[key].as_u64().ok_or("backrun cap")? {
            return Err(format!("backrun limit outside cap: {key}"));
        }
    }
    for (agent, p) in &cfg.policies {
        if !s
            .agents
            .get(agent)
            .is_some_and(|a| a.controlled_accounts.contains(&p.account))
            || p.account == s.chain.fee_account
            || c.reactive.contains_key(agent)
            || s.policies.contains_key(agent)
            || s.lp_policies.contains_key(agent)
            || s.mm_policies.contains_key(agent)
            || p.inputs.is_empty()
            || p.inputs.len() > cfg.limits.inputs
            || p.inputs.iter().any(|n| n.is_zero())
            || p.inputs.iter().collect::<BTreeSet<_>>().len() != p.inputs.len()
            || p.minimum_net_profit.is_zero()
            || p.lifetime.is_zero()
        {
            return Err("invalid/duplicate backrun policy".into());
        }
        h::validate_limits(&h::venues(s), &p.route_limits)?;
        p.minimum_net_profit
            .checked_add(p.bid)?
            .checked_add(p.payment_fee()?)?;
        for (limit, price) in [
            (p.resource_limit, p.resource_price),
            (p.payment_limit, p.payment_price),
        ] {
            s.validate_tx(&Transaction {
                id: "validation".into(),
                agent: agent.clone(),
                sender: p.account.clone(),
                chain: s.chain.id.clone(),
                nonce: Amount::ZERO,
                limit,
                price,
                deadline: Amount::ZERO,
                program: vec![],
            })?;
            if limit > s.chain.block_limit {
                return Err("unfit backrun declared limit".into());
            }
        }
    }
    if c.bundles
        .as_ref()
        .unwrap()
        .emissions
        .iter()
        .any(|e| e.bundle.id.starts_with("policy:backrun:"))
    {
        return Err("reserved backrun BundleId".into());
    }
    Ok(())
}
impl Policy {
    fn payment_fee(&self) -> Result<Amount, String> {
        if self.bid.is_zero() {
            Ok(Amount::ZERO)
        } else {
            self.payment_price.checked_mul(115.into())
        }
    }
}

pub(super) struct Trial {
    pub outcome: String,
    pub final_balance: Amount,
    pub attempts: usize,
    pub receipts: Vec<Receipt>,
}
pub(super) struct Search<'a> {
    pub policy: &'a Policy,
    pub chain: &'a crate::Chain,
    pub observation: &'a Observation,
    pub limits: &'a Limits,
    pub target: Amount,
    pub event_id: Amount,
}
impl Search<'_> {
    pub(super) fn run(
        &self,
        decision: &mut Decision,
        metrics: &mut Metrics,
        simulate: &mut dyn FnMut(&[Transaction]) -> Result<Trial, String>,
    ) -> Result<(), String> {
        let seed = crate::run::digest(
            &serde_json::to_vec(&(&decision.agent, &decision.trigger, self.event_id))
                .map_err(|e| e.to_string())?,
        );
        let id = format!("policy:backrun:{seed}");
        let deadline = decision.at.checked_add(self.policy.lifetime)?;
        let mut worker = Worker {
            search: self,
            decision,
            metrics,
            simulate,
            id,
            deadline,
            venues: self.observation.route_venues.clone(),
            input: Amount::ZERO,
        };
        worker.venues.sort_by(|a, b| a.id.cmp(&b.id));
        let mut inputs = self.policy.inputs.clone();
        inputs.sort();
        for input in inputs {
            worker.input = input;
            worker.visit(&self.chain.fee_asset, &mut vec![])?;
            if worker.decision.truncated {
                break;
            }
        }
        if worker.decision.bundle.is_some() {
            worker.decision.action = crate::policy::DecisionAction::Submit;
            worker.decision.reason = "ProfitableConditionalBundle".into();
        } else if worker.decision.truncated {
            worker.decision.reason = "SearchLimit".into();
        }
        Ok(())
    }
}
struct Worker<'a, 'b> {
    search: &'a Search<'a>,
    decision: &'b mut Decision,
    metrics: &'b mut Metrics,
    simulate: &'b mut dyn FnMut(&[Transaction]) -> Result<Trial, String>,
    id: String,
    deadline: Amount,
    venues: Vec<h::Venue>,
    input: Amount,
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
        let p = self.search.policy;
        let length = if p.bid.is_zero() { 1 } else { 2 };
        let caps = self.search.limits;
        if self.decision.candidates.len() >= caps.candidates_per_reaction
            || self.metrics.candidates >= caps.candidates_per_round
            || self.metrics.members + length > caps.member_attempts
        {
            self.stop();
            return Ok(());
        }
        self.metrics.candidates += 1;
        let obs = self.search.observation;
        let response = Transaction {
            id: format!("{}:response", self.id),
            agent: self.decision.agent.clone(),
            sender: p.account.clone(),
            chain: self.search.chain.id.clone(),
            nonce: obs.nonce,
            limit: p.resource_limit,
            price: p.resource_price,
            deadline: self.deadline,
            program: vec![
                Call::ExecuteRoute {
                    asset_in: self.search.chain.fee_asset.clone(),
                    amount: self.input,
                    hops: path.to_vec(),
                    min_out: 1.into(),
                    residual_policy: h::ResidualPolicy::RequireFull,
                },
                Call::AssertTransactionNetGain {
                    asset: self.search.chain.fee_asset.clone(),
                    minimum: p
                        .minimum_net_profit
                        .checked_add(p.bid)?
                        .checked_add(p.payment_fee()?)?,
                },
            ],
        };
        let mut txs = vec![response];
        if !p.bid.is_zero() {
            txs.push(Transaction {
                id: format!("{}:payment", self.id),
                agent: self.decision.agent.clone(),
                sender: p.account.clone(),
                chain: self.search.chain.id.clone(),
                nonce: obs.nonce.checked_add(1.into())?,
                limit: p.payment_limit,
                price: p.payment_price,
                deadline: self.deadline,
                program: vec![Call::Transfer {
                    to: self.search.chain.fee_account.clone(),
                    asset: self.search.chain.fee_asset.clone(),
                    amount: p.bid,
                }],
            });
        }
        let trial = (self.simulate)(&txs)?;
        self.metrics.members += trial.attempts;
        let mut used = Amount::ZERO;
        let mut fees = Amount::ZERO;
        for receipt in &trial.receipts {
            used = used.checked_add(receipt.used)?;
            fees = fees.checked_add(receipt.fee)?;
        }
        self.metrics.used = self.metrics.used.checked_add(used)?;
        let baseline = obs
            .balances
            .get(&self.search.chain.fee_asset)
            .copied()
            .unwrap_or_default();
        let net = if trial.outcome == "Success" {
            trial
                .final_balance
                .checked_sub(baseline)
                .ok()
                .filter(|n| *n >= p.minimum_net_profit)
        } else {
            None
        };
        let candidate = Candidate {
            input: self.input,
            hops: path.to_vec(),
            outcome: trial.outcome,
            net,
            used,
            fees,
            attempts: trial.attempts,
            receipts: trial.receipts,
        };
        let better = net.is_some()
            && self.decision.selected.is_none_or(|i| {
                let old = &self.decision.candidates[i];
                candidate
                    .net
                    .cmp(&old.net)
                    .reverse()
                    .then(candidate.used.cmp(&old.used))
                    .then(candidate.input.cmp(&old.input))
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
            let payment_tx_id = txs.get(1).map(|tx| tx.id.clone());
            let mut members = vec![bundles::Member::VisibleTxId {
                tx_id: self.decision.trigger.clone(),
            }];
            members.extend(txs.into_iter().map(|transaction| bundles::Member::Own {
                transaction: Box::new(transaction),
            }));
            self.decision.bundle = Some(bundles::Bundle {
                id: self.id.clone(),
                agent: self.decision.agent.clone(),
                members,
                min_block: self.search.target,
                max_block: self.search.target,
                deadline: self.deadline,
                allowed_reverts: BTreeSet::new(),
                bid: p.bid,
                payment_tx_id,
            });
            self.decision.selected = Some(self.decision.candidates.len());
        }
        self.decision.candidates.push(candidate);
        Ok(())
    }
}
