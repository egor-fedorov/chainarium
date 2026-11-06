use super::*;
use crate::hooks::{self, Action, Effect, LiquidityResult, Point};
impl Context<'_> {
    // Internal frames cannot be supplied as user Calls. Effects pass capability checks below.
    fn hook_frame(
        &mut self,
        w: &mut World,
        path: String,
        meta: hooks::Trace,
        caller: &str,
        callee: &str,
        f: impl FnOnce(&mut Self, &mut World) -> Result<(), ExecutionError>,
    ) -> Result<(), ExecutionError> {
        let saved = w.clone();
        let event_start = self.events.len();
        let ti = self.trace.len();
        let id = meta.hook.clone();
        self.trace.push(Trace {
            hook: Some(meta),
            liquidation: None,
            route: None,
            clmm: None,
            book: None,
            twap: None,
            source_updates: vec![],
            solver: vec![],
            oracle: None,
            path,
            origin: self.tx.sender.clone(),
            caller: caller.into(),
            callee: callee.into(),
            operation: "Hook".into(),
            outcome: "Attempt".into(),
            used: self.used,
            events: vec![],
        });
        let result = self.charge(10).and_then(|()| f(self, w));
        self.trace[ti].used = self.used;
        self.trace[ti].events = self.events[event_start..].to_vec();
        self.trace[ti].hook.as_mut().unwrap().counter_after = w
            .hooks
            .get(&id)
            .ok_or_else(|| Model("missing hook state".into()))?
            .counter;
        self.trace[ti].outcome = match &result {
            Ok(()) => "Success",
            Err(Protocol(_)) => "ProtocolRevert",
            Err(Exhausted) => "ResourceExhausted",
            Err(Model(_)) => "ModelError",
        }
        .into();
        if result.is_err() {
            *w = saved;
            self.events.truncate(event_start);
        }
        result
    }
    pub(super) fn run_hook(
        &mut self,
        w: &mut World,
        pool: &str,
        point: Point,
        action: Action,
        path: &str,
    ) -> Result<Option<u16>, ExecutionError> {
        let Some((id, cfg)) = hooks::binding(self.s, pool) else {
            return Ok(None);
        };
        let Some(program) = cfg.programs.get(&point).filter(|p| !p.is_empty()) else {
            return Ok(None);
        };
        let (id, cfg, program) = (id.clone(), cfg.clone(), program.clone());
        let p = self
            .s
            .protocols
            .get(pool)
            .ok_or_else(|| Model("hook pool identity".into()))?
            .clone();
        let sender = self.tx.sender.clone();
        let mut observation =
            hooks::observation(w, &id, &cfg, &p, point, &sender, action).map_err(Model)?;
        let base_path = format!("{path}.hook.{point:?}");
        let entry = self.trace.len();
        let mut fee = None;
        let meta = hooks::Trace {
            hook: id.clone(),
            point,
            instruction: None,
            observation: Some(observation.clone()),
            counter_before: observation.counter,
            counter_after: observation.counter,
            effective_fee: None,
        };
        self.hook_frame(w, base_path.clone(), meta, pool, &cfg.account, |ctx, w| {
            let mut spent: BTreeMap<String, Amount> = BTreeMap::new();
            for (i, instruction) in program.iter().enumerate() {
                observation.counter = w.hooks[&id].counter;
                observation.balances = w.accounts[&cfg.account].balances.clone();
                let meta = hooks::Trace {
                    hook: id.clone(),
                    point,
                    instruction: Some(instruction.clone()),
                    observation: None,
                    counter_before: observation.counter,
                    counter_after: observation.counter,
                    effective_fee: None,
                };
                let ti = ctx.trace.len();
                ctx.hook_frame(
                    w,
                    format!("{base_path}.{i}"),
                    meta,
                    &cfg.account,
                    &cfg.account,
                    |ctx, w| {
                        hooks::authorize(&cfg, point, instruction)?;
                        match instruction.evaluate(&observation)? {
                            Effect::None => {}
                            Effect::Fee(value) => {
                                if value >= 10000 {
                                    return Err(Protocol("HookInvalidFee".into()));
                                }
                                fee = Some(value);
                                ctx.trace[ti].hook.as_mut().unwrap().effective_fee = fee;
                                ctx.events.push(Event {
                                    kind: "HookFee".into(),
                                    from: id.clone(),
                                    to: pool.into(),
                                    asset: "bps".into(),
                                    amount: (value as u64).into(),
                                });
                            }
                            Effect::Counter(value) => {
                                w.hooks
                                    .get_mut(&id)
                                    .ok_or_else(|| Model("missing hook counter".into()))?
                                    .counter = value;
                                ctx.events.push(Event {
                                    kind: "HookCounter".into(),
                                    from: id.clone(),
                                    to: pool.into(),
                                    asset: format!("counter:{id}"),
                                    amount: value,
                                });
                            }
                            Effect::Pay(asset, amount) => {
                                let total = spent.entry(asset.clone()).or_default();
                                *total = total.checked_add(amount)?;
                                if *total > cfg.capabilities.spend_own[&asset] || amount.is_zero() {
                                    return Err(Protocol("HookSpendLimit".into()));
                                }
                                ctx.transfer(w, &cfg.account, &sender, &asset, amount)?;
                            }
                        }
                        Ok(())
                    },
                )?;
            }
            Ok(())
        })?;
        self.trace[entry].hook.as_mut().unwrap().effective_fee = fee;
        Ok(fee)
    }
    pub(super) fn liquidity_hook(
        &mut self,
        w: &mut World,
        c: &Call,
        point: Point,
        actual: Option<LiquidityResult>,
        path: &str,
    ) -> Result<(), ExecutionError> {
        if let Some((pool, action)) = hooks::liquidity_action(c, actual) {
            self.run_hook(w, pool, point, action, path)?;
        }
        Ok(())
    }
}
