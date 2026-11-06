use super::*;
use crate::policy::agents as a;

impl Round<'_> {
    pub(super) fn baseline_decision(
        &mut self,
        agent: &str,
        rng: &mut crate::run::Streams,
    ) -> Result<(), String> {
        let p = &self.s.baseline_policies[agent];
        let mut d = a::Decision::new(agent, p, (self.round as u64).into());
        if p.eligible(d.round) {
            // Sampling precedes all state-dependent checks, including BusyNonce.
            if let a::Kind::RandomUser { programs, .. } = &p.kind {
                d.selection = Some(a::select(rng, agent, programs.len())?);
            }
            let exhausted = d.selection.as_ref().is_some_and(|s| {
                s.gate.value.is_none() || s.template.as_ref().is_none_or(|t| t.value.is_none())
            });
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
            if exhausted {
                d.skip("RngBudget");
            } else if self.state.busy(agent, &p.account, nonce) {
                d.skip("BusyNonce");
            } else if nonce.checked_add(1.into()).is_err() {
                d.skip("NonceExhausted");
            } else if self.parent.timestamp.checked_add(p.lifetime).is_err() {
                d.skip("DeadlineOverflow");
            } else if invalid_time {
                d.skip("DeliveryTimeOverflow");
            } else {
                if self.system_preview.is_none() {
                    self.system_preview = Some(engine::begin_block(self.s, self.parent)?.state);
                }
                let preview = self.system_preview.as_ref().unwrap();
                let observation = a::Observation::capture(self.s, self.parent, preview, p)?;
                let tx_id = format!("policy:baseline:{agent}:{}", d.round);
                p.plan(&mut d, &observation, &tx_id)?;
                d.observation = Some(observation);
                if !d.program.is_empty() {
                    let tx = Transaction {
                        id: tx_id,
                        agent: agent.into(),
                        sender: p.account.clone(),
                        chain: self.s.chain.id.clone(),
                        nonce,
                        limit: p.resource_limit,
                        price: p.resource_price,
                        deadline: self.parent.timestamp.checked_add(p.lifetime)?,
                        program: d.program.clone(),
                    };
                    match engine::inclusion(self.s, preview, &tx, self.s.chain.block_limit)
                        .and_then(|()| a::validate_program_access(&tx.program, &p.account))
                    {
                        Err(reason) => d.skip(&format!("Rejected: {reason}")),
                        Ok(()) => {
                            let mut world = preview.clone();
                            let receipt = engine::execute(self.s, &mut world, &tx)?;
                            (self.probe_check)(&world)?;
                            if receipt.status == "Success"
                                || p.preflight() == a::Preflight::AnyReceipt
                            {
                                d.action = policy::DecisionAction::Submit;
                                d.reason = if receipt.status == "Success" {
                                    "PreflightSuccess"
                                } else {
                                    "SubmitPredictedFailure"
                                }
                                .into();
                                d.transaction = Some(tx);
                            } else {
                                d.skip(&format!(
                                    "Preflight: {}",
                                    receipt.error.as_deref().unwrap_or(&receipt.status)
                                ));
                            }
                            d.trial = Some(receipt);
                        }
                    }
                }
            }
        }
        if let Some(tx) = &d.transaction {
            *self.intents.entry(agent.into()).or_default() += 1;
            let outcome = self.submit(
                tx.clone(),
                self.c.agents[agent].outgoing.clone(),
                0,
                self.parent.timestamp,
                None,
            )?;
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
            "Baseline",
            &d.reason,
        );
        self.baselines.push(d);
        Ok(())
    }
}
