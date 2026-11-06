use super::*;
use crate::credit::{self, Loss};
impl Context<'_> {
    fn credit_price(
        &mut self,
        cfg: &credit::LendingConfig,
        w: &World,
    ) -> Result<crate::amount::Price, ExecutionError> {
        self.trace
            .last_mut()
            .ok_or_else(|| Model("missing consumer trace".into()))?
            .oracle = Some(credit::OracleUse::capture(self.s, cfg, w));
        Ok(credit::fresh_price(cfg, w)?.clone())
    }
    pub(super) fn credit_call(&mut self, w: &mut World, c: &Call) -> Result<(), ExecutionError> {
        let sender = self.tx.sender.clone();
        let market = match c {
            Call::Supply { market, .. }
            | Call::Withdraw { market, .. }
            | Call::DepositCollateral { market, .. }
            | Call::WithdrawCollateral { market, .. }
            | Call::SetCollateralUsage { market, .. }
            | Call::Borrow { market, .. }
            | Call::Repay { market, .. }
            | Call::Liquidate { market, .. } => market,
            _ => return Err(Model("not a credit call".into())),
        };
        let cfg = self
            .s
            .lending
            .get(market)
            .ok_or("unknown lending market")?
            .clone();
        let mut l = w
            .lending
            .get(market)
            .ok_or_else(|| Model("missing lending state".into()))?
            .clone();
        let nonzero = |a: Amount| -> Result<(), ExecutionError> {
            if a.is_zero() {
                Err(Protocol("zero amount".into()))
            } else {
                Ok(())
            }
        };
        match c {
            Call::Supply {
                amount, min_shares, ..
            } => {
                nonzero(*amount)?;
                if l.bankrupt() {
                    return Err(Protocol("Bankrupt: ordinary supply disabled".into()));
                }
                let m = if l.total_shares.is_zero() {
                    *amount
                } else {
                    amount.mul_div(l.total_shares, l.claims, false)?
                };
                if m.is_zero() || m < *min_shares {
                    return Err(Protocol("min_shares or zero mint".into()));
                }
                let claims = l.claims.checked_add(*amount)?;
                if claims > cfg.supply_cap {
                    return Err(Protocol("supply cap".into()));
                }
                l.total_shares = l.total_shares.checked_add(m)?;
                l.claims = claims;
                let held = l.deposit_shares.get(&sender).copied().unwrap_or_default();
                l.deposit_shares
                    .insert(sender.clone(), held.checked_add(m)?);
                self.transfer(w, &sender, &cfg.cash_account, &cfg.debt_asset, *amount)?;
                self.events.push(Event {
                    kind: "MintDepositShares".into(),
                    from: market.clone(),
                    to: sender,
                    asset: format!("deposit:{market}"),
                    amount: m,
                });
            }
            Call::Withdraw {
                shares, min_assets, ..
            } => {
                nonzero(*shares)?;
                if l.bankrupt() {
                    return Err(Protocol("Bankrupt: no monetary claim".into()));
                }
                let held = l.deposit_shares.get(&sender).copied().unwrap_or_default();
                if *shares > held || l.total_shares.is_zero() {
                    return Err(Protocol("insufficient deposit shares".into()));
                }
                let amount = shares.mul_div(l.claims, l.total_shares, false)?;
                if amount.is_zero() {
                    return Err(Protocol("zero withdrawal assets".into()));
                }
                if amount < *min_assets {
                    return Err(Protocol("min_assets".into()));
                }
                if w.balance(&cfg.cash_account, &cfg.debt_asset) < amount {
                    return Err(Protocol("insufficient reserve cash".into()));
                }
                l.claims = l.claims.checked_sub(amount)?;
                l.total_shares = l.total_shares.checked_sub(*shares)?;
                l.deposit_shares
                    .insert(sender.clone(), held.checked_sub(*shares)?);
                self.transfer(w, &cfg.cash_account, &sender, &cfg.debt_asset, amount)?;
                self.events.push(Event {
                    kind: "BurnDepositShares".into(),
                    from: sender,
                    to: market.clone(),
                    asset: format!("deposit:{market}"),
                    amount: *shares,
                });
            }
            Call::DepositCollateral { amount, .. } => {
                nonzero(*amount)?;
                let escrow = cfg
                    .positions
                    .get(&sender)
                    .ok_or("sender has no collateral position")?;
                self.transfer(w, &sender, escrow, &cfg.collateral_asset, *amount)?;
            }
            Call::WithdrawCollateral { amount, .. } => {
                nonzero(*amount)?;
                let escrow = cfg
                    .positions
                    .get(&sender)
                    .ok_or("sender has no collateral position")?;
                let remaining = w
                    .balance(escrow, &cfg.collateral_asset)
                    .checked_sub(*amount)?;
                let p = &l.positions[&sender];
                if !p.debt.is_zero() {
                    let price = self.credit_price(&cfg, w)?;
                    if !p.collateral_enabled
                        || credit::risk_value(remaining, &price, cfg.lt_bps) < p.debt.big()
                    {
                        return Err(Protocol("collateral withdrawal violates HF".into()));
                    }
                }
                self.transfer(w, escrow, &sender, &cfg.collateral_asset, *amount)?;
            }
            Call::SetCollateralUsage { enabled, .. } => {
                let p = l
                    .positions
                    .get_mut(&sender)
                    .ok_or("sender has no collateral position")?;
                if !p.debt.is_zero() {
                    self.credit_price(&cfg, w)?;
                    if !*enabled {
                        return Err(Protocol("collateral disable violates HF".into()));
                    }
                }
                p.collateral_enabled = *enabled;
                self.events.push(Event {
                    kind: "CollateralUsage".into(),
                    from: sender,
                    to: market.clone(),
                    asset: "enabled".into(),
                    amount: Amount::from(u64::from(*enabled)),
                });
            }
            Call::Borrow { amount, .. } => {
                nonzero(*amount)?;
                let escrow = cfg
                    .positions
                    .get(&sender)
                    .ok_or("sender has no collateral position")?;
                let p = l
                    .positions
                    .get_mut(&sender)
                    .ok_or_else(|| Model("missing position".into()))?;
                if !p.collateral_enabled {
                    return Err(Protocol("collateral disabled".into()));
                }
                let debt = p.debt.checked_add(*amount)?;
                let total = l.debt.checked_add(*amount)?;
                if total > cfg.borrow_cap {
                    return Err(Protocol("borrow cap".into()));
                }
                let price = self.credit_price(&cfg, w)?;
                if credit::risk_value(
                    w.balance(escrow, &cfg.collateral_asset),
                    &price,
                    cfg.ltv_bps,
                ) < debt.big()
                {
                    return Err(Protocol("borrow violates LTV".into()));
                }
                p.debt = debt;
                l.debt = total;
                self.transfer(w, &cfg.cash_account, &sender, &cfg.debt_asset, *amount)?;
            }
            Call::Repay {
                borrower, amount, ..
            } => {
                nonzero(*amount)?;
                let p = l.positions.get_mut(borrower).ok_or("unknown borrower")?;
                if p.debt.is_zero() {
                    return Err(Protocol("position has no debt".into()));
                }
                let paid = std::cmp::min(*amount, p.debt);
                self.transfer(w, &sender, &cfg.cash_account, &cfg.debt_asset, paid)?;
                p.debt = p.debt.checked_sub(paid)?;
                l.debt = l.debt.checked_sub(paid)?;
                if p.debt.is_zero() && !p.remainder.is_zero() {
                    self.events.push(Event {
                        kind: "InterestRemainderCleared".into(),
                        from: borrower.clone(),
                        to: market.clone(),
                        asset: format!("fractional:{}", cfg.debt_asset),
                        amount: p.remainder,
                    });
                    p.remainder = Amount::ZERO;
                }
            }
            Call::Liquidate {
                borrower,
                amount,
                min_collateral,
                ..
            } => {
                nonzero(*amount)?;
                let escrow = cfg.positions.get(borrower).ok_or("unknown borrower")?;
                let mut p = l
                    .positions
                    .get(borrower)
                    .ok_or_else(|| Model("missing position".into()))?
                    .clone();
                if p.debt.is_zero() {
                    return Err(Protocol("NoDebt".into()));
                }
                let price = self.credit_price(&cfg, w)?;
                let collateral = w.balance(escrow, &cfg.collateral_asset);
                let hf = if p.collateral_enabled {
                    credit::risk_value(collateral, &price, cfg.lt_bps)
                } else {
                    BigUint::from(0u8)
                };
                if hf >= p.debt.big() {
                    return Err(Protocol("position is not liquidatable".into()));
                }
                let close = p
                    .debt
                    .mul_div((cfg.close_bps as u64).into(), 10000.into(), true)?;
                let numerator =
                    collateral.big() * price.numerator().big() * BigUint::from(10000u16);
                let denominator =
                    price.denominator().big() * BigUint::from(10000u64 + cfg.bonus_bps as u64);
                let cap = (numerator + &denominator - BigUint::from(1u8)) / denominator;
                let paid = Amount::from_big(std::cmp::min(
                    cap,
                    std::cmp::min(amount.big(), std::cmp::min(p.debt.big(), close.big())),
                ))?;
                let seize = Amount::from_big(std::cmp::min(
                    collateral.big(),
                    paid.big()
                        * BigUint::from(10000u64 + cfg.bonus_bps as u64)
                        * price.denominator().big()
                        / (price.numerator().big() * BigUint::from(10000u16)),
                ))?;
                if paid.is_zero() || seize.is_zero() || seize < *min_collateral {
                    return Err(Protocol("zero liquidation or min_collateral".into()));
                }
                self.transfer(w, &sender, &cfg.cash_account, &cfg.debt_asset, paid)?;
                self.transfer(w, escrow, &sender, &cfg.collateral_asset, seize)?;
                p.debt = p.debt.checked_sub(paid)?;
                l.debt = l.debt.checked_sub(paid)?;
                if seize == collateral && !p.debt.is_zero() {
                    let loss = p.debt;
                    let equity = std::cmp::min(l.equity, loss);
                    let deposit = loss.checked_sub(equity)?;
                    l.debt = l.debt.checked_sub(loss)?;
                    l.equity = l.equity.checked_sub(equity)?;
                    l.claims = l.claims.checked_sub(deposit)?;
                    p.debt = Amount::ZERO;
                    l.losses.push(Loss {
                        block: w.block,
                        borrower: borrower.clone(),
                        amount: loss,
                        equity_loss: equity,
                        depositor_loss: deposit,
                    });
                    self.events.push(Event {
                        kind: "BadDebtWrittenOff".into(),
                        from: borrower.clone(),
                        to: market.clone(),
                        asset: cfg.debt_asset.clone(),
                        amount: loss,
                    });
                }
                if p.debt.is_zero() && !p.remainder.is_zero() {
                    self.events.push(Event {
                        kind: "InterestRemainderCleared".into(),
                        from: borrower.clone(),
                        to: market.clone(),
                        asset: format!("fractional:{}", cfg.debt_asset),
                        amount: p.remainder,
                    });
                    p.remainder = Amount::ZERO;
                }
                l.positions.insert(borrower.clone(), p);
            }
            _ => return Err(Model("unsupported credit operation".into())),
        }
        Arc::make_mut(&mut w.lending).insert(market.clone(), l);
        credit::invariant(self.s, w).map_err(Model)?;
        Ok(())
    }
}
