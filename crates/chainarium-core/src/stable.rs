//! DEC-16: two-asset StableSwap, exact integer math and metered Newton iterations.
use crate::{Amount, model::*};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MODEL: &str = "stableswap-2-v1";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct StablePool {
    pub pool: Pool,
    pub amplification: u32,
    pub iteration_limit: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Parameters {
    pub amplification: u32,
    pub iteration_limit: u16,
    pub fee_bps: u16,
    pub scale_x: Amount,
    pub scale_y: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SolverStep {
    pub solver: String,
    pub iteration: u16,
    pub before: String,
    pub after: String,
    pub converged: bool,
}
impl StablePool {
    pub fn parameters(&self, s: &Scenario) -> Result<Parameters, String> {
        let scale = |asset: &str| -> Result<Amount, String> {
            let d = s
                .assets
                .get(asset)
                .ok_or("unknown StableSwap asset")?
                .decimals;
            if d > 18 {
                return Err("StableSwap decimals must be 0..18".into());
            }
            Ok(10u64.pow(u32::from(18 - d)).into())
        };
        let p = Parameters {
            amplification: self.amplification,
            iteration_limit: self.iteration_limit,
            fee_bps: self.pool.fee_bps,
            scale_x: scale(&self.pool.x)?,
            scale_y: scale(&self.pool.y)?,
        };
        p.validate()?;
        Ok(p)
    }
}
impl Parameters {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=1_000_000).contains(&self.amplification)
            || !(1..=255).contains(&self.iteration_limit)
            || self.fee_bps >= 10000
            || self.scale_x.is_zero()
            || self.scale_y.is_zero()
        {
            Err("invalid StableSwap parameters".into())
        } else {
            Ok(())
        }
    }
    pub fn normalized(&self, raw: [Amount; 2]) -> Result<[Amount; 2], String> {
        self.validate()?;
        let x = raw[0]
            .checked_mul(self.scale_x)
            .map_err(|_| "StableSwap normalized domain")?;
        let y = raw[1]
            .checked_mul(self.scale_y)
            .map_err(|_| "StableSwap normalized domain")?;
        x.checked_add(y)
            .map_err(|_| "StableSwap normalized domain")?;
        if x.is_zero() != y.is_zero() {
            return Err("StableSwap one-sided reserve".into());
        }
        Ok([x, y])
    }
}
pub fn validate<'a>(s: &'a Scenario, protected: &mut BTreeSet<&'a String>) -> Result<(), String> {
    if !s.stable_pools.is_empty() && !s.model_versions.iter().any(|v| v == MODEL) {
        return Err("missing StableSwap model version".into());
    }
    for (id, cfg) in &s.stable_pools {
        let p = &cfg.pool;
        if id.is_empty()
            || s.protocols.contains_key(id)
            || s.flash_providers.contains_key(id)
            || s.lending.contains_key(id)
            || p.x == p.y
            || !s.accounts.contains_key(&p.account)
            || !s.accounts.contains_key(&p.owner)
            || s.protected(&p.owner)
            || !protected.insert(&p.account)
        {
            return Err("invalid StableSwap genesis".into());
        }
        let params = cfg.parameters(s)?;
        let raw = [
            s.accounts[&p.account]
                .balances
                .get(&p.x)
                .copied()
                .unwrap_or_default(),
            s.accounts[&p.account]
                .balances
                .get(&p.y)
                .copied()
                .unwrap_or_default(),
        ];
        let xp = params.normalized(raw)?;
        invariant(xp, &params, &mut || Ok::<(), String>(()), &mut vec![])?;
    }
    Ok(())
}
fn wide(n: BigUint) -> Result<BigUint, String> {
    if n.bits() > 1024 {
        Err("StableSwap intermediate exceeds 1024 bits".into())
    } else {
        Ok(n)
    }
}
fn close(a: &BigUint, b: &BigUint) -> bool {
    let difference = if a >= b { a - b } else { b - a };
    difference <= BigUint::from(1u8)
}
/// Domain is checked before every solver. Charge occurs before any iteration arithmetic.
pub fn invariant<E: From<String>>(
    xp: [Amount; 2],
    p: &Parameters,
    charge: &mut impl FnMut() -> Result<(), E>,
    steps: &mut Vec<SolverStep>,
) -> Result<Amount, E> {
    p.validate()?;
    let sum = xp[0].checked_add(xp[1])?;
    if sum.is_zero() {
        return Ok(Amount::ZERO);
    }
    if xp.iter().any(|x| x.is_zero()) {
        return Err("StableSwap one-sided reserve".to_string().into());
    }
    let x = xp[0].big();
    let y = xp[1].big();
    let s = sum.big();
    let ann = BigUint::from(2u64 * u64::from(p.amplification));
    let two = BigUint::from(2u8);
    let three = BigUint::from(3u8);
    let one = BigUint::from(1u8);
    let mut d = s.clone();
    for i in 1..=p.iteration_limit {
        charge()?;
        let mut dp = wide(&d * &d)? / (&two * &x);
        dp = wide(dp * &d)? / (&two * &y);
        let next =
            wide(wide(&ann * &s + &two * &dp)? * &d)? / wide((&ann - &one) * &d + &three * &dp)?;
        let converged = close(&d, &next);
        steps.push(SolverStep {
            solver: "D".into(),
            iteration: i,
            before: d.to_string(),
            after: next.to_string(),
            converged,
        });
        if converged {
            return Ok(Amount::from_big(next)?);
        }
        d = next;
    }
    Err("SolverDidNotConverge: D".to_string().into())
}
fn solve_y<E: From<String>>(
    x: Amount,
    d: Amount,
    p: &Parameters,
    charge: &mut impl FnMut() -> Result<(), E>,
    steps: &mut Vec<SolverStep>,
) -> Result<Amount, E> {
    let x = x.big();
    let d = d.big();
    let ann = BigUint::from(2u64 * u64::from(p.amplification));
    let two = BigUint::from(2u8);
    let c = wide(&d * &d)? / (&two * &x);
    let c = wide(c * &d)? / (&two * &ann);
    let b = x + &d / &ann;
    let mut y = d.clone();
    for i in 1..=p.iteration_limit {
        charge()?;
        let denominator = &two * &y + &b;
        if denominator <= d {
            return Err("StableSwap nonpositive y denominator".to_string().into());
        }
        let next = wide(wide(&y * &y)? + &c)? / (denominator - &d);
        let converged = close(&y, &next);
        steps.push(SolverStep {
            solver: "y".into(),
            iteration: i,
            before: y.to_string(),
            after: next.to_string(),
            converged,
        });
        if converged {
            return Ok(Amount::from_big(next)?);
        }
        y = next;
    }
    Err("SolverDidNotConverge: y".to_string().into())
}
pub fn output<E: From<String>>(
    raw: [Amount; 2],
    input_x: bool,
    amount: Amount,
    p: &Parameters,
    charge: &mut impl FnMut() -> Result<(), E>,
    steps: &mut Vec<SolverStep>,
) -> Result<Amount, E> {
    let xp = p.normalized(raw)?;
    if amount.is_zero() || xp[0].is_zero() {
        return Err("zero input or no StableSwap liquidity".to_string().into());
    }
    let d = invariant(xp, p, charge, steps)?;
    let (i, j, si, so) = if input_x {
        (0, 1, p.scale_x, p.scale_y)
    } else {
        (1, 0, p.scale_y, p.scale_x)
    };
    // A transient x+dx+y may exceed the domain; the resulting committed balances may not.
    let next_x = xp[i].checked_add(amount.checked_mul(si)?)?;
    let y = solve_y(next_x, d, p, charge, steps)?;
    let dy = xp[j].checked_sub(y)?.checked_sub(1.into())?;
    let net = dy.mul_div((10000 - u64::from(p.fee_bps)).into(), 10000.into(), false)?;
    let out = net.mul_div(1.into(), so, false)?;
    if out.is_zero() || out >= raw[j] {
        return Err("unexecutable StableSwap output".to_string().into());
    }
    let mut after = raw;
    after[i] = after[i].checked_add(amount)?;
    after[j] = after[j].checked_sub(out)?;
    p.normalized(after)?;
    Ok(out)
}
