use super::*;
use crate::policy::flash_arbitrage as f;

impl Round<'_> {
    pub(super) fn flash_arbitrage_decision(&mut self, agent: &str) -> Result<(), String> {
        let cfg = self
            .c
            .flash_arbitrage
            .as_ref()
            .ok_or("missing flash arbitrage config")?;
        let p = &cfg.policies[agent];
        let mut d = f::Decision::new(agent, (self.round as u64).into());
        let nonce = self.parent.accounts[&p.account].nonce;
        let delivery = &self.c.agents[agent].outgoing;
        let invalid_time = self
            .parent
            .timestamp
            .checked_add(delivery.builder_delay)
            .is_err()
            || self.c.agents.iter().any(|(recipient, network)| {
                let visible = match &delivery.channel {
                    Channel::Public => network.observe_public,
                    Channel::Private { recipients } => recipients.contains(recipient),
                };
                visible
                    && self
                        .parent
                        .timestamp
                        .checked_add(network.observation_delay)
                        .is_err()
            });
        if self.state.busy(agent, &p.account, nonce) {
            d.skip("BusyNonce");
        } else if nonce.checked_add(1.into()).is_err() {
            d.skip("NonceExhausted");
        } else if self.parent.timestamp.checked_add(p.lifetime).is_err() {
            d.skip("DeadlineOverflow");
        } else if invalid_time {
            d.skip("DeliveryTimeOverflow");
        } else if self.flash_arbitrage.metrics.trials >= cfg.limits.trials_per_round
            || self.flash_arbitrage.metrics.venue_checks >= cfg.limits.venue_checks
        {
            d.skip("SearchLimit");
            d.truncated = true;
            self.flash_arbitrage.metrics.truncated = true;
        } else {
            if self.system_preview.is_none() {
                self.system_preview = Some(engine::begin_block(self.s, self.parent)?.state);
            }
            let preview = self.system_preview.as_ref().unwrap();
            let observation = f::Observation::capture(self.s, self.parent, preview, p)?;
            let deadline = self.parent.timestamp.checked_add(p.lifetime)?;
            let s = self.s;
            let check = self.probe_check;
            let mut simulate = |tx: &Transaction| -> Result<f::Trial, String> {
                if !p.permits(tx, agent, &s.chain, &observation, deadline, cfg.limits.hops) {
                    return Err("unauthorized flash arbitrage simulation".into());
                }
                if let Err(reason) = engine::inclusion(s, preview, tx, s.chain.block_limit) {
                    return Ok(f::Trial {
                        receipt: None,
                        outcome: format!("Rejected: {reason}"),
                        final_balance: preview.balance(&p.account, &p.asset),
                    });
                }
                let mut world = preview.clone();
                let receipt = engine::execute(s, &mut world, tx)?;
                check(&world)?;
                Ok(f::Trial {
                    outcome: receipt.status.clone(),
                    receipt: Some(receipt),
                    final_balance: world.balance(&p.account, &p.asset),
                })
            };
            f::Search {
                policy: p,
                chain: &s.chain,
                limits: &cfg.limits,
                observation: &observation,
            }
            .run(&mut d, &mut self.flash_arbitrage.metrics, &mut simulate)?;
            d.observation = Some(observation);
        }
        if let Some(tx) = &d.transaction {
            *self.intents.entry(agent.into()).or_default() += 1;
            let outcome =
                self.submit(tx.clone(), delivery.clone(), 0, self.parent.timestamp, None)?;
            if let SubmissionOutcome::Rejected { reason } = &outcome {
                d.skip(&format!("Submission: {}", reason.label()));
            }
            d.submission = Some(outcome);
        }
        self.note(
            None,
            self.parent.timestamp,
            d.transaction.as_ref().map(|tx| tx.id.as_str()),
            Some(agent),
            "FlashArbitrage",
            &d.reason,
        );
        self.flash_arbitrage.decisions.push(d);
        Ok(())
    }
}
