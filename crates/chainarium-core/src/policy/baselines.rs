use super::{AgentPolicy, Decision, DecisionAction, Simulation};
use crate::{Amount, Call, Chain, Scenario, Transaction, routing::Observation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const LP_MODEL: &str = "scheduled-clmm-lp-v1";
pub const MM_MODEL: &str = "bounded-book-mm-v1";
pub const MAX_OWNED: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClmmObservation {
    pub sqrt_price: Amount,
    pub current_tick: i32,
    pub active_liquidity: Amount,
    pub own_positions: BTreeMap<Amount, crate::clmm::Position>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BookObservation {
    pub config: crate::book::Config,
    pub own_orders: BTreeMap<Amount, crate::book::Order>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum LpTarget {
    Target {
        lower: i32,
        upper: i32,
        liquidity: Amount,
        max_x: Amount,
        max_q: Amount,
    },
    Exit,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LpEvent {
    pub block: Amount,
    pub target: LpTarget,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LpPolicy {
    pub account: String,
    pub pool: String,
    pub resource_limit: Amount,
    pub resource_price: Amount,
    pub schedule: Vec<LpEvent>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Anchor {
    Fixed { ticks: Amount },
    PublicOracle { feed: String, max_age: Amount },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MmPolicy {
    pub account: String,
    pub book: String,
    pub lots: Amount,
    pub spread_ticks: Amount,
    pub refresh_every: Amount,
    pub resource_limit: Amount,
    pub resource_price: Amount,
    pub anchor: Anchor,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AnchorUse {
    pub ticks: Option<Amount>,
    pub feed: Option<String>,
    pub record: Option<crate::credit::OracleRecord>,
    pub age: Option<Amount>,
    pub max_age: Option<Amount>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Details {
    pub policy: String,
    pub venue: String,
    pub next_block: Amount,
    pub owned_items: Amount,
    pub target: Option<LpTarget>,
    pub anchor: Option<AnchorUse>,
}

pub fn validate(s: &Scenario) -> Result<(), String> {
    let mut agents = BTreeSet::new();
    for (agent, p) in &s.policies {
        agents.insert(agent.clone());
        if let Some(limits) = &p.route_limits {
            if !s
                .model_versions
                .iter()
                .any(|m| m == crate::routing::heterogeneous::MODEL)
            {
                return Err("missing heterogeneous arbitrage model".into());
            }
            crate::routing::heterogeneous::validate_limits(
                &crate::routing::heterogeneous::venues(s),
                limits,
            )?;
        }
    }
    let mut envelope = |agent: &String,
                        account: &str,
                        model: &str,
                        limit: Amount,
                        price: Amount|
     -> Result<(), String> {
        if !agents.insert(agent.clone()) {
            return Err("only one policy per AgentId".into());
        }
        if !s.model_versions.iter().any(|m| m == model)
            || !s
                .agents
                .get(agent)
                .is_some_and(|a| a.controlled_accounts.contains(account))
            || s.protected(account)
            || limit < 100.into()
        {
            return Err("invalid policy account/model/resource limit".into());
        }
        limit.checked_mul(price)?;
        Ok(())
    };
    for (agent, p) in &s.lp_policies {
        envelope(
            agent,
            &p.account,
            LP_MODEL,
            p.resource_limit,
            p.resource_price,
        )?;
        if !s.clmm_pools.contains_key(&p.pool) || p.schedule.len() > 1024 {
            return Err("LP policy pool/schedule limit".into());
        }
        let mut blocks = BTreeSet::new();
        for event in &p.schedule {
            if event.block.is_zero() || !blocks.insert(event.block) {
                return Err("invalid/duplicate LP event block".into());
            }
            if let LpTarget::Target {
                lower,
                upper,
                liquidity,
                ..
            } = &event.target
                && (*lower < crate::clmm::math::MIN_TICK
                    || *upper > crate::clmm::math::MAX_TICK
                    || lower >= upper
                    || liquidity.is_zero())
            {
                return Err("invalid LP target range/liquidity".into());
            }
        }
    }
    for (agent, p) in &s.mm_policies {
        envelope(
            agent,
            &p.account,
            MM_MODEL,
            p.resource_limit,
            p.resource_price,
        )?;
        let cfg = s.books.get(&p.book).ok_or("unknown MM book")?;
        if p.lots.is_zero() || p.spread_ticks.is_zero() || p.refresh_every.is_zero() {
            return Err("zero MM lots/spread/cadence".into());
        }
        match &p.anchor {
            Anchor::Fixed { ticks } => {
                prices(*ticks, p.spread_ticks)?;
            }
            Anchor::PublicOracle { feed, .. } => {
                let cfg_feed = s.feeds.get(feed).ok_or("unknown MM public feed")?;
                if cfg_feed.base_asset != cfg.base_asset || cfg_feed.quote_asset != cfg.quote_asset
                {
                    return Err("MM oracle pair does not match book".into());
                }
            }
        }
    }
    Ok(())
}
fn prices(anchor: Amount, spread: Amount) -> Result<(Amount, Amount), String> {
    let bid = anchor
        .checked_sub(spread)
        .map_err(|_| "MM nonpositive bid")?;
    if bid.is_zero() {
        return Err("MM nonpositive bid".into());
    }
    Ok((bid, anchor.checked_add(spread)?))
}
fn transaction(
    agent: &str,
    chain: &Chain,
    obs: &Observation,
    limit: Amount,
    price: Amount,
    program: Vec<Call>,
) -> Result<Transaction, String> {
    Ok(Transaction {
        id: format!("policy:{agent}:{}", obs.block.checked_add(1.into())?),
        agent: agent.into(),
        sender: obs.account.clone(),
        chain: chain.id.clone(),
        nonce: obs.nonce,
        limit,
        price,
        deadline: obs.timestamp.checked_add(chain.interval)?,
        program,
    })
}
fn simulate_intent(
    mut decision: Decision,
    tx: Transaction,
    simulate: &dyn Fn(&Transaction) -> Result<Simulation, String>,
) -> Result<Decision, String> {
    decision.candidates = 1.into();
    let simulation = simulate(&tx)?;
    if simulation.success {
        decision.action = DecisionAction::Submit;
        decision.transaction = Some(tx);
    } else {
        decision.action = DecisionAction::Skip;
        decision.reason = format!(
            "simulation failed: {}",
            simulation
                .error
                .as_deref()
                .unwrap_or("unknown execution failure")
        );
    }
    Ok(decision)
}
impl AgentPolicy for LpPolicy {
    fn decide(
        &self,
        agent: &str,
        chain: &Chain,
        obs: &Observation,
        simulate: &dyn Fn(&Transaction) -> Result<Simulation, String>,
    ) -> Result<Decision, String> {
        let mut d = Decision::new(agent, obs, "no scheduled LP action");
        d.action = DecisionAction::Hold;
        let next = obs.block.checked_add(1.into())?;
        let pool = obs
            .clmm
            .get(&self.pool)
            .ok_or("LP pool absent from observation")?;
        d.details = Some(Details {
            policy: LP_MODEL.into(),
            venue: self.pool.clone(),
            next_block: next,
            owned_items: (pool.own_positions.len() as u64).into(),
            target: None,
            anchor: None,
        });
        let Some(event) = self.schedule.iter().find(|e| e.block == next) else {
            return Ok(d);
        };
        d.details.as_mut().unwrap().target = Some(event.target.clone());
        if pool.own_positions.len() > MAX_OWNED {
            d.action = DecisionAction::Skip;
            d.reason = "LP own-position limit 16".into();
            return Ok(d);
        }
        let mut program = vec![];
        for (id, p) in &pool.own_positions {
            program.push(Call::CollectPosition {
                pool: self.pool.clone(),
                position_id: *id,
            });
            if !p.liquidity.is_zero() {
                program.push(Call::BurnPosition {
                    pool: self.pool.clone(),
                    position_id: *id,
                    liquidity: p.liquidity,
                    min_x: Amount::ZERO,
                    min_q: Amount::ZERO,
                });
            }
        }
        if let LpTarget::Target {
            lower,
            upper,
            liquidity,
            max_x,
            max_q,
        } = event.target
        {
            program.push(Call::MintPosition {
                pool: self.pool.clone(),
                lower,
                upper,
                liquidity,
                max_x,
                max_q,
            });
        }
        if program.is_empty() {
            d.reason = "LP already exited".into();
            return Ok(d);
        }
        d.reason = "scheduled LP action".into();
        simulate_intent(
            d,
            transaction(
                agent,
                chain,
                obs,
                self.resource_limit,
                self.resource_price,
                program,
            )?,
            simulate,
        )
    }
}
impl AgentPolicy for MmPolicy {
    fn decide(
        &self,
        agent: &str,
        chain: &Chain,
        obs: &Observation,
        simulate: &dyn Fn(&Transaction) -> Result<Simulation, String>,
    ) -> Result<Decision, String> {
        use crate::book::{Side, TimeInForce};
        let mut d = Decision::new(agent, obs, "MM refresh not due");
        d.action = DecisionAction::Hold;
        let next = obs.block.checked_add(1.into())?;
        let book = obs
            .books
            .get(&self.book)
            .ok_or("MM book absent from observation")?;
        d.details = Some(Details {
            policy: MM_MODEL.into(),
            venue: self.book.clone(),
            next_block: next,
            owned_items: (book.own_orders.len() as u64).into(),
            target: None,
            anchor: None,
        });
        if next.big() % self.refresh_every.big() != num_bigint::BigUint::from(0u8) {
            return Ok(d);
        }
        if book.own_orders.len() > MAX_OWNED {
            d.action = DecisionAction::Skip;
            d.reason = "MM own-order limit 16".into();
            return Ok(d);
        }
        let anchor = match &self.anchor {
            Anchor::Fixed { ticks } => {
                d.details.as_mut().unwrap().anchor = Some(AnchorUse {
                    ticks: Some(*ticks),
                    feed: None,
                    record: None,
                    age: None,
                    max_age: None,
                });
                Some(*ticks)
            }
            Anchor::PublicOracle { feed, max_age } => {
                let record = obs.feeds.get(feed).cloned().flatten();
                let age = record
                    .as_ref()
                    .and_then(|r| obs.timestamp.checked_sub(r.observed_at).ok());
                let mut used = AnchorUse {
                    ticks: None,
                    feed: Some(feed.clone()),
                    record: record.clone(),
                    age,
                    max_age: Some(*max_age),
                };
                let value =
                    if let Some(r) = record.filter(|_| age.is_some_and(|age| age <= *max_age)) {
                        // Price is already quote atoms per base atom. At most 512 bits per product.
                        let n = r.price.numerator().big() * book.config.lot_atoms.big();
                        let denominator =
                            r.price.denominator().big() * book.config.quote_tick_atoms.big();
                        match Amount::from_big(n / denominator) {
                            Ok(ticks) => {
                                used.ticks = Some(ticks);
                                Some(ticks)
                            }
                            Err(e) => {
                                d.action = DecisionAction::Skip;
                                d.reason = format!("MM anchor: {e}");
                                d.details.as_mut().unwrap().anchor = Some(used);
                                return Ok(d);
                            }
                        }
                    } else {
                        None
                    };
                d.details.as_mut().unwrap().anchor = Some(used);
                value
            }
        };
        let desired = match anchor.map(|a| prices(a, self.spread_ticks)).transpose() {
            Ok(value) => value,
            Err(e) => {
                d.action = DecisionAction::Skip;
                d.reason = e;
                return Ok(d);
            }
        };
        if let Some((bid, ask)) = desired
            && book.own_orders.len() == 2
            && [(Side::Buy, bid), (Side::Sell, ask)]
                .iter()
                .all(|(side, price)| {
                    book.own_orders.values().any(|o| {
                        o.side == *side && o.price_ticks == *price && o.remaining_lots == self.lots
                    })
                })
        {
            d.reason = "MM quotes already match target".into();
            return Ok(d);
        }
        let mut program: Vec<_> = book
            .own_orders
            .keys()
            .map(|id| Call::CancelOrder {
                book: self.book.clone(),
                order_id: *id,
            })
            .collect();
        if let Some((bid, ask)) = desired {
            program.extend([(Side::Buy, bid), (Side::Sell, ask)].map(|(side, price)| {
                Call::PlaceOrder {
                    book: self.book.clone(),
                    side,
                    price_ticks: price,
                    lots: self.lots,
                    tif: TimeInForce::GTC,
                }
            }));
            d.reason = "MM replace quotes from observed anchor".into();
        } else {
            d.reason = "MM missing/stale oracle: cancel quotes".into();
        }
        if program.is_empty() {
            d.reason = "MM missing/stale oracle: no quotes to cancel".into();
            return Ok(d);
        }
        simulate_intent(
            d,
            transaction(
                agent,
                chain,
                obs,
                self.resource_limit,
                self.resource_price,
                program,
            )?,
            simulate,
        )
    }
}
