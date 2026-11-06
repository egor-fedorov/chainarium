pub mod baselines;
pub mod liquidator;
use crate::{
    Amount, engine,
    model::*,
    routing::{Observation, Route, search},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArbitragePolicy {
    pub account: String,
    pub asset: String,
    pub input: Amount,
    pub min_profit: Amount,
    pub resource_limit: Amount,
    pub resource_price: Amount,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_limits: Option<Vec<crate::routing::heterogeneous::DirectionalLimit>>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum DecisionAction {
    Hold,
    Skip,
    Submit,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub action: DecisionAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<baselines::Details>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executed_route: Option<crate::routing::heterogeneous::RouteResult>,
    pub agent: String,
    pub observed_block: Amount,
    pub route: Option<Route>,
    pub transaction: Option<Transaction>,
    pub expected_net_gain: Option<Amount>,
    pub reason: String,
    pub search_truncated: bool,
    pub candidates: Amount,
}
impl Decision {
    pub fn new(agent: &str, obs: &Observation, reason: &str) -> Self {
        Self {
            agent: agent.into(),
            observed_block: obs.block,
            action: DecisionAction::Skip,
            details: None,
            executed_route: None,
            route: None,
            transaction: None,
            expected_net_gain: None,
            reason: reason.into(),
            search_truncated: false,
            candidates: Amount::ZERO,
        }
    }
}
pub struct Simulation {
    pub success: bool,
    pub final_balance: Amount,
    pub receipt: Option<Receipt>,
    pub error: Option<String>,
}
pub trait AgentPolicy {
    fn decide(
        &self,
        agent: &str,
        chain: &Chain,
        observation: &Observation,
        simulate: &dyn Fn(&Transaction) -> Result<Simulation, String>,
    ) -> Result<Decision, String>;
}
impl AgentPolicy for ArbitragePolicy {
    fn decide(
        &self,
        agent: &str,
        chain: &Chain,
        obs: &Observation,
        simulate: &dyn Fn(&Transaction) -> Result<Simulation, String>,
    ) -> Result<Decision, String> {
        let mut decision = Decision::new(agent, obs, "no profitable executable cycle");
        if self.asset != chain.fee_asset {
            decision.reason = "fee valuation unavailable in input asset".into();
            return Ok(decision);
        }
        let initial = obs.balances.get(&self.asset).copied().unwrap_or_default();
        let budget = self.resource_limit.checked_mul(self.resource_price)?;
        if initial < self.input.checked_add(budget)? {
            decision.reason = "insufficient own capital plus fee reserve".into();
            return Ok(decision);
        }
        if let Some(limits) = &self.route_limits {
            use crate::routing::heterogeneous::{self as h, Trial};
            let query = h::Query {
                agent: agent.into(),
                sender: self.account.clone(),
                asset_in: self.asset.clone(),
                asset_out: self.asset.clone(),
                amount: self.input,
                min_out: 1.into(),
                residual_policy: h::ResidualPolicy::RequireFull,
                resource_limit: self.resource_limit,
                resource_price: self.resource_price,
                limits: limits.clone(),
            };
            let guard = Call::AssertTransactionNetGain {
                asset: self.asset.clone(),
                minimum: self.min_profit,
            };
            let search = h::search_with_tail(obs, chain, &query, &[guard], &|tx| {
                let sim = simulate(tx)?;
                Ok(match sim.receipt {
                    Some(receipt) => Trial::Executed(Box::new(receipt)),
                    None => Trial::Rejected(
                        sim.error
                            .unwrap_or_else(|| "policy simulation rejected".into()),
                    ),
                })
            })?;
            decision.candidates = search.examined;
            decision.search_truncated = search.truncated;
            for candidate in search.candidates {
                let Some(route) = candidate.result() else {
                    continue;
                };
                let receipt = candidate.receipt.as_ref().unwrap();
                let fee = if self.account == chain.fee_account {
                    Amount::ZERO
                } else {
                    receipt.fee
                };
                let gain = route.output.checked_sub(self.input)?.checked_sub(fee)?;
                if decision.expected_net_gain.is_none_or(|old| gain > old) {
                    decision.action = DecisionAction::Submit;
                    decision.executed_route = Some(route.clone());
                    decision.expected_net_gain = Some(gain);
                    let mut tx = candidate.transaction;
                    tx.id = format!("policy:{agent}:{}", obs.block.checked_add(1.into())?);
                    decision.transaction = Some(tx);
                    decision.reason = "profitable full-input cycle after execution fee".into();
                }
            }
            return Ok(decision);
        }
        let search = search(obs, &self.asset, &self.asset, self.input);
        decision.search_truncated = search.truncated;
        decision.candidates = search.examined;
        for route in search.routes {
            // The final guard projects actual fee/refund after all metered solver work.
            // It remains valid if earlier transactions change the iteration count.
            let mut program = route.calls(self.input, 1.into(), "");
            program.push(Call::AssertTransactionNetGain {
                asset: self.asset.clone(),
                minimum: self.min_profit,
            });
            let tx = Transaction {
                id: format!("policy:{agent}:{}", obs.block.checked_add(1.into())?),
                agent: agent.into(),
                sender: self.account.clone(),
                chain: chain.id.clone(),
                nonce: obs.nonce,
                limit: self.resource_limit,
                price: self.resource_price,
                deadline: obs.timestamp.checked_add(chain.interval)?,
                program,
            };
            let result = simulate(&tx)?;
            if !result.success || result.final_balance < initial {
                continue;
            }
            let gain = result.final_balance.checked_sub(initial)?;
            if gain < self.min_profit {
                continue;
            }
            if decision.expected_net_gain.is_none_or(|old| gain > old) {
                decision.action = DecisionAction::Submit;
                decision.expected_net_gain = Some(gain);
                decision.route = Some(route);
                decision.transaction = Some(tx);
                decision.reason = "profitable cycle after execution fee".into();
            }
        }
        Ok(decision)
    }
}
/// Controller supplies the restricted observation and a read-only executor simulation.
/// The policy itself never receives a World or Scenario.
pub fn decisions(s: &Scenario, parent: &World) -> Result<Vec<Decision>, String> {
    decisions_unless(s, parent, &|_, _, _| false)
}
pub(crate) fn decisions_unless(
    s: &Scenario,
    parent: &World,
    busy: &dyn Fn(&str, &str, Amount) -> bool,
) -> Result<Vec<Decision>, String> {
    let mut result = vec![];
    let agents: std::collections::BTreeSet<_> = s
        .policies
        .keys()
        .chain(s.lp_policies.keys())
        .chain(s.mm_policies.keys())
        .collect();
    for agent in agents {
        let (policy, account, asset): (&dyn AgentPolicy, &str, &str) =
            if let Some(p) = s.policies.get(agent) {
                (p, &p.account, &p.asset)
            } else if let Some(p) = s.lp_policies.get(agent) {
                (p, &p.account, &s.chain.fee_asset)
            } else {
                let p = &s.mm_policies[agent];
                (p, &p.account, &s.chain.fee_asset)
            };
        let obs = Observation::capture(s, parent, account)?;
        if busy(agent, account, obs.nonce) {
            result.push(Decision::new(
                agent,
                &obs,
                "unfinished own intent at current nonce",
            ));
            continue;
        }
        let simulate = |tx: &Transaction| -> Result<Simulation, String> {
            let b = engine::build_block(s, parent, std::slice::from_ref(tx))?;
            Ok(Simulation {
                success: b.receipts.first().is_some_and(|r| r.status == "Success"),
                final_balance: b.state.balance(account, asset),
                error: b
                    .receipts
                    .first()
                    .and_then(|r| r.error.clone())
                    .or_else(|| b.rejected.first().map(|r| r.reason.clone())),
                receipt: b.receipts.into_iter().next(),
            })
        };
        result.push(policy.decide(agent, &s.chain, &obs, &simulate)?);
    }
    Ok(result)
}
