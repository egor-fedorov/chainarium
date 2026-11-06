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
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub agent: String,
    pub observed_block: Amount,
    pub route: Option<Route>,
    pub transaction: Option<Transaction>,
    pub expected_net_gain: Option<Amount>,
    pub reason: String,
    pub search_truncated: bool,
    pub candidates: Amount,
}
pub struct Simulation {
    pub success: bool,
    pub final_balance: Amount,
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
        let mut decision = Decision {
            agent: agent.into(),
            observed_block: obs.block,
            route: None,
            transaction: None,
            expected_net_gain: None,
            reason: "no profitable executable cycle".into(),
            search_truncated: false,
            candidates: Amount::ZERO,
        };
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
        let search = search(obs, &self.asset, &self.asset, self.input);
        decision.search_truncated = search.truncated;
        decision.candidates = search.examined;
        for route in search.routes {
            // The final guard projects actual fee/refund after all metered solver work.
            // It remains valid if earlier transactions change the iteration count.
            let mut program = route.calls(self.input, 1.into(), "");
            program.push(Call::AssertNetBalanceDelta {
                asset: self.asset.clone(),
                baseline: initial,
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
    let mut result = vec![];
    for (agent, policy) in &s.policies {
        let obs = Observation::capture(s, parent, &policy.account)?;
        let simulate = |tx: &Transaction| -> Result<Simulation, String> {
            let b = engine::build_block(s, parent, std::slice::from_ref(tx))?;
            Ok(Simulation {
                success: b.receipts.first().is_some_and(|r| r.status == "Success"),
                final_balance: b.state.balance(&policy.account, &policy.asset),
            })
        };
        result.push(policy.decide(agent, &s.chain, &obs, &simulate)?);
    }
    Ok(result)
}
