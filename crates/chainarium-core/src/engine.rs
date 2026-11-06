mod book_calls;
mod clmm_calls;
mod credit_calls;
mod liquidation_calls;
mod oracle_calls;
mod route_calls;
use crate::{Amount, model::*};
use num_bigint::BigUint;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Debug, PartialEq, Eq)]
pub enum ExecutionError {
    Protocol(String),
    Exhausted,
    Model(String),
}
use ExecutionError::*;
impl From<&str> for ExecutionError {
    fn from(s: &str) -> Self {
        Protocol(s.into())
    }
}
impl From<String> for ExecutionError {
    fn from(s: String) -> Self {
        Protocol(s)
    }
}

impl World {
    pub fn genesis(s: &Scenario) -> Result<Self, String> {
        s.validate()?;
        let mut shares = BTreeMap::new();
        for (id, p) in s.pools() {
            let x = s.accounts[&p.account]
                .balances
                .get(&p.x)
                .copied()
                .unwrap_or_default();
            let y = s.accounts[&p.account]
                .balances
                .get(&p.y)
                .copied()
                .unwrap_or_default();
            let total = if let Some(cfg) = s.stable_pools.get(id) {
                let params = cfg.parameters(s)?;
                crate::stable::invariant(
                    params.normalized([x, y])?,
                    &params,
                    &mut || Ok::<(), String>(()),
                    &mut vec![],
                )?
            } else {
                Amount::from_big((x.big() * y.big()).sqrt())?
            };
            shares.insert(id.clone(), total);
        }
        let mut w = Self {
            accounts: Arc::new(s.accounts.clone()),
            allowances: Arc::new(BTreeMap::new()),
            lp_balances: Arc::new(
                shares
                    .iter()
                    .map(|(id, amount)| {
                        (
                            id.clone(),
                            BTreeMap::from([(s.pool(id).unwrap().owner.clone(), *amount)]),
                        )
                    })
                    .collect(),
            ),
            shares,
            block: Amount::ZERO,
            timestamp: Amount::ZERO,
            lending: Arc::new(
                s.lending
                    .iter()
                    .map(|(id, cfg)| (id.clone(), crate::credit::LendingState::new(cfg)))
                    .collect(),
            ),
            feeds: Arc::new(
                s.feeds
                    .iter()
                    .map(|(id, cfg)| {
                        (
                            id.clone(),
                            crate::credit::FeedState {
                                record: cfg.initial.clone(),
                                seen_updates: cfg
                                    .initial
                                    .as_ref()
                                    .map(|r| BTreeSet::from([r.update_id.clone()]))
                                    .unwrap_or_default(),
                            },
                        )
                    })
                    .collect(),
            ),
            price_history: Arc::new(BTreeMap::new()),
            clmm: Arc::new(
                s.clmm_pools
                    .iter()
                    .map(|(id, c)| Ok((id.clone(), crate::clmm::State::new(c)?)))
                    .collect::<Result<_, String>>()?,
            ),
            books: Arc::new(
                s.books
                    .keys()
                    .map(|id| (id.clone(), crate::book::State::default()))
                    .collect(),
            ),
        };
        w.price_history = Arc::new(crate::twap::genesis(s, &w)?);
        Ok(w)
    }
    pub fn balance(&self, account: &str, asset: &str) -> Amount {
        self.accounts
            .get(account)
            .and_then(|a| a.balances.get(asset))
            .copied()
            .unwrap_or_default()
    }
    fn debit(&mut self, account: &str, asset: &str, n: Amount) -> Result<(), String> {
        let a = Arc::make_mut(&mut self.accounts)
            .get_mut(account)
            .ok_or("unknown account")?;
        let b = a
            .balances
            .get(asset)
            .copied()
            .unwrap_or_default()
            .checked_sub(n)?;
        a.balances.insert(asset.into(), b);
        Ok(())
    }
    fn credit(&mut self, account: &str, asset: &str, n: Amount) -> Result<(), String> {
        let a = Arc::make_mut(&mut self.accounts)
            .get_mut(account)
            .ok_or("unknown account")?;
        let b = a
            .balances
            .get(asset)
            .copied()
            .unwrap_or_default()
            .checked_add(n)?;
        a.balances.insert(asset.into(), b);
        Ok(())
    }
    fn transfer(&mut self, from: &str, to: &str, asset: &str, n: Amount) -> Result<(), String> {
        if !self.accounts.contains_key(to) {
            return Err("unknown recipient".into());
        }
        if self.balance(from, asset) < n {
            return Err("insufficient balance".into());
        }
        if from == to {
            return Ok(());
        }
        self.balance(to, asset).checked_add(n)?;
        self.debit(from, asset, n)?;
        self.credit(to, asset, n)
    }
    pub fn invariant(&self, s: &Scenario) -> Result<(), String> {
        if self.accounts.len() != s.accounts.len() || self.accounts.keys().ne(s.accounts.keys()) {
            return Err("account identity invariant".into());
        }
        let total = |accounts: &BTreeMap<String, Account>, asset: &str| -> BigUint {
            accounts
                .values()
                .map(|a| a.balances.get(asset).copied().unwrap_or_default().big())
                .sum()
        };
        for asset in s.assets.keys() {
            if total(&self.accounts, asset) != total(&s.accounts, asset) {
                return Err(format!("supply invariant: {asset}"));
            }
        }
        for a in self.accounts.values() {
            if a.balances.keys().any(|asset| !s.assets.contains_key(asset)) {
                return Err("unknown ledger asset".into());
            }
        }
        if self.shares.len() != s.protocols.len() + s.stable_pools.len() {
            return Err("shares identity invariant".into());
        }
        if self.lp_balances.len() != s.protocols.len() + s.stable_pools.len() {
            return Err("LP identity invariant".into());
        }
        for (id, p) in s.pools() {
            let total = *self.shares.get(id).ok_or("missing LP supply")?;
            let positions = self.lp_balances.get(id).ok_or("missing LP positions")?;
            let sum = positions.iter().try_fold(Amount::ZERO, |sum, (owner, n)| {
                if !s.accounts.contains_key(owner) || s.protected(owner) {
                    return Err("invalid LP owner".to_string());
                }
                sum.checked_add(*n)
            })?;
            if sum != total {
                return Err("LP supply invariant".into());
            }
            let x = self.balance(&p.account, &p.x);
            let y = self.balance(&p.account, &p.y);
            if (total.is_zero() && (!x.is_zero() || !y.is_zero()))
                || (!total.is_zero() && (x.is_zero() || y.is_zero()))
            {
                return Err("AMM reserve/share invariant".into());
            }
            if let Some(cfg) = s.stable_pools.get(id) {
                cfg.parameters(s)?.normalized([x, y])?;
            }
        }
        crate::credit::invariant(s, self)?;
        crate::twap::invariant(s, self)?;
        crate::book::invariant(s, self)?;
        crate::clmm::invariant(s, self)?;
        for (owner, spenders) in self.allowances.iter() {
            if !s.accounts.contains_key(owner) || s.protected(owner) {
                return Err("allowance owner invariant".into());
            }
            for (spender, assets) in spenders {
                if !s.accounts.contains_key(spender)
                    || assets.keys().any(|a| !s.assets.contains_key(a))
                {
                    return Err("allowance reference invariant".into());
                }
            }
        }
        Ok(())
    }
}

pub fn quote(
    s: &Scenario,
    w: &World,
    pool: &str,
    asset: &str,
    amount: Amount,
) -> Result<(String, Amount), String> {
    let p = s.pool(pool).ok_or("unknown pool")?;
    let other = if asset == p.x {
        &p.y
    } else if asset == p.y {
        &p.x
    } else {
        return Err("asset not in pool".into());
    };
    let x = w.balance(&p.account, asset);
    let y = w.balance(&p.account, other);
    let out = if let Some(cfg) = s.stable_pools.get(pool) {
        crate::stable::output(
            [w.balance(&p.account, &p.x), w.balance(&p.account, &p.y)],
            asset == p.x,
            amount,
            &cfg.parameters(s)?,
            &mut || Ok::<(), String>(()),
            &mut vec![],
        )?
    } else {
        cpmm_output(x, y, amount, p.fee_bps)?
    };
    Ok((other.clone(), out))
}
pub fn cpmm_output(x: Amount, y: Amount, amount: Amount, fee_bps: u16) -> Result<Amount, String> {
    if fee_bps >= 10000 || amount.is_zero() || x.is_zero() || y.is_zero() {
        return Err("zero input, no liquidity or invalid fee".into());
    }
    x.checked_add(amount)?;
    // At most 526 bits; all three factors and the fee range are bounded above.
    let effective = amount.big() * BigUint::from(10000 - fee_bps);
    let out =
        Amount::from_big((&effective * y.big()) / (x.big() * BigUint::from(10000u16) + effective))?;
    if out.is_zero() || out >= y {
        return Err("unexecutable output".into());
    }
    Ok(out)
}

struct Context<'a> {
    s: &'a Scenario,
    tx: &'a Transaction,
    entry_fee_balance: Amount,
    used: Amount,
    events: Vec<Event>,
    trace: Vec<Trace>,
    charges: Vec<Charge>,
    outputs: BTreeMap<String, (String, Amount)>,
    route_results: BTreeMap<String, crate::routing::heterogeneous::RouteResult>,
    active_flash: BTreeSet<String>,
    liquidations: BTreeMap<String, crate::liquidation::SaleResult>,
}
impl Context<'_> {
    fn charge(&mut self, n: u64) -> Result<(), ExecutionError> {
        let remaining = self.tx.limit.checked_sub(self.used).map_err(Model)?;
        let requested = Amount::from(n);
        let charged = std::cmp::min(remaining, requested);
        self.used = self.used.checked_add(charged).map_err(Model)?;
        self.charges.push(Charge {
            phase: match n {
                100 => "Transaction",
                10 => "Call",
                5 => "LedgerTransfer",
                _ => "Iteration",
            }
            .into(),
            requested,
            charged,
            total: self.used,
        });
        if charged < requested {
            return Err(Exhausted);
        }
        Ok(())
    }
    fn transfer(
        &mut self,
        w: &mut World,
        from: &str,
        to: &str,
        asset: &str,
        amount: Amount,
    ) -> Result<(), ExecutionError> {
        self.charge(5)?;
        if !self.s.assets.contains_key(asset) {
            return Err(Protocol("unknown asset".into()));
        }
        w.transfer(from, to, asset, amount)?;
        self.events.push(Event {
            kind: "Transfer".into(),
            from: from.into(),
            to: to.into(),
            asset: asset.into(),
            amount,
        });
        Ok(())
    }
    fn call(&mut self, w: &mut World, c: &Call, path: String) -> Result<(), ExecutionError> {
        // A savepoint includes all protocol state, not just the touched ledger entries.
        let saved = w.clone();
        let saved_outputs = self.outputs.clone();
        let saved_routes = self.route_results.clone();
        let saved_liquidations = self.liquidations.clone();
        let event_start = self.events.len();
        let trace_index = self.trace.len();
        let callee = match c {
            Call::MintPosition { pool, .. }
            | Call::BurnPosition { pool, .. }
            | Call::CollectPosition { pool, .. }
            | Call::ClmmSwap { pool, .. } => pool.clone(),
            Call::PlaceOrder { book, .. } | Call::CancelOrder { book, .. } => book.clone(),
            Call::SwapExactIn { pool, .. }
            | Call::SwapOutput { pool, .. }
            | Call::AddLiquidity { pool, .. }
            | Call::RemoveLiquidity { pool, .. } => pool.clone(),
            Call::FlashLoan { provider, .. } => provider.clone(),
            Call::Supply { market, .. }
            | Call::Withdraw { market, .. }
            | Call::DepositCollateral { market, .. }
            | Call::WithdrawCollateral { market, .. }
            | Call::SetCollateralUsage { market, .. }
            | Call::Borrow { market, .. }
            | Call::Repay { market, .. }
            | Call::Liquidate { market, .. }
            | Call::LiquidateAndSell { market, .. } => market.clone(),
            Call::PublishOracle { feed, .. }
            | Call::PublishSpot { feed, .. }
            | Call::PublishTwap { feed, .. } => feed.clone(),
            _ => self.tx.sender.clone(),
        };
        self.trace.push(Trace {
            liquidation: None,
            route: None,
            clmm: None,
            book: None,
            twap: None,
            source_updates: vec![],
            solver: vec![],
            oracle: None,
            path: path.clone(),
            origin: self.tx.sender.clone(),
            caller: self.tx.sender.clone(),
            callee,
            operation: c.label().into(),
            outcome: "Attempt".into(),
            used: self.used,
            events: vec![],
        });
        let result = self
            .charge(10)
            .and_then(|()| self.body(w, c, &path))
            .and_then(|()| {
                match c {
                    Call::SwapExactIn { pool, .. }
                    | Call::SwapOutput { pool, .. }
                    | Call::AddLiquidity { pool, .. }
                    | Call::RemoveLiquidity { pool, .. } => {
                        self.trace[trace_index].source_updates =
                            crate::twap::sync_pool(self.s, w, pool)?;
                    }
                    _ => {}
                }
                Ok(())
            });
        let attempts = self.events[event_start..].to_vec();
        self.trace[trace_index].used = self.used;
        self.trace[trace_index].events = attempts;
        self.trace[trace_index].outcome = match &result {
            Ok(()) => "Success",
            Err(Exhausted) => "ResourceExhausted",
            Err(Protocol(_)) => "ProtocolRevert",
            Err(Model(_)) => "ModelError",
        }
        .into();
        if result.is_err() {
            *w = saved;
            self.outputs = saved_outputs;
            self.route_results = saved_routes;
            self.liquidations = saved_liquidations;
            self.events.truncate(event_start);
        }
        result
    }
    fn swap(
        &mut self,
        w: &mut World,
        pool: &str,
        asset_in: &str,
        amount: Amount,
        min_out: Amount,
        path: &str,
    ) -> Result<(), ExecutionError> {
        let sender = self.tx.sender.clone();
        let p = self.s.pool(pool).ok_or("unknown pool")?.clone();
        let stable = self
            .s
            .stable_pools
            .get(pool)
            .map(|cfg| cfg.parameters(self.s))
            .transpose()?;
        let (asset_out, out) = if let Some(params) = stable.as_ref() {
            let other = if asset_in == p.x {
                p.y.clone()
            } else if asset_in == p.y {
                p.x.clone()
            } else {
                return Err(Protocol("asset not in pool".into()));
            };
            let raw = [w.balance(&p.account, &p.x), w.balance(&p.account, &p.y)];
            let mut steps = vec![];
            let result = crate::stable::output(
                raw,
                asset_in == p.x,
                amount,
                params,
                &mut || self.charge(1),
                &mut steps,
            );
            self.trace
                .last_mut()
                .ok_or_else(|| Model("missing swap trace".into()))?
                .solver
                .extend(steps);
            (other, result?)
        } else {
            quote(self.s, w, pool, asset_in, amount)?
        };
        if out < min_out {
            return Err(Protocol("min_out".into()));
        }
        let before = w.balance(&p.account, &p.x).big() * w.balance(&p.account, &p.y).big();
        self.transfer(w, &sender, &p.account, asset_in, amount)?;
        self.transfer(w, &p.account, &sender, &asset_out, out)?;
        let after = w.balance(&p.account, &p.x).big() * w.balance(&p.account, &p.y).big();
        if stable.is_none() && after < before {
            return Err(Model("CPMM k decreased".into()));
        }
        self.outputs.insert(path.into(), (asset_out, out));
        Ok(())
    }
    fn body(&mut self, w: &mut World, c: &Call, path: &str) -> Result<(), ExecutionError> {
        let sender = self.tx.sender.clone();
        match c {
            Call::ExecuteRoute { .. } => self.route_call(w, c, path),
            Call::LiquidateAndSell { .. } => self.liquidation_sale(w, c, path),
            Call::Transfer { to, asset, amount } => {
                if self.s.protected(to) {
                    return Err(Protocol("protected protocol account".into()));
                }
                self.transfer(w, &sender, to, asset, *amount)
            }
            Call::Approve {
                spender,
                asset,
                amount,
            } => {
                if !self.s.accounts.contains_key(spender) || !self.s.assets.contains_key(asset) {
                    return Err(Protocol("unknown allowance reference".into()));
                }
                Arc::make_mut(&mut w.allowances)
                    .entry(sender)
                    .or_default()
                    .entry(spender.clone())
                    .or_default()
                    .insert(asset.clone(), *amount);
                Ok(())
            }
            Call::TransferFrom {
                from,
                to,
                asset,
                amount,
            } => {
                if self.s.protected(from) || self.s.protected(to) {
                    return Err(Protocol("protected protocol account".into()));
                }
                if from != &sender {
                    let allowance = w
                        .allowances
                        .get(from)
                        .and_then(|a| a.get(&sender))
                        .and_then(|a| a.get(asset))
                        .copied()
                        .unwrap_or_default();
                    let left = allowance
                        .checked_sub(*amount)
                        .map_err(|_| Protocol("insufficient allowance".into()))?;
                    Arc::make_mut(&mut w.allowances)
                        .entry(from.clone())
                        .or_default()
                        .entry(sender.clone())
                        .or_default()
                        .insert(asset.clone(), left);
                }
                self.transfer(w, from, to, asset, *amount)
            }
            Call::SwapExactIn {
                pool,
                asset_in,
                amount,
                min_out,
            } => self.swap(w, pool, asset_in, *amount, *min_out, path),
            Call::SwapOutput {
                pool,
                asset_in,
                output_of,
                min_out,
            } => {
                let (asset, amount) = self
                    .outputs
                    .get(output_of)
                    .cloned()
                    .ok_or_else(|| Model("validated output missing".into()))?;
                if &asset != asset_in {
                    return Err(Model("validated output type mismatch".into()));
                }
                self.swap(w, pool, asset_in, amount, *min_out, path)
            }
            Call::AddLiquidity {
                pool,
                max_x,
                max_y,
                min_shares,
            } => {
                let p = self.s.pool(pool).ok_or("unknown pool")?.clone();
                let total = *w
                    .shares
                    .get(pool)
                    .ok_or_else(|| Model("missing LP supply".into()))?;
                let x = w.balance(&p.account, &p.x);
                let y = w.balance(&p.account, &p.y);
                let (m, dx, dy) = if total.is_zero() {
                    if !x.is_zero() || !y.is_zero() {
                        return Err(Model("empty pool has reserves".into()));
                    }
                    let m = if let Some(cfg) = self.s.stable_pools.get(pool) {
                        let params = cfg.parameters(self.s)?;
                        let xp = params.normalized([*max_x, *max_y])?;
                        let mut steps = vec![];
                        let result = crate::stable::invariant(
                            xp,
                            &params,
                            &mut || self.charge(1),
                            &mut steps,
                        );
                        self.trace
                            .last_mut()
                            .ok_or_else(|| Model("missing LP trace".into()))?
                            .solver
                            .extend(steps);
                        result?
                    } else {
                        Amount::from_big((max_x.big() * max_y.big()).sqrt())?
                    };
                    (m, *max_x, *max_y)
                } else {
                    let mx = max_x.big() * total.big() / x.big();
                    let my = max_y.big() * total.big() / y.big();
                    let m = Amount::from_big(std::cmp::min(mx, my))?;
                    (m, m.mul_div(x, total, true)?, m.mul_div(y, total, true)?)
                };
                if m.is_zero() || m < *min_shares || dx > *max_x || dy > *max_y {
                    return Err(Protocol("min_shares or zero mint".into()));
                }
                if let Some(cfg) = self.s.stable_pools.get(pool) {
                    cfg.parameters(self.s)?
                        .normalized([x.checked_add(dx)?, y.checked_add(dy)?])?;
                }
                let next = total.checked_add(m)?;
                let held = w.lp_balances[pool]
                    .get(&sender)
                    .copied()
                    .unwrap_or_default()
                    .checked_add(m)?;
                self.transfer(w, &sender, &p.account, &p.x, dx)?;
                self.transfer(w, &sender, &p.account, &p.y, dy)?;
                w.shares.insert(pool.clone(), next);
                Arc::make_mut(&mut w.lp_balances)
                    .entry(pool.clone())
                    .or_default()
                    .insert(sender.clone(), held);
                self.events.push(Event {
                    kind: "MintShares".into(),
                    from: pool.clone(),
                    to: sender,
                    asset: format!("LP:{pool}"),
                    amount: m,
                });
                Ok(())
            }
            Call::RemoveLiquidity {
                pool,
                shares,
                min_x,
                min_y,
            } => {
                let p = self.s.pool(pool).ok_or("unknown pool")?.clone();
                let total = *w
                    .shares
                    .get(pool)
                    .ok_or_else(|| Model("missing LP supply".into()))?;
                let held = w.lp_balances[pool]
                    .get(&sender)
                    .copied()
                    .unwrap_or_default();
                if shares.is_zero() || *shares > held || total.is_zero() {
                    return Err(Protocol("insufficient LP shares".into()));
                }
                let dx = shares.mul_div(w.balance(&p.account, &p.x), total, false)?;
                let dy = shares.mul_div(w.balance(&p.account, &p.y), total, false)?;
                if dx.is_zero() || dy.is_zero() || dx < *min_x || dy < *min_y {
                    return Err(Protocol("min_assets or zero burn output".into()));
                }
                self.transfer(w, &p.account, &sender, &p.x, dx)?;
                self.transfer(w, &p.account, &sender, &p.y, dy)?;
                w.shares.insert(pool.clone(), total.checked_sub(*shares)?);
                Arc::make_mut(&mut w.lp_balances)
                    .entry(pool.clone())
                    .or_default()
                    .insert(sender.clone(), held.checked_sub(*shares)?);
                self.events.push(Event {
                    kind: "BurnShares".into(),
                    from: sender,
                    to: pool.clone(),
                    asset: format!("LP:{pool}"),
                    amount: *shares,
                });
                Ok(())
            }
            Call::FlashLoan {
                provider,
                receiver,
                amount,
                callback,
            } => {
                if receiver != &sender {
                    return Err(Protocol("receiver must be sender".into()));
                }
                if amount.is_zero() {
                    return Err(Protocol("zero flash principal".into()));
                }
                let p = self
                    .s
                    .flash_providers
                    .get(provider)
                    .ok_or("unknown flash provider")?
                    .clone();
                if !self.active_flash.insert(provider.clone()) {
                    return Err(Protocol("flash provider reentry".into()));
                }
                let fee = amount.mul_div((p.fee_bps as u64).into(), 10000.into(), true)?;
                let due = amount.checked_add(fee)?;
                let before = w.balance(&p.account, &p.asset);
                let result = (|| -> Result<(), ExecutionError> {
                    self.transfer(w, &p.account, receiver, &p.asset, *amount)?;
                    let i = self.trace.len();
                    // Callback is a real additional call, and consumes a node in admission validation.
                    let cb = Call::Sequence {
                        calls: callback.clone(),
                    };
                    let result = self.call(w, &cb, format!("{path}.callback"));
                    self.trace[i].caller = provider.clone();
                    self.trace[i].callee = receiver.clone();
                    self.trace[i].operation = "FlashCallback".into();
                    result?;
                    self.transfer(w, receiver, &p.account, &p.asset, due)?;
                    if w.balance(&p.account, &p.asset) != before.checked_add(fee)? {
                        return Err(Model("flash settlement invariant".into()));
                    }
                    self.events.push(Event {
                        kind: "FlashSettled".into(),
                        from: receiver.clone(),
                        to: provider.clone(),
                        asset: p.asset.clone(),
                        amount: fee,
                    });
                    Ok(())
                })();
                self.active_flash.remove(provider);
                result
            }
            Call::AssertNetBalanceDelta { asset, minimum, .. }
            | Call::AssertTransactionNetGain { asset, minimum } => {
                let baseline = match c {
                    Call::AssertNetBalanceDelta { baseline, .. } => *baseline,
                    _ => self.entry_fee_balance,
                };
                let refund = self
                    .tx
                    .limit
                    .checked_sub(self.used)?
                    .checked_mul(self.tx.price)?;
                let mut settled = w.balance(&sender, asset).checked_add(refund)?;
                if sender == self.s.chain.fee_account {
                    settled = settled.checked_add(self.used.checked_mul(self.tx.price)?)?;
                }
                if settled < baseline.checked_add(*minimum)? {
                    return Err(Protocol("net balance delta assertion".into()));
                }
                Ok(())
            }
            Call::AssertBalanceDelta {
                account,
                asset,
                baseline,
                minimum,
            } => {
                if !self.s.accounts.contains_key(account) || !self.s.assets.contains_key(asset) {
                    return Err(Protocol("unknown delta reference".into()));
                }
                if w.balance(account, asset) < baseline.checked_add(*minimum)? {
                    return Err(Protocol("balance delta assertion".into()));
                }
                Ok(())
            }
            Call::Supply { .. }
            | Call::Withdraw { .. }
            | Call::DepositCollateral { .. }
            | Call::WithdrawCollateral { .. }
            | Call::SetCollateralUsage { .. }
            | Call::Borrow { .. }
            | Call::Repay { .. }
            | Call::Liquidate { .. } => self.credit_call(w, c),
            Call::PlaceOrder { .. } | Call::CancelOrder { .. } => self.book_call(w, c),
            Call::MintPosition { .. }
            | Call::BurnPosition { .. }
            | Call::CollectPosition { .. }
            | Call::ClmmSwap { .. } => self.clmm_call(w, c),
            Call::PublishOracle { .. } | Call::PublishSpot { .. } | Call::PublishTwap { .. } => {
                self.oracle_call(w, c)
            }
            Call::AssertCondition { condition, message } => {
                if *condition {
                    Ok(())
                } else {
                    Err(Protocol(message.clone()))
                }
            }
            Call::AssertBalance {
                account,
                asset,
                minimum,
            } => {
                if !self.s.accounts.contains_key(account) || !self.s.assets.contains_key(asset) {
                    return Err(Protocol("unknown assertion reference".into()));
                }
                if w.balance(account, asset) >= *minimum {
                    Ok(())
                } else {
                    Err(Protocol("balance assertion".into()))
                }
            }
            Call::Sequence { calls } => {
                for (i, call) in calls.iter().enumerate() {
                    self.call(w, call, format!("{path}.{i}"))?;
                }
                Ok(())
            }
        }
    }
}

pub(crate) fn inclusion(
    s: &Scenario,
    w: &World,
    tx: &Transaction,
    remaining: Amount,
) -> Result<(), String> {
    s.validate_tx(tx)?;
    if tx.deadline < w.timestamp {
        return Err("expired deadline".into());
    }
    if w.accounts.get(&tx.sender).ok_or("unknown sender")?.nonce != tx.nonce {
        return Err("nonce mismatch (waiting or stale)".into());
    }
    if tx.limit > remaining {
        return Err("declared limit does not fit block".into());
    }
    if w.balance(&tx.sender, &s.chain.fee_asset) < tx.limit.checked_mul(tx.price)? {
        return Err("insufficient fee budget".into());
    }
    Ok(())
}
pub(crate) fn simulate_transaction(
    s: &Scenario,
    w: &World,
    tx: &Transaction,
) -> Result<Receipt, String> {
    inclusion(s, w, tx, s.chain.block_limit)?;
    let mut snapshot = w.clone();
    let receipt = execute(s, &mut snapshot, tx)?;
    snapshot.invariant(s)?;
    Ok(receipt)
}
pub(crate) fn probe_transaction(
    s: &Scenario,
    w: &World,
    tx: &Transaction,
) -> Result<crate::routing::heterogeneous::Trial, String> {
    use crate::routing::heterogeneous::Trial;
    if let Err(error) = inclusion(s, w, tx, s.chain.block_limit) {
        return Ok(Trial::Rejected(error));
    }
    let mut snapshot = w.clone();
    let receipt = execute(s, &mut snapshot, tx)?;
    snapshot.invariant(s)?;
    Ok(Trial::Executed(Box::new(receipt)))
}

pub(crate) fn execute(s: &Scenario, w: &mut World, tx: &Transaction) -> Result<Receipt, String> {
    let entry_fee_balance = w.balance(&tx.sender, &s.chain.fee_asset);
    let reserved = tx.limit.checked_mul(tx.price)?;
    w.debit(&tx.sender, &s.chain.fee_asset, reserved)?;
    Arc::make_mut(&mut w.accounts)
        .get_mut(&tx.sender)
        .ok_or("missing sender")?
        .nonce = tx.nonce.checked_add(1.into())?;
    let envelope = w.clone();
    let mut ctx = Context {
        s,
        tx,
        entry_fee_balance,
        used: Amount::ZERO,
        events: vec![],
        trace: vec![],
        charges: vec![],
        outputs: BTreeMap::new(),
        route_results: BTreeMap::new(),
        active_flash: BTreeSet::new(),
        liquidations: BTreeMap::new(),
    };
    let outcome = ctx.charge(100).and_then(|()| {
        for (i, c) in tx.program.iter().enumerate() {
            ctx.call(w, c, i.to_string())?;
        }
        if !ctx.active_flash.is_empty() {
            return Err(Model("unclosed flash scope".into()));
        }
        Ok(())
    });
    let (status, error) = match outcome {
        Ok(()) => ("Success", None),
        Err(Protocol(e)) => {
            *w = envelope;
            ctx.events.clear();
            ("ProtocolRevert", Some(e))
        }
        Err(Exhausted) => {
            *w = envelope;
            ctx.events.clear();
            ("ResourceExhausted", Some("resource limit".into()))
        }
        Err(Model(e)) => return Err(e),
    };
    let fee = ctx.used.checked_mul(tx.price)?;
    let refund = reserved.checked_sub(fee)?;
    w.credit(&tx.sender, &s.chain.fee_asset, refund)?;
    w.credit(&s.chain.fee_account, &s.chain.fee_asset, fee)?;
    if status != "Success" {
        ctx.route_results.clear();
        ctx.liquidations.clear();
        for t in &mut ctx.trace {
            if t.outcome == "Success" {
                t.outcome = "RolledBack".into();
            }
        }
    }
    w.invariant(s)?;
    Ok(Receipt {
        liquidations: ctx.liquidations,
        route_results: ctx.route_results,
        tx_id: tx.id.clone(),
        sender: tx.sender.clone(),
        status: status.into(),
        error,
        used: ctx.used,
        reserved,
        fee,
        refund,
        events: ctx.events,
        trace: ctx.trace,
        charges: ctx.charges,
    })
}

/// Build on a private snapshot. The caller installs the result only after success.
/// Input list is a candidate list, not an already-validated block sequence.
pub fn build_block(s: &Scenario, parent: &World, txs: &[Transaction]) -> Result<Block, String> {
    build_with_check(s, parent, txs, |_| Ok(()))
}
fn build_with_check(
    s: &Scenario,
    parent: &World,
    txs: &[Transaction],
    mut check: impl FnMut(&World) -> Result<(), String>,
) -> Result<Block, String> {
    let mut block = begin_block(s, parent)?;
    let w = &mut block.state;
    let mut used = Amount::ZERO;
    let mut receipts = vec![];
    let mut rejected = vec![];
    for tx in txs {
        if let Err(reason) = inclusion(s, w, tx, s.chain.block_limit.checked_sub(used)?) {
            rejected.push(Rejected {
                tx_id: tx.id.clone(),
                reason,
            });
            continue;
        }
        let r = execute(s, w, tx)?;
        used = used.checked_add(r.used)?;
        receipts.push(r);
        check(w)?;
    }
    w.invariant(s)?;
    block.used = used;
    block.receipts = receipts;
    block.rejected = rejected;
    Ok(block)
}
/// Shared system transition, exactly once for each canonical candidate block.
pub(crate) fn begin_block(s: &Scenario, parent: &World) -> Result<Block, String> {
    parent.invariant(s)?;
    let mut w = parent.clone();
    w.block = w.block.checked_add(1.into())?;
    w.timestamp = w.timestamp.checked_add(s.chain.interval)?;
    let mut system_events = crate::credit::accrue(s, &mut w)?;
    system_events.extend(crate::twap::advance(s, &mut w)?);
    w.invariant(s)?;
    Ok(Block {
        scheduler: None,
        system_events,
        decisions: vec![],
        number: w.block,
        timestamp: w.timestamp,
        receipts: vec![],
        rejected: vec![],
        used: Amount::ZERO,
        state: w,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_error_discards_clmm_crossings_and_fee_growth() {
        let s = Scenario::parse(include_bytes!(
            "../../../scenarios/demos/sc-clmm-crossing.json"
        ))
        .unwrap();
        let initial = World::genesis(&s).unwrap();
        let parent = build_block(&s, &initial, &s.blocks[0]).unwrap().state;
        let before = parent.clone();
        let mut checked = false;
        let result = build_with_check(&s, &parent, &s.blocks[1], |attempt| {
            checked = true;
            assert_eq!(attempt.clmm["clmm"].liquidity, 3000000.into());
            assert!(!attempt.clmm["clmm"].fee_growth[0].is_zero());
            Err("injected ModelError after CLMM crossing".into())
        });
        assert!(checked && result.is_err());
        assert_eq!(parent, before);
    }
    #[test]
    fn model_error_discards_book_escrow_index_and_order_sequence() {
        let s =
            Scenario::parse(include_bytes!("../../../scenarios/demos/sc-08-book.json")).unwrap();
        let parent = World::genesis(&s).unwrap();
        let before = parent.clone();
        let mut checked = false;
        let result = build_with_check(&s, &parent, &s.blocks[0], |attempt| {
            checked = true;
            assert_eq!(attempt.books["book"].next_id, 2.into());
            assert_eq!(attempt.balance("book-escrow", "X"), 20.into());
            Err("injected ModelError after order reservation".into())
        });
        assert!(checked && result.is_err());
        assert_eq!(parent, before);
    }
    #[test]
    fn model_error_after_success_cannot_commit_envelope_or_protocol_effects() {
        let s = Scenario::parse(include_bytes!("../../../scenarios/demos/sc-01.json")).unwrap();
        let parent = World::genesis(&s).unwrap();
        let before = parent.clone();
        let mut seen = 0;
        let result = build_with_check(&s, &parent, &s.blocks[0], |_| {
            seen += 1;
            Err("injected invariant failure after first transaction".into())
        });
        assert!(result.is_err());
        assert_eq!(seen, 1);
        assert_eq!(parent, before);
    }
    #[test]
    fn model_error_discards_system_interest_and_oracle_publication() {
        let s =
            Scenario::parse(include_bytes!("../../../scenarios/demos/sc-05-credit.json")).unwrap();
        let genesis = World::genesis(&s).unwrap();
        let parent = build_block(&s, &genesis, &s.blocks[0]).unwrap().state;
        let before = parent.clone();
        let mut checked = false;
        let result = build_with_check(&s, &parent, &s.blocks[1], |attempt| {
            checked = true;
            assert_eq!(attempt.lending["credit"].debt, 90.into());
            assert_eq!(
                attempt.feeds["feed"]
                    .record
                    .as_ref()
                    .unwrap()
                    .price
                    .numerator(),
                11.into()
            );
            Err("injected model error after accrual and publication".into())
        });
        assert!(checked && result.is_err());
        assert_eq!(parent, before);
        assert_eq!(parent.lending["credit"].debt, 80.into());
    }
    #[test]
    fn model_error_discards_price_integral_and_twap_publication() {
        let s = Scenario::parse(include_bytes!("../../../scenarios/demos/sc-twap.json")).unwrap();
        let genesis = World::genesis(&s).unwrap();
        let parent = build_block(&s, &genesis, &s.blocks[0]).unwrap().state;
        let before = parent.clone();
        let mut checked = false;
        let result = build_with_check(&s, &parent, &s.blocks[1], |attempt| {
            checked = true;
            assert_eq!(attempt.price_history["cpmm-price"].last_time, 60.into());
            assert_eq!(
                attempt.feeds["twap"]
                    .record
                    .as_ref()
                    .unwrap()
                    .price
                    .numerator(),
                150.into()
            );
            Err("injected ModelError after integral and TWAP".into())
        });
        assert!(checked && result.is_err());
        assert_eq!(parent, before);
        assert_eq!(parent.price_history["cpmm-price"].last_time, 30.into());
        assert!(parent.feeds["twap"].record.is_none());
    }
}
