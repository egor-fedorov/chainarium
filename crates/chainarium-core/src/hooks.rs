//! Accepted DEC-25: finite typed effects over a restricted CPMM context.
use crate::{Amount, Call, Pool, Scenario, World};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MODEL: &str = "bounded-pool-hooks-v1";
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Point {
    BeforeSwap,
    AfterSwap,
    BeforeLiquidity,
    AfterLiquidity,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub swap_fee: bool,
    pub reject: bool,
    pub write_local: bool,
    pub spend_own: BTreeMap<String, Amount>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Instruction {
    FeeByInput {
        asset_in: String,
        threshold: Amount,
        below_bps: u16,
        at_or_above_bps: u16,
    },
    MaxInput {
        asset_in: String,
        maximum: Amount,
    },
    Reject {
        reason: String,
    },
    IncrementCounter,
    PaySender {
        asset: String,
        amount: Amount,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub pool: String,
    pub account: String,
    pub capabilities: Capabilities,
    pub programs: BTreeMap<Point, Vec<Instruction>>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub counter: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LiquidityResult {
    pub x: Amount,
    pub y: Amount,
    pub shares: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Action {
    Swap {
        asset_in: String,
        input: Amount,
        min_out: Amount,
        base_fee: u16,
        effective_fee: u16,
        output: Option<Amount>,
    },
    Add {
        max_x: Amount,
        max_y: Amount,
        min_shares: Amount,
        actual: Option<LiquidityResult>,
    },
    Remove {
        shares: Amount,
        min_x: Amount,
        min_y: Amount,
        actual: Option<LiquidityResult>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub hook: String,
    pub pool: String,
    pub point: Point,
    pub block: Amount,
    pub timestamp: Amount,
    pub sender: String,
    pub counter: Amount,
    pub balances: BTreeMap<String, Amount>,
    pub x: String,
    pub y: String,
    pub reserves: [Amount; 2],
    pub total_shares: Amount,
    pub sender_shares: Amount,
    pub action: Action,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    pub hook: String,
    pub point: Point,
    pub instruction: Option<Instruction>,
    pub observation: Option<Observation>,
    pub counter_before: Amount,
    pub counter_after: Amount,
    pub effective_fee: Option<u16>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PublicView {
    pub config: Config,
    pub counter: Amount,
    pub balances: BTreeMap<String, Amount>,
}
pub fn views(s: &Scenario, w: &World) -> BTreeMap<String, PublicView> {
    s.hooks
        .iter()
        .map(|(id, c)| {
            (
                id.clone(),
                PublicView {
                    config: c.clone(),
                    counter: w.hooks[id].counter,
                    balances: w.accounts[&c.account].balances.clone(),
                },
            )
        })
        .collect()
}
pub(crate) fn binding<'a>(s: &'a Scenario, pool: &str) -> Option<(&'a String, &'a Config)> {
    s.hooks.iter().find(|(_, c)| c.pool == pool)
}
pub(crate) fn authorize(c: &Config, point: Point, i: &Instruction) -> Result<(), String> {
    let ok = match i {
        Instruction::FeeByInput { .. } => point == Point::BeforeSwap && c.capabilities.swap_fee,
        Instruction::MaxInput { .. } => point == Point::BeforeSwap && c.capabilities.reject,
        Instruction::Reject { .. } => c.capabilities.reject,
        Instruction::IncrementCounter => c.capabilities.write_local,
        Instruction::PaySender { asset, amount } => c
            .capabilities
            .spend_own
            .get(asset)
            .is_some_and(|max| amount <= max),
    };
    if ok {
        Ok(())
    } else {
        Err("HookCapabilityDenied".into())
    }
}
pub(crate) enum Effect {
    None,
    Fee(u16),
    Counter(Amount),
    Pay(String, Amount),
}
impl Instruction {
    // Deliberately no World, Scenario, ledger authority, pending inputs, or callbacks.
    pub(crate) fn evaluate(&self, o: &Observation) -> Result<Effect, String> {
        match self {
            Self::FeeByInput {
                asset_in,
                threshold,
                below_bps,
                at_or_above_bps,
            } => {
                let Action::Swap {
                    asset_in: input,
                    input: amount,
                    ..
                } = &o.action
                else {
                    return Err("HookPointMismatch".into());
                };
                Ok(if input != asset_in {
                    Effect::None
                } else {
                    Effect::Fee(if amount < threshold {
                        *below_bps
                    } else {
                        *at_or_above_bps
                    })
                })
            }
            Self::MaxInput { asset_in, maximum } => {
                let Action::Swap {
                    asset_in: input,
                    input: amount,
                    ..
                } = &o.action
                else {
                    return Err("HookPointMismatch".into());
                };
                if input == asset_in && amount > maximum {
                    Err("HookInputLimit".into())
                } else {
                    Ok(Effect::None)
                }
            }
            Self::Reject { reason } => Err(format!("HookReject: {reason}")),
            Self::IncrementCounter => Ok(Effect::Counter(
                o.counter
                    .checked_add(1.into())
                    .map_err(|_| "HookStateExhausted")?,
            )),
            Self::PaySender { asset, amount } => Ok(Effect::Pay(asset.clone(), *amount)),
        }
    }
}
pub(crate) fn validate<'a>(
    s: &'a Scenario,
    protected: &mut BTreeSet<&'a String>,
) -> Result<(), String> {
    let has = s.model_versions.iter().any(|v| v == MODEL);
    if (!s.hooks.is_empty() && !has) || s.hooks.len() > 64 {
        return Err("hook model/config or binding limit".into());
    }
    let mut pools = BTreeSet::new();
    for (id, c) in &s.hooks {
        let p = s
            .protocols
            .get(&c.pool)
            .ok_or("hooks require native CPMM")?;
        if id.is_empty()
            || !pools.insert(&c.pool)
            || !s.accounts.contains_key(&c.account)
            || c.account == s.chain.fee_account
            || !protected.insert(&c.account)
        {
            return Err("invalid hook binding/account".into());
        }
        if c.capabilities
            .spend_own
            .iter()
            .any(|(a, n)| !s.assets.contains_key(a) || n.is_zero())
        {
            return Err("invalid hook spend capability".into());
        }
        for (point, program) in &c.programs {
            if program.len() > 16 {
                return Err("hook instruction limit".into());
            }
            let mut fees = BTreeSet::new();
            let mut spend: BTreeMap<&String, Amount> = BTreeMap::new();
            for i in program {
                authorize(c, *point, i)?;
                match i {
                    Instruction::FeeByInput {
                        asset_in,
                        threshold,
                        below_bps,
                        at_or_above_bps,
                    } => {
                        if (asset_in != &p.x && asset_in != &p.y)
                            || threshold.is_zero()
                            || *below_bps >= 10000
                            || *at_or_above_bps >= 10000
                            || !fees.insert(asset_in)
                        {
                            return Err("invalid hook fee rule".into());
                        }
                    }
                    Instruction::MaxInput { asset_in, .. } => {
                        if asset_in != &p.x && asset_in != &p.y {
                            return Err("hook input asset".into());
                        }
                    }
                    Instruction::Reject { reason } => {
                        if reason.is_empty() || reason.len() > 128 {
                            return Err("hook rejection reason".into());
                        }
                    }
                    Instruction::PaySender { asset, amount } => {
                        if !s.assets.contains_key(asset) || amount.is_zero() {
                            return Err("invalid hook payment".into());
                        }
                        let total = spend.entry(asset).or_default();
                        *total = total.checked_add(*amount)?;
                        if *total > c.capabilities.spend_own[asset] {
                            return Err("hook spend cap".into());
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    if !s.hooks.is_empty() && s.policies.values().any(|p| p.route_limits.is_none()) {
        return Err("hooks require executor-based arbitrage route_limits".into());
    }
    Ok(())
}
pub(crate) fn invariant(s: &Scenario, w: &World) -> Result<(), String> {
    if w.hooks.keys().ne(s.hooks.keys()) {
        return Err("hook state identity invariant".into());
    }
    Ok(())
}
// Counts generated children at the actual pool-call depth, including route/flash expansion.
pub(crate) fn count_children(
    s: &Scenario,
    pool: &str,
    swap: bool,
    depth: usize,
    n: &mut usize,
) -> Result<(), String> {
    if let Some((_, c)) = binding(s, pool) {
        let points = if swap {
            [Point::BeforeSwap, Point::AfterSwap]
        } else {
            [Point::BeforeLiquidity, Point::AfterLiquidity]
        };
        for point in points {
            if let Some(p) = c.programs.get(&point).filter(|p| !p.is_empty()) {
                *n += 1 + p.len();
                if *n > 1024 || depth + 2 > 32 {
                    return Err("hook child node/depth limit".into());
                }
            }
        }
    }
    Ok(())
}
pub(crate) fn liquidity_action(
    call: &Call,
    actual: Option<LiquidityResult>,
) -> Option<(&str, Action)> {
    match call {
        Call::AddLiquidity {
            pool,
            max_x,
            max_y,
            min_shares,
        } => Some((
            pool,
            Action::Add {
                max_x: *max_x,
                max_y: *max_y,
                min_shares: *min_shares,
                actual,
            },
        )),
        Call::RemoveLiquidity {
            pool,
            shares,
            min_x,
            min_y,
        } => Some((
            pool,
            Action::Remove {
                shares: *shares,
                min_x: *min_x,
                min_y: *min_y,
                actual,
            },
        )),
        _ => None,
    }
}
pub(crate) fn observation(
    w: &World,
    id: &str,
    c: &Config,
    p: &Pool,
    point: Point,
    sender: &str,
    action: Action,
) -> Result<Observation, String> {
    Ok(Observation {
        hook: id.into(),
        pool: c.pool.clone(),
        point,
        block: w.block,
        timestamp: w.timestamp,
        sender: sender.into(),
        counter: w.hooks.get(id).ok_or("missing hook state")?.counter,
        balances: w.accounts[&c.account].balances.clone(),
        x: p.x.clone(),
        y: p.y.clone(),
        reserves: [w.balance(&p.account, &p.x), w.balance(&p.account, &p.y)],
        total_shares: w.shares[&c.pool],
        sender_shares: w.lp_balances[&c.pool]
            .get(sender)
            .copied()
            .unwrap_or_default(),
        action,
    })
}
