pub mod math;
use crate::{Amount, Scenario, World};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MODEL: &str = "clmm-native-v1";
pub const MAX_POSITIONS: usize = 1024;
pub const MAX_TICKS: usize = 2048;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub account: String,
    pub x: String,
    pub q: String,
    pub fee_bps: u16,
    pub initial_sqrt: Amount,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SignedLiquidity {
    pub negative: bool,
    pub magnitude: Amount,
}
impl SignedLiquidity {
    pub fn add(self, n: Amount, negative: bool) -> Result<Self, String> {
        let result = if self.negative == negative {
            Self {
                negative,
                magnitude: self.magnitude.checked_add(n)?,
            }
        } else if self.magnitude >= n {
            Self {
                negative: self.negative,
                magnitude: self.magnitude.checked_sub(n)?,
            }
        } else {
            Self {
                negative,
                magnitude: n.checked_sub(self.magnitude)?,
            }
        };
        Ok(if result.magnitude.is_zero() {
            Self::default()
        } else {
            result
        })
    }
    pub fn apply(self, l: Amount, up: bool) -> Result<Amount, String> {
        if self.negative == up {
            l.checked_sub(self.magnitude)
        } else {
            l.checked_add(self.magnitude)
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Tick {
    pub gross: Amount,
    pub net: SignedLiquidity,
    pub outside: [Amount; 2],
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub owner: String,
    pub lower: i32,
    pub upper: i32,
    pub liquidity: Amount,
    pub last_inside: [Amount; 2],
    pub owed: [Amount; 2],
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub sqrt_price: Amount,
    pub current_tick: i32,
    pub liquidity: Amount,
    pub total_liquidity: Amount,
    pub fee_growth: [Amount; 2],
    pub next_id: Amount,
    pub ticks: BTreeMap<i32, Tick>,
    pub positions: BTreeMap<Amount, Position>,
}
impl State {
    pub fn new(c: &Config) -> Result<Self, String> {
        Ok(Self {
            sqrt_price: c.initial_sqrt,
            current_tick: math::tick_at(c.initial_sqrt)?,
            liquidity: Amount::ZERO,
            total_liquidity: Amount::ZERO,
            fee_growth: [Amount::ZERO; 2],
            next_id: 1.into(),
            ticks: BTreeMap::new(),
            positions: BTreeMap::new(),
        })
    }
    pub fn inside(&self, lower: i32, upper: i32) -> Result<[Amount; 2], String> {
        let lo = self.ticks.get(&lower).ok_or("missing CLMM lower tick")?;
        let hi = self.ticks.get(&upper).ok_or("missing CLMM upper tick")?;
        let mut result = [Amount::ZERO; 2];
        for (i, value) in result.iter_mut().enumerate() {
            let below = if self.current_tick >= lower {
                lo.outside[i]
            } else {
                self.fee_growth[i].checked_sub(lo.outside[i])?
            };
            let above = if self.current_tick < upper {
                hi.outside[i]
            } else {
                self.fee_growth[i].checked_sub(hi.outside[i])?
            };
            *value = self.fee_growth[i].checked_sub(below)?.checked_sub(above)?;
        }
        Ok(result)
    }
    pub fn accrue(&self, p: &mut Position) -> Result<(), String> {
        if p.liquidity.is_zero() {
            return Ok(());
        }
        let inside = self.inside(p.lower, p.upper)?;
        for (i, value) in inside.iter().enumerate() {
            p.owed[i] = p.owed[i].checked_add(math::earned(
                p.liquidity,
                value.checked_sub(p.last_inside[i])?,
            )?)?;
        }
        p.last_inside = inside;
        Ok(())
    }
    pub(crate) fn update_tick(
        &mut self,
        t: i32,
        l: Amount,
        add: bool,
        upper: bool,
    ) -> Result<(), String> {
        let outside = if t <= self.current_tick {
            self.fee_growth
        } else {
            [Amount::ZERO; 2]
        };
        let tick = self.ticks.entry(t).or_insert(Tick {
            gross: Amount::ZERO,
            net: SignedLiquidity::default(),
            outside,
        });
        tick.gross = if add {
            tick.gross.checked_add(l)?
        } else {
            tick.gross.checked_sub(l)?
        };
        tick.net = tick.net.add(l, upper ^ !add)?;
        if tick.gross.is_zero() {
            if !tick.net.magnitude.is_zero() {
                return Err("CLMM empty tick net".into());
            }
            self.ticks.remove(&t);
        }
        Ok(())
    }
    pub(crate) fn cross(&mut self, t: i32, up: bool) -> Result<(), String> {
        let tick = self.ticks.get_mut(&t).ok_or("missing crossed tick")?;
        for (i, outside) in tick.outside.iter_mut().enumerate() {
            *outside = self.fee_growth[i].checked_sub(*outside)?;
        }
        self.liquidity = tick.net.apply(self.liquidity, up)?;
        self.current_tick = if up { t } else { t - 1 };
        Ok(())
    }
    pub fn claims(&self) -> Result<[Amount; 2], String> {
        let mut claims = [Amount::ZERO; 2];
        for p in self.positions.values() {
            let principal = math::principal(p.liquidity, self.sqrt_price, p.lower, p.upper, false)?;
            let mut p = p.clone();
            self.accrue(&mut p)?;
            for i in 0..2 {
                claims[i] = claims[i]
                    .checked_add(principal[i])?
                    .checked_add(p.owed[i])?;
            }
        }
        Ok(claims)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SwapStep {
    pub sqrt_before: Amount,
    pub sqrt_after: Amount,
    pub tick_before: i32,
    pub tick_after: i32,
    pub liquidity_before: Amount,
    pub liquidity_after: Amount,
    pub gross: Amount,
    pub net: Amount,
    pub fee: Amount,
    pub output: Amount,
    pub crossed_tick: Option<i32>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    pub pool: String,
    pub action: String,
    pub position_id: Option<Amount>,
    pub sqrt_before: Amount,
    pub sqrt_after: Amount,
    pub tick_before: i32,
    pub tick_after: i32,
    pub liquidity_before: Amount,
    pub liquidity_after: Amount,
    pub amounts: [Amount; 2],
    pub position_owed: [Amount; 2],
    pub asset_in: Option<String>,
    pub requested: Amount,
    pub spent: Amount,
    pub output: Amount,
    pub remainder: Amount,
    pub fee: Amount,
    pub steps: Vec<SwapStep>,
}
impl Trace {
    pub fn new(pool: &str, action: &str, s: &State) -> Self {
        Self {
            pool: pool.into(),
            action: action.into(),
            position_id: None,
            sqrt_before: s.sqrt_price,
            sqrt_after: s.sqrt_price,
            tick_before: s.current_tick,
            tick_after: s.current_tick,
            liquidity_before: s.liquidity,
            liquidity_after: s.liquidity,
            amounts: [Amount::ZERO; 2],
            position_owed: [Amount::ZERO; 2],
            asset_in: None,
            requested: Amount::ZERO,
            spent: Amount::ZERO,
            output: Amount::ZERO,
            remainder: Amount::ZERO,
            fee: Amount::ZERO,
            steps: vec![],
        }
    }
}

pub fn validate<'a>(s: &'a Scenario, protected: &mut BTreeSet<&'a String>) -> Result<(), String> {
    if s.clmm_pools.len() > 16
        || (!s.clmm_pools.is_empty() && !s.model_versions.iter().any(|m| m == MODEL))
    {
        return Err("CLMM registry/model limit".into());
    }
    for (id, c) in &s.clmm_pools {
        if id.is_empty()
            || s.pool(id).is_some()
            || s.books.contains_key(id)
            || s.lending.contains_key(id)
            || s.flash_providers.contains_key(id)
            || c.x == c.q
            || !s.assets.contains_key(&c.x)
            || !s.assets.contains_key(&c.q)
            || c.fee_bps >= 10000
            || !s.accounts.contains_key(&c.account)
            || !protected.insert(&c.account)
        {
            return Err("invalid CLMM config".into());
        }
        math::tick_at(c.initial_sqrt)?;
        if s.accounts[&c.account]
            .balances
            .values()
            .any(|n| !n.is_zero())
        {
            return Err("CLMM genesis account must be empty".into());
        }
    }
    Ok(())
}
pub fn invariant(s: &Scenario, w: &World) -> Result<(), String> {
    if s.clmm_pools.keys().ne(w.clmm.keys()) {
        return Err("CLMM registry invariant".into());
    }
    for (id, c) in &s.clmm_pools {
        let state = &w.clmm[id];
        if state.positions.len() > MAX_POSITIONS
            || state.ticks.len() > MAX_TICKS
            || state.next_id.is_zero()
        {
            return Err("CLMM capacity/ID invariant".into());
        }
        let at = math::tick_at(state.sqrt_price)?;
        if state.current_tick != at
            && !(state.current_tick == at - 1 && state.sqrt_price == math::tick_sqrt(at)?)
        {
            return Err("CLMM current tick invariant".into());
        }
        let (mut total, mut active) = (Amount::ZERO, Amount::ZERO);
        let mut expected: BTreeMap<i32, (Amount, SignedLiquidity)> = BTreeMap::new();
        for (id, p) in &state.positions {
            if id.is_zero()
                || *id >= state.next_id
                || !s.accounts.contains_key(&p.owner)
                || s.protected(&p.owner)
                || p.lower < math::MIN_TICK
                || p.upper > math::MAX_TICK
                || p.lower >= p.upper
                || (p.liquidity.is_zero() && p.owed.iter().all(|n| n.is_zero()))
            {
                return Err("CLMM position invariant".into());
            }
            total = total.checked_add(p.liquidity)?;
            if p.lower <= state.current_tick && state.current_tick < p.upper {
                active = active.checked_add(p.liquidity)?;
            }
            if !p.liquidity.is_zero() {
                for (tick, negative) in [(p.lower, false), (p.upper, true)] {
                    let value = expected.entry(tick).or_default();
                    value.0 = value.0.checked_add(p.liquidity)?;
                    value.1 = value.1.add(p.liquidity, negative)?;
                }
            }
        }
        if total != state.total_liquidity
            || active != state.liquidity
            || expected.keys().ne(state.ticks.keys())
        {
            return Err("CLMM liquidity invariant".into());
        }
        for (tick, (gross, net)) in expected {
            let actual = &state.ticks[&tick];
            if actual.gross != gross
                || actual.net != net
                || actual
                    .outside
                    .iter()
                    .zip(state.fee_growth)
                    .any(|(a, g)| *a > g)
            {
                return Err("CLMM tick accounting invariant".into());
            }
        }
        let claims = state.claims()?;
        for (asset, n) in [(&c.x, claims[0]), (&c.q, claims[1])] {
            w.balance(&c.account, asset)
                .checked_sub(n)
                .map_err(|_| "CLMM backing invariant")?;
        }
        if w.accounts[&c.account]
            .balances
            .iter()
            .any(|(asset, n)| asset != &c.x && asset != &c.q && !n.is_zero())
        {
            return Err("CLMM foreign asset invariant".into());
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PositionView {
    pub id: Amount,
    pub owner: String,
    pub lower: i32,
    pub upper: i32,
    pub liquidity: Amount,
    pub active: bool,
    pub principal: [Amount; 2],
    pub collectable: [Amount; 2],
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct TickView {
    pub tick: i32,
    pub sqrt_price: Amount,
    pub liquidity_above: Amount,
    pub gross: Amount,
    pub net: SignedLiquidity,
    pub outside: [Amount; 2],
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct View {
    pub pool: String,
    pub config: Config,
    pub sqrt_price: Amount,
    pub price: crate::amount::Price,
    pub current_tick: i32,
    pub active_liquidity: Amount,
    pub total_liquidity: Amount,
    pub reserves: [Amount; 2],
    pub claims: [Amount; 2],
    pub rounding_surplus: [Amount; 2],
    pub fee_growth: [Amount; 2],
    pub positions: Vec<PositionView>,
    pub ticks: Vec<TickView>,
}
pub fn views(s: &Scenario, w: &World) -> Result<Vec<View>, String> {
    s.clmm_pools
        .iter()
        .map(|(id, c)| {
            let state = &w.clmm[id];
            let claims = state.claims()?;
            let numerator = state.sqrt_price.big() * state.sqrt_price.big();
            let shift = numerator.trailing_zeros().unwrap_or(0).min(192) as usize;
            let price = crate::amount::Price::new(
                c.x.clone(),
                c.q.clone(),
                Amount::from_big(numerator >> shift)?,
                Amount::from_big((math::b96() * math::b96()) >> shift)?,
            )?;
            let reserves = [w.balance(&c.account, &c.x), w.balance(&c.account, &c.q)];
            let positions = state
                .positions
                .iter()
                .map(|(id, p)| {
                    let mut valued = p.clone();
                    state.accrue(&mut valued)?;
                    Ok(PositionView {
                        id: *id,
                        owner: p.owner.clone(),
                        lower: p.lower,
                        upper: p.upper,
                        liquidity: p.liquidity,
                        active: !p.liquidity.is_zero()
                            && p.lower <= state.current_tick
                            && state.current_tick < p.upper,
                        principal: math::principal(
                            p.liquidity,
                            state.sqrt_price,
                            p.lower,
                            p.upper,
                            false,
                        )?,
                        collectable: valued.owed,
                    })
                })
                .collect::<Result<_, String>>()?;
            let mut active = Amount::ZERO;
            let ticks = state
                .ticks
                .iter()
                .map(|(tick, t)| {
                    active = t.net.apply(active, true)?;
                    Ok(TickView {
                        tick: *tick,
                        sqrt_price: math::tick_sqrt(*tick)?,
                        liquidity_above: active,
                        gross: t.gross,
                        net: t.net,
                        outside: t.outside,
                    })
                })
                .collect::<Result<_, String>>()?;
            Ok(View {
                pool: id.clone(),
                config: c.clone(),
                sqrt_price: state.sqrt_price,
                price,
                current_tick: state.current_tick,
                active_liquidity: state.liquidity,
                total_liquidity: state.total_liquidity,
                reserves,
                claims,
                rounding_surplus: [
                    reserves[0].checked_sub(claims[0])?,
                    reserves[1].checked_sub(claims[1])?,
                ],
                fee_growth: state.fee_growth,
                positions,
                ticks,
            })
        })
        .collect()
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Quote {
    pub state_ref: String,
    pub block: Amount,
    pub receipt: crate::Receipt,
}
pub fn quote(s: &Scenario, w: &World, tx: &crate::Transaction) -> Result<Quote, String> {
    if tx.program.len() != 1 || !matches!(tx.program[0], crate::Call::ClmmSwap { .. }) {
        return Err("CLMM quote requires one ClmmSwap".into());
    }
    let receipt = crate::engine::simulate_transaction(s, w, tx)?;
    Ok(Quote {
        state_ref: crate::run::digest(&serde_json::to_vec(&(s, w)).map_err(|e| e.to_string())?),
        block: w.block,
        receipt,
    })
}
