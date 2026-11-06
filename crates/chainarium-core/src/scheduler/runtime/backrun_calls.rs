use super::*;

impl Round<'_> {
    pub(super) fn backrun_reaction(
        &mut self,
        id: &str,
        agent: &str,
        depth: usize,
        event: &Queued,
        at: Amount,
    ) -> Result<(), String> {
        let mut decision = backrun::Decision::new(agent, id, self.parent.block, at);
        self.search_reaction(&mut decision, depth, event)?;
        self.note(
            Some(event),
            at,
            Some(id),
            Some(agent),
            "Backrun",
            &decision.reason,
        );
        if let Some(bundle) = &decision.bundle {
            *self.intents.entry(agent.into()).or_default() += 1;
            self.submit_bundle(bundle, &self.c.agents[agent].outgoing, event, at, true)?;
        }
        self.search.decisions.push(decision);
        Ok(())
    }
    fn search_reaction(
        &mut self,
        d: &mut backrun::Decision,
        depth: usize,
        event: &Queued,
    ) -> Result<(), String> {
        if self.metrics.reactions >= self.c.limits.reactions_per_round {
            d.reason = "ReactionLimit".into();
            return Ok(());
        }
        self.metrics.reactions += 1;
        let cfg = self.c.backrun.as_ref().ok_or("missing backrun config")?;
        let p = cfg.policies.get(&d.agent).ok_or("unknown backrun agent")?;
        let obs = self
            .state
            .observation(self.s, self.parent, &d.agent, &p.account)?;
        let reason = if depth >= self.c.limits.depth {
            Some("DepthLimit")
        } else if self.intents.get(&d.agent).copied().unwrap_or_default()
            >= self.c.limits.intents_per_agent
        {
            Some("IntentLimit")
        } else if self.state.busy(&d.agent, &p.account, obs.market.nonce) {
            Some("BusyNonce")
        } else {
            None
        };
        if let Some(reason) = reason {
            d.reason = reason.into();
            return Ok(());
        }
        let Some(trigger) = obs
            .messages
            .iter()
            .find(|m| m.transaction.id == d.trigger)
            .map(|m| &m.transaction)
        else {
            d.reason = "TriggerUnavailable".into();
            return Ok(());
        };
        if trigger.agent == d.agent
            || self.s.agents[&d.agent]
                .controlled_accounts
                .contains(&trigger.sender)
        {
            d.reason = "OwnTrigger".into();
            return Ok(());
        }
        if self.search.metrics.members >= cfg.limits.member_attempts
            || self.search.metrics.candidates >= cfg.limits.candidates_per_round
            || self.search.metrics.venue_checks >= cfg.limits.venue_checks
        {
            self.search.metrics.truncated = true;
            d.truncated = true;
            d.reason = "SearchLimit".into();
            return Ok(());
        }
        let mut prefix = engine::begin_block(self.s, self.parent)?;
        self.search.metrics.prefixes += 1;
        self.search.metrics.members += 1;
        if let Err(reason) =
            engine::inclusion(self.s, &prefix.state, trigger, self.s.chain.block_limit)
        {
            d.reason = format!("TriggerRejected: {reason}");
            return Ok(());
        }
        let receipt = engine::execute(self.s, &mut prefix.state, trigger)?;
        (self.probe_check)(&prefix.state)?;
        prefix.used = receipt.used;
        self.search.metrics.used = self.search.metrics.used.checked_add(receipt.used)?;
        let success = receipt.status == "Success";
        d.prefix = Some(receipt);
        if !success {
            d.reason = "TriggerFailed".into();
            return Ok(());
        }
        let preview = Observation::capture(self.s, &prefix.state, &p.account)?;
        let s = self.s;
        let check = self.probe_check;
        let agent = d.agent.clone();
        let mut simulate = |transactions: &[Transaction]| -> Result<backrun::Trial, String> {
            let mut world = prefix.state.clone();
            let mut remaining = s.chain.block_limit.checked_sub(prefix.used)?;
            let mut trial = backrun::Trial {
                outcome: "Success".into(),
                final_balance: Amount::ZERO,
                attempts: 0,
                receipts: vec![],
            };
            for tx in transactions {
                if tx.agent != agent || tx.sender != p.account {
                    return Err("unauthorized backrun simulation".into());
                }
                trial.attempts += 1;
                if let Err(reason) = engine::inclusion(s, &world, tx, remaining) {
                    trial.outcome = format!("Rejected: {reason}");
                    break;
                }
                let receipt = engine::execute(s, &mut world, tx)?;
                check(&world)?;
                remaining = remaining.checked_sub(receipt.used)?;
                let success = receipt.status == "Success";
                if !success {
                    trial.outcome = receipt.status.clone();
                }
                trial.receipts.push(receipt);
                if !success {
                    break;
                }
            }
            trial.final_balance = world.balance(&p.account, &s.chain.fee_asset);
            Ok(trial)
        };
        let search = backrun::Search {
            policy: p,
            chain: &s.chain,
            observation: &preview,
            limits: &cfg.limits,
            target: prefix.number,
            event_id: event.id,
        };
        // Callback authorization uses a separate immutable identifier from the decision being updated.
        search.run(d, &mut self.search.metrics, &mut simulate)?;
        Ok(())
    }
}
