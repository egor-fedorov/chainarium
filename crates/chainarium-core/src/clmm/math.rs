use crate::Amount;
use num_bigint::BigUint;
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};

pub const MIN_TICK: i32 = -32768;
pub const MAX_TICK: i32 = 32768;
pub fn b96() -> BigUint {
    BigUint::from(1u8) << 96usize
}
pub fn b128() -> BigUint {
    BigUint::from(1u8) << 128usize
}

fn quotient(n: BigUint, d: BigUint, up: bool) -> Result<Amount, String> {
    if n.bits() > 1024 || d.bits() > 1024 || d == BigUint::from(0u8) {
        return Err("CLMM arithmetic domain".into());
    }
    let value = if up {
        (&n + &d - BigUint::from(1u8)) / &d
    } else {
        n / d
    };
    Amount::from_big(value)
}

/// Pure memoization: keys have a finite 65537-element domain. The cache is not
/// simulation state and cannot change output, resource cost, or RNG positions.
pub fn tick_sqrt(tick: i32) -> Result<Amount, String> {
    if !(MIN_TICK..=MAX_TICK).contains(&tick) {
        return Err("CLMM tick bounds".into());
    }
    static CACHE: OnceLock<Mutex<BTreeMap<i32, Amount>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    if let Some(value) = cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&tick)
        .copied()
    {
        return Ok(value);
    }
    let n = BigUint::from(10001u32).pow(tick.unsigned_abs());
    let d = BigUint::from(10000u32).pow(tick.unsigned_abs());
    let (n, d) = if tick >= 0 { (n, d) } else { (d, n) };
    let numerator = (b96() * b96()) * n;
    if numerator.bits() > 1048576 || d.bits() > 1048576 {
        return Err("CLMM tick arithmetic bound".into());
    }
    let value = Amount::from_big((numerator / d).sqrt())?;
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(tick, value);
    Ok(value)
}
pub fn tick_at(sqrt: Amount) -> Result<i32, String> {
    if sqrt < tick_sqrt(MIN_TICK)? || sqrt > tick_sqrt(MAX_TICK)? {
        return Err("CLMM sqrt-price bounds".into());
    }
    let (mut lo, mut hi) = (MIN_TICK, MAX_TICK);
    while lo < hi {
        let middle = lo + (hi - lo + 1) / 2;
        if tick_sqrt(middle)? <= sqrt {
            lo = middle;
        } else {
            hi = middle - 1;
        }
    }
    Ok(lo)
}
pub fn delta_x(l: Amount, a: Amount, b: Amount, up: bool) -> Result<Amount, String> {
    if a.is_zero() || a > b {
        return Err("CLMM delta bounds".into());
    }
    quotient(
        l.big() * b96() * b.checked_sub(a)?.big(),
        a.big() * b.big(),
        up,
    )
}
pub fn delta_q(l: Amount, a: Amount, b: Amount, up: bool) -> Result<Amount, String> {
    if a.is_zero() || a > b {
        return Err("CLMM delta bounds".into());
    }
    quotient(l.big() * b.checked_sub(a)?.big(), b96(), up)
}
pub fn next_sqrt(s: Amount, l: Amount, net: Amount, x_in: bool) -> Result<Amount, String> {
    if l.is_zero() || s.is_zero() {
        return Err("CLMM no active liquidity".into());
    }
    if x_in {
        quotient(
            l.big() * b96() * s.big(),
            l.big() * b96() + net.big() * s.big(),
            true,
        )
    } else {
        s.checked_add(quotient(net.big() * b96(), l.big(), false)?)
    }
}
pub fn growth(fee: Amount, l: Amount) -> Result<Amount, String> {
    quotient(fee.big() * b128(), l.big(), false)
}
pub fn earned(l: Amount, delta: Amount) -> Result<Amount, String> {
    quotient(l.big() * delta.big(), b128(), false)
}
pub fn principal(
    l: Amount,
    s: Amount,
    lower: i32,
    upper: i32,
    up: bool,
) -> Result<[Amount; 2], String> {
    let (a, b) = (tick_sqrt(lower)?, tick_sqrt(upper)?);
    let p = s.max(a).min(b);
    Ok([delta_x(l, p, b, up)?, delta_q(l, a, p, up)?])
}
