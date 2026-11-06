use super::*;
use crate::clmm::{self, Position, SwapStep, math};
use std::ops::Bound::{Excluded, Included, Unbounded};

impl Context<'_> {
    pub(super) fn clmm_call(&mut self, w: &mut World, c: &Call) -> Result<(), ExecutionError> {
        let pool = match c {
            Call::MintPosition { pool, .. }
            | Call::BurnPosition { pool, .. }
            | Call::CollectPosition { pool, .. }
            | Call::ClmmSwap { pool, .. } => pool,
            _ => return Err(Model("invalid CLMM dispatch".into())),
        };
        let cfg = self
            .s
            .clmm_pools
            .get(pool)
            .ok_or("unknown CLMM pool")?
            .clone();
        let ti = self.trace.len() - 1;
        self.trace[ti].clmm = Some(clmm::Trace::new(pool, c.label(), &w.clmm[pool]));
        let result = self.clmm_body(w, c, pool, &cfg, ti);
        let state = &w.clmm[pool];
        let trace = self.trace[ti].clmm.as_mut().unwrap();
        trace.sqrt_after = state.sqrt_price;
        trace.tick_after = state.current_tick;
        trace.liquidity_after = state.liquidity;
        if let Some(id) = trace.position_id
            && let Some(p) = state.positions.get(&id)
        {
            trace.position_owed = p.owed;
        }
        result
    }
    fn clmm_payout(
        &mut self,
        w: &mut World,
        c: &clmm::Config,
        to: &str,
        amounts: [Amount; 2],
    ) -> Result<(), ExecutionError> {
        for (asset, n) in [(&c.x, amounts[0]), (&c.q, amounts[1])] {
            if w.balance(&c.account, asset) < n {
                return Err(Model("CLMM claim exceeds reserve".into()));
            }
            if !n.is_zero() {
                self.transfer(w, &c.account, to, asset, n)?;
            }
        }
        Ok(())
    }
    fn clmm_body(
        &mut self,
        w: &mut World,
        call: &Call,
        pool: &str,
        c: &clmm::Config,
        ti: usize,
    ) -> Result<(), ExecutionError> {
        let sender = self.tx.sender.clone();
        match call {
            Call::MintPosition {
                lower,
                upper,
                liquidity,
                max_x,
                max_q,
                ..
            } => {
                if *lower < math::MIN_TICK
                    || *upper > math::MAX_TICK
                    || lower >= upper
                    || liquidity.is_zero()
                {
                    return Err(Protocol("CLMM range/liquidity".into()));
                }
                let state = &w.clmm[pool];
                let added = [lower, upper]
                    .iter()
                    .filter(|t| !state.ticks.contains_key(t))
                    .count();
                if state.positions.len() >= clmm::MAX_POSITIONS
                    || state.ticks.len() + added > clmm::MAX_TICKS
                {
                    return Err(Protocol("CLMM capacity".into()));
                }
                let total = state.total_liquidity.checked_add(*liquidity)?;
                let id = state.next_id;
                let next = id.checked_add(1.into())?;
                let amounts = math::principal(*liquidity, state.sqrt_price, *lower, *upper, true)?;
                if amounts[0] > *max_x || amounts[1] > *max_q {
                    return Err(Protocol("CLMM max deposit".into()));
                }
                let trace = self.trace[ti].clmm.as_mut().unwrap();
                trace.position_id = Some(id);
                trace.amounts = amounts;
                for (asset, n) in [(&c.x, amounts[0]), (&c.q, amounts[1])] {
                    if !n.is_zero() {
                        self.transfer(w, &sender, &c.account, asset, n)?;
                    }
                }
                let state = Arc::make_mut(&mut w.clmm).get_mut(pool).unwrap();
                state
                    .update_tick(*lower, *liquidity, true, false)
                    .map_err(Model)?;
                state
                    .update_tick(*upper, *liquidity, true, true)
                    .map_err(Model)?;
                let last_inside = state.inside(*lower, *upper).map_err(Model)?;
                state.positions.insert(
                    id,
                    Position {
                        owner: sender.clone(),
                        lower: *lower,
                        upper: *upper,
                        liquidity: *liquidity,
                        last_inside,
                        owed: [Amount::ZERO; 2],
                    },
                );
                state.next_id = next;
                state.total_liquidity = total;
                if *lower <= state.current_tick && state.current_tick < *upper {
                    state.liquidity = state.liquidity.checked_add(*liquidity).map_err(Model)?;
                }
                self.events.push(Event {
                    kind: "MintClmmLiquidity".into(),
                    from: sender,
                    to: pool.into(),
                    asset: format!("L:{pool}"),
                    amount: *liquidity,
                });
                Ok(())
            }
            Call::BurnPosition {
                position_id,
                liquidity,
                min_x,
                min_q,
                ..
            } => {
                let state = &w.clmm[pool];
                let mut position = state
                    .positions
                    .get(position_id)
                    .ok_or("unknown CLMM position")?
                    .clone();
                if position.owner != sender {
                    return Err(Protocol("CLMM position owner required".into()));
                }
                if liquidity.is_zero() || *liquidity > position.liquidity {
                    return Err(Protocol("CLMM burn liquidity".into()));
                }
                state.accrue(&mut position).map_err(Model)?;
                let amounts = math::principal(
                    *liquidity,
                    state.sqrt_price,
                    position.lower,
                    position.upper,
                    false,
                )?;
                if amounts[0] < *min_x || amounts[1] < *min_q {
                    return Err(Protocol("CLMM min withdrawal".into()));
                }
                let trace = self.trace[ti].clmm.as_mut().unwrap();
                trace.position_id = Some(*position_id);
                trace.amounts = amounts;
                trace.position_owed = position.owed;
                let state = Arc::make_mut(&mut w.clmm).get_mut(pool).unwrap();
                state.total_liquidity = state
                    .total_liquidity
                    .checked_sub(*liquidity)
                    .map_err(Model)?;
                if position.lower <= state.current_tick && state.current_tick < position.upper {
                    state.liquidity = state.liquidity.checked_sub(*liquidity).map_err(Model)?;
                }
                state
                    .update_tick(position.lower, *liquidity, false, false)
                    .map_err(Model)?;
                state
                    .update_tick(position.upper, *liquidity, false, true)
                    .map_err(Model)?;
                position.liquidity = position.liquidity.checked_sub(*liquidity).map_err(Model)?;
                if position.liquidity.is_zero() && position.owed.iter().all(|n| n.is_zero()) {
                    state.positions.remove(position_id);
                } else {
                    state.positions.insert(*position_id, position);
                }
                self.clmm_payout(w, c, &sender, amounts)?;
                self.events.push(Event {
                    kind: "BurnClmmLiquidity".into(),
                    from: sender,
                    to: pool.into(),
                    asset: format!("L:{pool}"),
                    amount: *liquidity,
                });
                Ok(())
            }
            Call::CollectPosition { position_id, .. } => {
                let state = &w.clmm[pool];
                let mut position = state
                    .positions
                    .get(position_id)
                    .ok_or("unknown CLMM position")?
                    .clone();
                if position.owner != sender {
                    return Err(Protocol("CLMM position owner required".into()));
                }
                state.accrue(&mut position).map_err(Model)?;
                let amounts = position.owed;
                position.owed = [Amount::ZERO; 2];
                let trace = self.trace[ti].clmm.as_mut().unwrap();
                trace.position_id = Some(*position_id);
                trace.amounts = amounts;
                let state = Arc::make_mut(&mut w.clmm).get_mut(pool).unwrap();
                if position.liquidity.is_zero() {
                    state.positions.remove(position_id);
                } else {
                    state.positions.insert(*position_id, position);
                }
                self.clmm_payout(w, c, &sender, amounts)
            }
            Call::ClmmSwap {
                asset_in,
                amount,
                sqrt_price_limit,
                min_out,
                allow_partial,
                ..
            } => {
                if asset_in != &c.x && asset_in != &c.q {
                    return Err(Protocol("CLMM input asset".into()));
                }
                let x_in = asset_in == &c.x;
                let s = w.clmm[pool].sqrt_price;
                math::tick_at(*sqrt_price_limit)?;
                if amount.is_zero()
                    || (x_in && *sqrt_price_limit >= s)
                    || (!x_in && *sqrt_price_limit <= s)
                {
                    return Err(Protocol("CLMM input/price limit".into()));
                }
                let trace = self.trace[ti].clmm.as_mut().unwrap();
                trace.asset_in = Some(asset_in.clone());
                trace.requested = *amount;
                trace.remainder = *amount;
                self.transfer(w, &sender, &c.account, asset_in, *amount)?;
                let (mut remaining, mut output) = (*amount, Amount::ZERO);
                while !remaining.is_zero() && w.clmm[pool].sqrt_price != *sqrt_price_limit {
                    let state = &w.clmm[pool];
                    let next = if x_in {
                        state
                            .ticks
                            .range((Unbounded, Included(state.current_tick)))
                            .next_back()
                    } else {
                        state
                            .ticks
                            .range((Excluded(state.current_tick), Unbounded))
                            .next()
                    }
                    .map(|(t, _)| *t);
                    let target = if let Some(t) = next {
                        let price = math::tick_sqrt(t)?;
                        if x_in {
                            price.max(*sqrt_price_limit)
                        } else {
                            price.min(*sqrt_price_limit)
                        }
                    } else {
                        *sqrt_price_limit
                    };
                    let crossed = next.filter(|t| math::tick_sqrt(*t).is_ok_and(|p| p == target));
                    if state.liquidity.is_zero() && crossed.is_none() {
                        break;
                    }
                    if self.trace[ti].clmm.as_ref().unwrap().steps.len() > clmm::MAX_TICKS {
                        return Err(Protocol("CLMM step limit".into()));
                    }
                    let charged = self.charge(1);
                    self.charges.last_mut().unwrap().phase = "ClmmStep".into();
                    charged?;
                    let (before, tick_before, l) =
                        (state.sqrt_price, state.current_tick, state.liquidity);
                    let (after, net, gross, fee, out) = if l.is_zero() {
                        (
                            target,
                            Amount::ZERO,
                            Amount::ZERO,
                            Amount::ZERO,
                            Amount::ZERO,
                        )
                    } else {
                        let needed = if x_in {
                            math::delta_x(l, target, before, true)?
                        } else {
                            math::delta_q(l, before, target, true)?
                        };
                        let available = remaining.mul_div(
                            (10000 - c.fee_bps as u64).into(),
                            10000.into(),
                            false,
                        )?;
                        let (after, net, gross) = if available >= needed {
                            (
                                target,
                                needed,
                                needed.mul_div(
                                    10000.into(),
                                    (10000 - c.fee_bps as u64).into(),
                                    true,
                                )?,
                            )
                        } else {
                            let after = math::next_sqrt(before, l, available, x_in)?;
                            if after == before {
                                return Err(Protocol("NoExecutableOutput".into()));
                            }
                            let net = if x_in {
                                math::delta_x(l, after, before, true)?
                            } else {
                                math::delta_q(l, before, after, true)?
                            };
                            if net > available {
                                return Err(Model("CLMM net input invariant".into()));
                            }
                            (after, net, remaining)
                        };
                        let fee = gross.checked_sub(net).map_err(Model)?;
                        let out = if x_in {
                            math::delta_q(l, after, before, false)?
                        } else {
                            math::delta_x(l, before, after, false)?
                        };
                        (after, net, gross, fee, out)
                    };
                    if (x_in && (after > before || after < target))
                        || (!x_in && (after < before || after > target))
                    {
                        return Err(Model("CLMM price progress invariant".into()));
                    }
                    let actually_crossed = crossed.filter(|_| after == target);
                    if gross.is_zero() && actually_crossed.is_none() {
                        return Err(Model("CLMM step made no progress".into()));
                    }
                    let state = Arc::make_mut(&mut w.clmm).get_mut(pool).unwrap();
                    if !fee.is_zero() {
                        let i = if x_in { 0 } else { 1 };
                        state.fee_growth[i] =
                            state.fee_growth[i].checked_add(math::growth(fee, l)?)?;
                    }
                    state.sqrt_price = after;
                    if let Some(t) = actually_crossed {
                        state.cross(t, !x_in).map_err(Model)?;
                    } else {
                        state.current_tick = math::tick_at(after)?;
                    }
                    remaining = remaining.checked_sub(gross).map_err(Model)?;
                    output = output.checked_add(out)?;
                    let trace = self.trace[ti].clmm.as_mut().unwrap();
                    trace.spent = amount.checked_sub(remaining).map_err(Model)?;
                    trace.remainder = remaining;
                    trace.output = output;
                    trace.fee = trace.fee.checked_add(fee)?;
                    trace.steps.push(SwapStep {
                        sqrt_before: before,
                        sqrt_after: after,
                        tick_before,
                        tick_after: state.current_tick,
                        liquidity_before: l,
                        liquidity_after: state.liquidity,
                        gross,
                        net,
                        fee,
                        output: out,
                        crossed_tick: actually_crossed,
                    });
                }
                if output.is_zero() {
                    return Err(Protocol("NoExecutableOutput".into()));
                }
                if !remaining.is_zero() && !allow_partial {
                    return Err(Protocol("CLMM partial fill disabled".into()));
                }
                if output < *min_out {
                    return Err(Protocol("CLMM min_out".into()));
                }
                let out_asset = if x_in { &c.q } else { &c.x };
                if w.balance(&c.account, out_asset) < output {
                    return Err(Model("CLMM output exceeds reserve".into()));
                }
                self.transfer(w, &c.account, &sender, out_asset, output)?;
                if !remaining.is_zero() {
                    self.transfer(w, &c.account, &sender, asset_in, remaining)?;
                }
                Ok(())
            }
            _ => Err(Model("invalid CLMM body".into())),
        }
    }
}
