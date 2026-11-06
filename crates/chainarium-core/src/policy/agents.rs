//! Accepted DEC-26: bounded user, borrower and native publisher policies.
use super::DecisionAction;
use crate::{
    Amount, Call, Receipt, Scenario, Transaction, World, credit, routing, run::Streams,
    twap::FeedKind,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MODEL: &str = "baseline-agents-v1";

#[cfg(test)]
mod sampling_tests {
    use super::*;
    #[test]
    fn uniform_rejection_limits_and_full_u64_domain_have_no_modulo_bias() {
        let threshold = (((1u128 << 64) / 1000000) * 1000000) as u64;
        let mut draws = [threshold.into(), (threshold - 1).into()].into_iter();
        let sample = uniform(1000000, &mut || Ok(draws.next().unwrap())).unwrap();
        assert_eq!(sample.value, Some(999999.into()));
        assert_eq!(sample.draws.len(), 2);
        let sample = uniform(1000000, &mut || Ok(u64::MAX.into())).unwrap();
        assert_eq!(sample.value, None);
        assert_eq!(sample.draws.len(), 32);
        for size in [1, 2, 16] {
            let sample = uniform(size, &mut || Ok(u64::MAX.into())).unwrap();
            assert_eq!(sample.value, Some((u64::MAX % size).into()));
            assert_eq!(sample.draws.len(), 1);
        }
        assert!(uniform(0, &mut || panic!("empty domain must not draw")).is_err());
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Cadence {
    pub first_block: Amount,
    pub last_block: Amount,
    pub every: Amount,
}
impl Cadence {
    fn validate(&self, end: Amount) -> Result<(), String> {
        if self.first_block.is_zero()
            || self.first_block > self.last_block
            || self.last_block > end
            || self.every.is_zero()
        {
            return Err("invalid baseline cadence".into());
        }
        Ok(())
    }
    pub fn eligible(&self, block: Amount) -> bool {
        block >= self.first_block
            && block <= self.last_block
            && !self.every.is_zero()
            && block.checked_sub(self.first_block).unwrap().big() % self.every.big()
                == Amount::ZERO.big()
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Preflight {
    SuccessfulOnly,
    AnyReceipt,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScriptEvent {
    pub block: Amount,
    pub program: Vec<Call>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub block: Amount,
    pub debt: Amount,
    pub collateral: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Kind {
    ScriptedUser {
        schedule: Vec<ScriptEvent>,
        preflight: Preflight,
    },
    RandomUser {
        cadence: Cadence,
        probability_ppm: u32,
        programs: Vec<Vec<Call>>,
        preflight: Preflight,
    },
    BorrowerTarget {
        cadence: Cadence,
        market: String,
        targets: Vec<Target>,
        max_debt_step: Amount,
        max_collateral_step: Amount,
    },
    NativeOraclePublisher {
        cadence: Cadence,
        feeds: Vec<String>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub account: String,
    pub resource_limit: Amount,
    pub resource_price: Amount,
    pub lifetime: Amount,
    pub kind: Kind,
}
impl Policy {
    pub fn eligible(&self, block: Amount) -> bool {
        match &self.kind {
            Kind::ScriptedUser { schedule, .. } => schedule.iter().any(|e| e.block == block),
            Kind::RandomUser { cadence, .. }
            | Kind::BorrowerTarget { cadence, .. }
            | Kind::NativeOraclePublisher { cadence, .. } => cadence.eligible(block),
        }
    }
    pub fn preflight(&self) -> Preflight {
        match self.kind {
            Kind::ScriptedUser { preflight, .. } | Kind::RandomUser { preflight, .. } => preflight,
            _ => Preflight::SuccessfulOnly,
        }
    }
    pub fn name(&self) -> &'static str {
        match self.kind {
            Kind::ScriptedUser { .. } => "ScriptedUser",
            Kind::RandomUser { .. } => "RandomUser",
            Kind::BorrowerTarget { .. } => "BorrowerTarget",
            Kind::NativeOraclePublisher { .. } => "NativeOraclePublisher",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreditView {
    pub market: String,
    pub collateral: Amount,
    pub debt: Amount,
    pub remainder: Amount,
    pub collateral_enabled: bool,
    pub last_accrual: Amount,
    pub oracle: credit::OracleUse,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub parent_block: Amount,
    pub parent_timestamp: Amount,
    pub market: routing::Observation,
    pub credit: Option<CreditView>,
    pub native_feeds: BTreeMap<String, FeedKind>,
}
impl Observation {
    pub(crate) fn capture(
        s: &Scenario,
        parent: &World,
        preview: &World,
        p: &Policy,
    ) -> Result<Self, String> {
        let own = if let Kind::BorrowerTarget { market, .. } = &p.kind {
            let cfg = &s.lending[market];
            let state = &preview.lending[market];
            let position = &state.positions[&p.account];
            Some(CreditView {
                market: market.clone(),
                collateral: preview.balance(&cfg.positions[&p.account], &cfg.collateral_asset),
                debt: position.debt,
                remainder: position.remainder,
                collateral_enabled: position.collateral_enabled,
                last_accrual: state.last_accrual,
                oracle: credit::OracleUse::capture(s, cfg, preview),
            })
        } else {
            None
        };
        let native_feeds = if let Kind::NativeOraclePublisher { feeds, .. } = &p.kind {
            feeds
                .iter()
                .map(|id| (id.clone(), s.feeds[id].kind.clone()))
                .collect()
        } else {
            BTreeMap::new()
        };
        Ok(Self {
            parent_block: parent.block,
            parent_timestamp: parent.timestamp,
            market: routing::Observation::capture(s, preview, &p.account)?,
            credit: own,
            native_feeds,
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub size: Amount,
    pub draws: Vec<Amount>,
    pub value: Option<Amount>,
}
pub(crate) fn uniform(
    size: u64,
    draw: &mut impl FnMut() -> Result<Amount, String>,
) -> Result<Sample, String> {
    if size == 0 {
        return Err("zero RNG sample domain".into());
    }
    let bound = (1u128 << 64) / u128::from(size) * u128::from(size);
    let mut sample = Sample {
        size: size.into(),
        draws: vec![],
        value: None,
    };
    for _ in 0..32 {
        let value = draw()?;
        let x = value
            .to_string()
            .parse::<u64>()
            .map_err(|_| "RNG value exceeds u64")?;
        sample.draws.push(value);
        if u128::from(x) < bound {
            sample.value = Some((x % size).into());
            break;
        }
    }
    Ok(sample)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub stream: String,
    pub position_before: Amount,
    pub position_after: Amount,
    pub gate: Sample,
    pub template: Option<Sample>,
}
pub(crate) fn select(rng: &mut Streams, agent: &str, count: usize) -> Result<Selection, String> {
    let stream = format!("agent/{agent}");
    let position_before = rng.position(&stream);
    let gate = uniform(1000000, &mut || rng.draw(&stream))?;
    let template = if gate.value.is_some() {
        Some(uniform(count as u64, &mut || rng.draw(&stream))?)
    } else {
        None
    };
    Ok(Selection {
        position_after: rng.position(&stream),
        stream,
        position_before,
        gate,
        template,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub agent: String,
    pub policy: String,
    pub round: Amount,
    pub action: DecisionAction,
    pub reason: String,
    pub selection: Option<Selection>,
    pub observation: Option<Observation>,
    pub target: Option<Target>,
    pub program: Vec<Call>,
    pub trial: Option<Receipt>,
    pub transaction: Option<Transaction>,
}
impl Decision {
    pub fn new(agent: &str, p: &Policy, round: Amount) -> Self {
        Self {
            agent: agent.into(),
            policy: p.name().into(),
            round,
            action: DecisionAction::Hold,
            reason: "NotScheduled".into(),
            selection: None,
            observation: None,
            target: None,
            program: vec![],
            trial: None,
            transaction: None,
        }
    }
    pub(crate) fn skip(&mut self, reason: &str) {
        self.action = DecisionAction::Skip;
        self.reason = reason.into();
    }
}
impl Policy {
    /// Only the bounded observation and current sampled choices enter policy logic.
    pub(crate) fn plan(
        &self,
        d: &mut Decision,
        o: &Observation,
        tx_id: &str,
    ) -> Result<(), String> {
        d.reason = "TargetReached".into();
        d.program = match &self.kind {
            Kind::ScriptedUser { schedule, .. } => schedule
                .iter()
                .find(|e| e.block == d.round)
                .ok_or("missing scripted event")?
                .program
                .clone(),
            Kind::RandomUser {
                programs,
                probability_ppm,
                ..
            } => {
                let s = d.selection.as_ref().ok_or("missing random selection")?;
                let (Some(gate), Some(index)) =
                    (s.gate.value, s.template.as_ref().and_then(|v| v.value))
                else {
                    d.skip("RngBudget");
                    return Ok(());
                };
                if gate >= u64::from(*probability_ppm).into() {
                    d.reason = "RandomHold".into();
                    return Ok(());
                }
                let index = index
                    .to_string()
                    .parse::<usize>()
                    .map_err(|_| "template index overflow")?;
                programs.get(index).ok_or("invalid template index")?.clone()
            }
            Kind::BorrowerTarget {
                market,
                targets,
                max_debt_step,
                max_collateral_step,
                ..
            } => {
                let Some(target) = targets.iter().rev().find(|t| t.block <= d.round) else {
                    d.reason = "NoTarget".into();
                    return Ok(());
                };
                d.target = Some(target.clone());
                let own = o.credit.as_ref().ok_or("missing own credit observation")?;
                let call = if own.debt > target.debt {
                    Call::Repay {
                        market: market.clone(),
                        borrower: self.account.clone(),
                        amount: own.debt.checked_sub(target.debt)?.min(*max_debt_step),
                    }
                } else if own.collateral < target.collateral {
                    Call::DepositCollateral {
                        market: market.clone(),
                        amount: target
                            .collateral
                            .checked_sub(own.collateral)?
                            .min(*max_collateral_step),
                    }
                } else if own.debt < target.debt {
                    Call::Borrow {
                        market: market.clone(),
                        amount: target.debt.checked_sub(own.debt)?.min(*max_debt_step),
                    }
                } else if own.collateral > target.collateral {
                    Call::WithdrawCollateral {
                        market: market.clone(),
                        amount: own
                            .collateral
                            .checked_sub(target.collateral)?
                            .min(*max_collateral_step),
                    }
                } else {
                    return Ok(());
                };
                vec![call]
            }
            Kind::NativeOraclePublisher { .. } => {
                let mut calls = vec![];
                for (feed, kind) in &o.native_feeds {
                    let previous = o
                        .market
                        .feeds
                        .get(feed)
                        .ok_or("missing publisher feed observation")?;
                    let Some(sequence) = previous
                        .as_ref()
                        .map_or(Ok(1.into()), |r| r.sequence.checked_add(1.into()))
                        .ok()
                    else {
                        d.skip("OracleSequenceExhausted");
                        return Ok(());
                    };
                    let update_id = crate::run::digest(
                        &serde_json::to_vec(&(tx_id, feed)).map_err(|e| e.to_string())?,
                    );
                    calls.push(match kind {
                        FeedKind::NativeSpot { .. } => Call::PublishSpot {
                            feed: feed.clone(),
                            sequence,
                            update_id,
                        },
                        FeedKind::NativeTwap { .. } => Call::PublishTwap {
                            feed: feed.clone(),
                            sequence,
                            update_id,
                            end: o.market.timestamp,
                        },
                        FeedKind::External => {
                            return Err("external source in native publisher".into());
                        }
                    });
                }
                calls
            }
        };
        if !d.program.is_empty() {
            d.reason = "Planned".into();
        }
        Ok(())
    }
}
pub(crate) fn validate(s: &Scenario) -> Result<(), String> {
    let has = s.model_versions.iter().any(|m| m == MODEL);
    if s.baseline_policies.is_empty() {
        if has && s.scheduler.is_none() {
            return Err("baseline agents require event scheduler".into());
        }
        return Ok(());
    }
    let cfg = s
        .scheduler
        .as_ref()
        .ok_or("baseline agents require event scheduler")?;
    if !has {
        return Err("missing baseline agents model".into());
    }
    let end = (cfg.rounds as u64).into();
    for (agent, p) in &s.baseline_policies {
        if s.policies.contains_key(agent)
            || s.lp_policies.contains_key(agent)
            || s.mm_policies.contains_key(agent)
            || cfg.reactive.contains_key(agent)
            || cfg
                .backrun
                .as_ref()
                .is_some_and(|c| c.policies.contains_key(agent))
            || cfg
                .liquidator
                .as_ref()
                .is_some_and(|c| c.policies.contains_key(agent))
            || !s
                .agents
                .get(agent)
                .is_some_and(|a| a.controlled_accounts.contains(&p.account))
            || s.protected(&p.account)
            || p.lifetime.is_zero()
            || p.resource_limit < 100.into()
            || p.resource_limit > s.chain.block_limit
        {
            return Err("invalid/duplicate baseline policy account/envelope".into());
        }
        p.resource_limit.checked_mul(p.resource_price)?;
        let program = |calls: &[Call]| -> Result<(), String> {
            if calls.is_empty() {
                return Err("empty baseline program".into());
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
                program: calls.to_vec(),
            })
        };
        let schedule = |blocks: Vec<Amount>, ordered: bool| -> Result<(), String> {
            if blocks.len() > 1024
                || blocks.iter().any(|b| b.is_zero() || *b > end)
                || blocks.iter().collect::<BTreeSet<_>>().len() != blocks.len()
                || (ordered && blocks.windows(2).any(|b| b[0] >= b[1]))
            {
                return Err("baseline schedule order/range/limit".into());
            }
            Ok(())
        };
        match &p.kind {
            Kind::ScriptedUser {
                schedule: events, ..
            } => {
                schedule(events.iter().map(|e| e.block).collect(), false)?;
                for e in events {
                    program(&e.program)?;
                }
            }
            Kind::RandomUser {
                cadence,
                probability_ppm,
                programs,
                ..
            } => {
                cadence.validate(end)?;
                if *probability_ppm > 1000000 || programs.is_empty() || programs.len() > 16 {
                    return Err("random probability/template limit".into());
                }
                for p in programs {
                    program(p)?;
                }
            }
            Kind::BorrowerTarget {
                cadence,
                market,
                targets,
                max_debt_step,
                max_collateral_step,
            } => {
                cadence.validate(end)?;
                schedule(targets.iter().map(|t| t.block).collect(), true)?;
                if max_debt_step.is_zero()
                    || max_collateral_step.is_zero()
                    || !s
                        .lending
                        .get(market)
                        .is_some_and(|c| c.positions.contains_key(&p.account))
                {
                    return Err("invalid borrower market/steps".into());
                }
                program(&[Call::DepositCollateral {
                    market: market.clone(),
                    amount: 1.into(),
                }])?;
            }
            Kind::NativeOraclePublisher { cadence, feeds } => {
                cadence.validate(end)?;
                if feeds.is_empty()
                    || feeds.len() > 16
                    || feeds.iter().collect::<BTreeSet<_>>().len() != feeds.len()
                {
                    return Err("native publisher feed limit/duplicate".into());
                }
                let mut calls = vec![];
                for id in feeds {
                    let feed = s.feeds.get(id).ok_or("unknown publisher feed")?;
                    if !feed.publishers.contains(&p.account) {
                        return Err("unauthorized native publisher".into());
                    }
                    calls.push(match &feed.kind {
                        FeedKind::NativeSpot { .. } => Call::PublishSpot {
                            feed: id.clone(),
                            sequence: 1.into(),
                            update_id: "validation".into(),
                        },
                        FeedKind::NativeTwap { .. } => Call::PublishTwap {
                            feed: id.clone(),
                            sequence: 1.into(),
                            update_id: "validation".into(),
                            end: Amount::ZERO,
                        },
                        FeedKind::External => {
                            return Err("native publisher cannot publish external feed".into());
                        }
                    });
                }
                program(&calls)?;
            }
        }
    }
    Ok(())
}
