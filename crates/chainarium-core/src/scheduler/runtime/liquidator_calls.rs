use super::*;
use crate::policy::liquidator as l;

impl Round<'_> {
    pub(super) fn liquidator_decision(&mut self, agent: &str) -> Result<(), String> {
        let cfg = self
            .c
            .liquidator
            .as_ref()
            .ok_or("missing liquidator config")?;
        let p = &cfg.policies[agent];
        let mut d = l::Decision::new(agent, p, self.parent);
        let nonce = self.parent.accounts[&p.account].nonce;
        let delivery = &self.c.agents[agent].outgoing;
        let invalid_time = self
            .parent
            .timestamp
            .checked_add(delivery.builder_delay)
            .is_err()
            || self.c.agents.iter().any(|(recipient, n)| {
                let visible = match &delivery.channel {
                    Channel::Public => n.observe_public,
                    Channel::Private { recipients } => recipients.contains(recipient),
                };
                visible
                    && self
                        .parent
                        .timestamp
                        .checked_add(n.observation_delay)
                        .is_err()
            });
        if self.state.busy(agent, &p.account, nonce) {
            d.reason = "BusyNonce".into();
        } else if nonce.checked_add(1.into()).is_err() {
            d.reason = "NonceExhausted".into();
        } else if self.parent.timestamp.checked_add(p.lifetime).is_err() {
            d.reason = "DeadlineOverflow".into();
        } else if invalid_time {
            d.reason = "DeliveryTimeOverflow".into();
        } else if self.liquidators.metrics.trials >= cfg.limits.trials_per_round
            || self.liquidators.metrics.venue_checks >= cfg.limits.venue_checks
        {
            d.reason = "SearchLimit".into();
            d.truncated = true;
            self.liquidators.metrics.truncated = true;
        } else {
            if self.system_preview.is_none() {
                self.system_preview = Some(engine::begin_block(self.s, self.parent)?.state);
            }
            let preview = self.system_preview.as_ref().unwrap();
            let observation = l::LiquidatorObservation::capture(self.s, self.parent, preview, p)?;
            let s = self.s;
            let check = self.probe_check;
            let mut simulate = |tx: &Transaction| -> Result<l::Trial, String> {
                if tx.agent != agent || tx.sender != p.account {
                    return Err("unauthorized liquidator simulation".into());
                }
                if let Err(reason) = engine::inclusion(s, preview, tx, s.chain.block_limit) {
                    return Ok(l::Trial {
                        receipt: None,
                        outcome: format!("Rejected: {reason}"),
                        final_balance: preview.balance(&p.account, &s.chain.fee_asset),
                    });
                }
                let mut world = preview.clone();
                let receipt = engine::execute(s, &mut world, tx)?;
                check(&world)?;
                Ok(l::Trial {
                    outcome: receipt.status.clone(),
                    receipt: Some(receipt),
                    final_balance: world.balance(&p.account, &s.chain.fee_asset),
                })
            };
            l::Search {
                policy: p,
                chain: &s.chain,
                limits: &cfg.limits,
                observation: &observation,
            }
            .run(&mut d, &mut self.liquidators.metrics, &mut simulate)?;
            d.observation = Some(observation);
        }
        self.note(
            None,
            self.parent.timestamp,
            None,
            Some(agent),
            "Liquidator",
            &d.reason,
        );
        if let Some(tx) = &d.transaction {
            *self.intents.entry(agent.into()).or_default() += 1;
            self.submit(tx.clone(), delivery.clone(), 0, self.parent.timestamp, None)?;
        }
        self.liquidators.decisions.push(d);
        Ok(())
    }
}
