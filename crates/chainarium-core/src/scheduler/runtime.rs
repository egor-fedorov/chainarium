use super::*;
mod backrun_calls;
mod baseline_calls;
mod bundle_calls;
mod liquidator_calls;
use crate::{Block, Rejected, engine, policy};

struct Round<'a> {
    s: &'a Scenario,
    c: &'a Config,
    parent: &'a World,
    state: State,
    trace: Vec<Trace>,
    metrics: Metrics,
    probes: Vec<bundles::Probe>,
    search: backrun::Report,
    liquidators: policy::liquidator::Report,
    baselines: Vec<policy::agents::Decision>,
    system_preview: Option<World>,
    probe_check: &'a dyn Fn(&World) -> Result<(), String>,
    round: usize,
    cutoff: Amount,
    intents: BTreeMap<String, usize>,
}
impl Round<'_> {
    fn note(
        &mut self,
        event: Option<&Queued>,
        at: Amount,
        tx: Option<&str>,
        agent: Option<&str>,
        action: &str,
        detail: &str,
    ) {
        *self.metrics.actions.entry(action.into()).or_default() += 1;
        self.trace.push(Trace {
            event_id: event.map(|e| e.id),
            due_at: event.map_or(at, |e| e.due_at),
            at,
            tx_id: tx.map(str::to_owned),
            agent: agent.map(str::to_owned),
            action: action.into(),
            detail: detail.into(),
        });
    }
    fn enqueue(
        &mut self,
        due_at: Amount,
        kind: EventKind,
        at: Amount,
        tx: Option<&str>,
        agent: Option<&str>,
    ) -> Result<bool, String> {
        if self.state.queue.len() >= self.c.limits.queue {
            self.note(
                None,
                at,
                tx,
                agent,
                "QueueLimit",
                "new event dropped; no fee or nonce",
            );
            return Ok(false);
        }
        let id = self.state.next_event_id;
        self.state.next_event_id = id.checked_add(1.into())?;
        self.state.queue.push(Queued { id, due_at, kind });
        Ok(true)
    }
    fn reject(&mut self, id: &str, reason: &str) {
        self.state.pending.remove(id);
        if let Some(k) = self.state.known.get_mut(id) {
            k.status = Status::Rejected {
                reason: reason.into(),
            };
        }
    }
    fn submit(
        &mut self,
        tx: Transaction,
        delivery: Delivery,
        depth: usize,
        at: Amount,
        event: Option<&Queued>,
    ) -> Result<(), String> {
        let id = tx.id.clone();
        if let Some(k) = self.state.known.get(&id)
            && (k.standalone
                || k.transaction != tx
                || matches!(k.status, Status::Included { .. } | Status::Rejected { .. }))
        {
            let outcome = if k.transaction == tx {
                "Duplicate"
            } else {
                "ConflictingTxId"
            };
            self.note(
                event,
                at,
                Some(&id),
                Some(&tx.agent),
                outcome,
                "first submission retained; no implicit retry",
            );
            return Ok(());
        }
        if !self.state.known.contains_key(&id) && self.state.known.len() >= self.c.limits.known {
            self.note(
                event,
                at,
                Some(&id),
                Some(&tx.agent),
                "KnownTransactionsLimit",
                "new transaction dropped",
            );
            return Ok(());
        }
        self.note(
            event,
            at,
            Some(&id),
            Some(&tx.agent),
            "Sent",
            &format!("{:?}; depth {depth}", delivery.channel),
        );
        self.state.known.insert(
            id.clone(),
            Known {
                standalone: true,
                transaction: tx,
                delivery: delivery.clone(),
                submitted_at: at,
                depth,
                status: Status::InFlight,
                committed_status: Status::InFlight,
            },
        );
        if !self.enqueue(
            at.checked_add(delivery.builder_delay)?,
            EventKind::BuilderDelivery { tx_id: id.clone() },
            at,
            Some(&id),
            None,
        )? {
            self.reject(&id, "QueueLimit");
        }
        for (agent, n) in &self.c.agents {
            let visible = match &delivery.channel {
                Channel::Public => n.observe_public,
                Channel::Private { recipients } => recipients.contains(agent),
            };
            if visible {
                self.enqueue(
                    at.checked_add(n.observation_delay)?,
                    EventKind::AgentDelivery {
                        tx_id: id.clone(),
                        agent: agent.clone(),
                    },
                    at,
                    Some(&id),
                    Some(agent),
                )?;
            }
        }
        Ok(())
    }
    fn admission(&mut self, id: &str, event: &Queued, at: Amount) -> Result<(), String> {
        if matches!(self.state.known[id].status, Status::Included { .. }) {
            self.note(
                Some(event),
                at,
                Some(id),
                None,
                "AlreadyIncluded",
                "member already selected through a bundle; original receipt retained",
            );
            return Ok(());
        }
        let tx = &self.state.known[id].transaction;
        let invalid = self
            .s
            .validate_tx(tx)
            .err()
            .or_else(|| (tx.deadline < at).then(|| "ExpiredDeadline".into()))
            .or_else(|| (tx.limit > self.s.chain.block_limit).then(|| "UnfitDeclaredLimit".into()))
            .or_else(|| {
                self.parent
                    .accounts
                    .get(&tx.sender)
                    .and_then(|a| (tx.nonce < a.nonce).then(|| "StaleNonce".into()))
            })
            .or_else(|| {
                (self.state.pending.len() + self.state.bundle_pending.len()
                    >= self.c.limits.pending)
                    .then(|| "MempoolLimit".into())
            });
        if let Some(reason) = invalid {
            self.reject(id, &reason);
            self.note(Some(event), at, Some(id), None, "Rejected", &reason);
            return Ok(());
        }
        let waiting = if tx.nonce > self.parent.accounts[&tx.sender].nonce {
            Some("FutureNonce")
        } else if self.parent.balance(&tx.sender, &self.s.chain.fee_asset)
            < tx.limit.checked_mul(tx.price)?
        {
            Some("FeeBudget")
        } else {
            None
        };
        self.state.pending.insert(
            id.into(),
            Pending {
                arrival: at,
                event_id: event.id,
                round: self.round,
            },
        );
        self.state.known.get_mut(id).unwrap().status =
            waiting.map_or(Status::Pending, |r| Status::Waiting { reason: r.into() });
        self.note(
            Some(event),
            at,
            Some(id),
            None,
            "Admitted",
            waiting.unwrap_or("Ready; no balance reserve or nonce change"),
        );
        Ok(())
    }
    fn visible(&mut self, id: &str, agent: &str, event: &Queued, at: Amount) -> Result<(), String> {
        let k = &self.state.known[id];
        let message = Message {
            transaction: k.transaction.clone(),
            channel: k.delivery.channel.clone(),
            submitted_at: k.submitted_at,
            delivered_at: at,
            event_id: event.id,
            round: self.round,
            depth: k.depth,
        };
        let depth = k.depth;
        let backrun_delay = self
            .c
            .backrun
            .as_ref()
            .and_then(|c| c.policies.get(agent))
            .filter(|_| {
                k.transaction.agent != agent
                    && !self.s.agents[agent]
                        .controlled_accounts
                        .contains(&k.transaction.sender)
            })
            .map(|p| p.reaction_delay);
        let list = self.state.visible.entry(agent.into()).or_default();
        list.push(message);
        self.trim_visible(agent, event, at);
        self.note(
            Some(event),
            at,
            Some(id),
            Some(agent),
            "Visible",
            "authorized message delivery",
        );
        if let Some(delay) = backrun_delay
            && self
                .state
                .triggered
                .entry(agent.into())
                .or_default()
                .insert(id.into())
        {
            match at.checked_add(delay) {
                Ok(due_at) => {
                    self.enqueue(
                        due_at,
                        EventKind::BackrunReaction {
                            tx_id: id.into(),
                            agent: agent.into(),
                            depth,
                        },
                        at,
                        Some(id),
                        Some(agent),
                    )?;
                }
                Err(_) => self.note(
                    Some(event),
                    at,
                    Some(id),
                    Some(agent),
                    "BackrunReactionTimeOverflow",
                    "reaction dropped: due time is not representable",
                ),
            }
        }
        if let Some(p) = self.c.reactive.get(agent)
            && p.rules.contains_key(id)
            && self
                .state
                .triggered
                .entry(agent.into())
                .or_default()
                .insert(id.into())
        {
            self.enqueue(
                at.checked_add(p.reaction_delay)?,
                EventKind::Reaction {
                    tx_id: id.into(),
                    agent: agent.into(),
                    depth,
                },
                at,
                Some(id),
                Some(agent),
            )?;
        }
        Ok(())
    }
    fn reaction(
        &mut self,
        id: &str,
        agent: &str,
        depth: usize,
        event: &Queued,
        at: Amount,
        decisions: &mut Vec<policy::Decision>,
    ) -> Result<(), String> {
        if self.metrics.reactions >= self.c.limits.reactions_per_round {
            self.note(
                Some(event),
                at,
                Some(id),
                Some(agent),
                "ReactionLimit",
                "reaction dropped without intent",
            );
            return Ok(());
        }
        self.metrics.reactions += 1;
        let reason = if depth >= self.c.limits.depth {
            Some("DepthLimit")
        } else if self.intents.get(agent).copied().unwrap_or_default()
            >= self.c.limits.intents_per_agent
        {
            Some("IntentLimit")
        } else {
            None
        };
        if let Some(reason) = reason {
            self.note(
                Some(event),
                at,
                Some(id),
                Some(agent),
                reason,
                "no new intent",
            );
            return Ok(());
        }
        let p = &self.c.reactive[agent];
        // The trigger is a queued authorization token from a prior delivery, not access to a global mempool.
        let obs = self
            .state
            .observation(self.s, self.parent, agent, &p.account)?;
        let d = p.decide(agent, &self.s.chain, &obs, id, event.id, at)?;
        let tx = d
            .transaction
            .clone()
            .ok_or("reactive policy omitted its intent")?;
        decisions.push(d);
        *self.intents.entry(agent.into()).or_default() += 1;
        self.note(
            Some(event),
            at,
            Some(id),
            Some(agent),
            "Reaction",
            "literal program; committed nonce; no search",
        );
        self.submit(
            tx,
            self.c.agents[agent].outgoing.clone(),
            depth + 1,
            at,
            Some(event),
        )
    }
    fn expire_visible(&mut self) {
        self.expire_bundle_visible();
        let mut removed = vec![];
        for (agent, messages) in &mut self.state.visible {
            messages.retain(|m| {
                let retain = self.round - m.round < self.c.limits.retention;
                if !retain {
                    removed.push((agent.clone(), m.transaction.id.clone()));
                }
                retain
            });
        }
        for (agent, id) in removed {
            self.note(
                None,
                self.parent.timestamp,
                Some(&id),
                Some(&agent),
                "TTLExpired",
                "visible message retention",
            );
        }
    }
    fn prune(&mut self, block: &mut Block) {
        let mut removed = vec![];
        for (id, p) in &self.state.pending {
            let tx = &self.state.known[id].transaction;
            let reason = if tx.deadline < block.timestamp {
                Some("ExpiredDeadline")
            } else if tx.nonce < block.state.accounts[&tx.sender].nonce {
                Some("StaleNonce")
            } else if self.round - p.round >= self.c.limits.retention {
                Some("TTLExpired")
            } else {
                None
            };
            if let Some(reason) = reason {
                removed.push((id.clone(), reason));
            }
        }
        for (id, reason) in removed {
            self.reject(&id, reason);
            self.note(None, self.cutoff, Some(&id), None, "Rejected", reason);
            block.rejected.push(Rejected {
                tx_id: id,
                reason: reason.into(),
            });
        }
    }
    fn ordered(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.state.pending.keys().cloned().collect();
        match &self.c.builder {
            Builder::Fixed { priority } => {
                let rank: BTreeMap<_, _> =
                    priority.iter().enumerate().map(|(i, id)| (id, i)).collect();
                ids.retain(|id| rank.contains_key(id));
                ids.sort_by_key(|id| rank[id]);
            }
            mode => ids.sort_by(|a, b| {
                let ka = &self.state.known[a];
                let kb = &self.state.known[b];
                let pa = &self.state.pending[a];
                let pb = &self.state.pending[b];
                let price = if matches!(mode, Builder::FeePriority) {
                    kb.transaction.price.cmp(&ka.transaction.price)
                } else {
                    std::cmp::Ordering::Equal
                };
                price.then((pa.arrival, pa.event_id, a).cmp(&(pb.arrival, pb.event_id, b)))
            }),
        }
        ids
    }
    fn build(&mut self) -> Result<Block, String> {
        if self.c.bundles.is_some() {
            return self.build_candidates();
        }
        let mut block = engine::begin_block(self.s, self.parent)?;
        loop {
            self.prune(&mut block);
            if self.state.pending.is_empty() {
                break;
            }
            if self.metrics.attempts >= self.c.limits.inclusion_attempts {
                self.note(
                    None,
                    self.cutoff,
                    None,
                    None,
                    "InclusionLimit",
                    "remaining entries retained",
                );
                break;
            }
            if let Builder::Fixed { priority } = &self.c.builder {
                let unlisted: Vec<_> = self
                    .state
                    .pending
                    .keys()
                    .filter(|id| !priority.contains(id))
                    .cloned()
                    .collect();
                for id in unlisted {
                    let k = self.state.known.get_mut(&id).unwrap();
                    let waiting = Status::Waiting {
                        reason: "not in Fixed priority list".into(),
                    };
                    if k.status != waiting {
                        k.status = waiting;
                        self.note(
                            None,
                            self.cutoff,
                            Some(&id),
                            None,
                            "Waiting",
                            "not in Fixed priority list",
                        );
                    }
                }
            }
            let mut chosen = None;
            for id in self.ordered() {
                if self.metrics.checks >= self.c.limits.eligibility_checks {
                    self.note(
                        None,
                        self.cutoff,
                        None,
                        None,
                        "EligibilityLimit",
                        "remaining entries retained",
                    );
                    break;
                }
                self.metrics.checks += 1;
                let k = self.state.known.get_mut(&id).unwrap();
                if let Err(reason) = engine::inclusion(
                    self.s,
                    &block.state,
                    &k.transaction,
                    self.s.chain.block_limit.checked_sub(block.used)?,
                ) {
                    let waiting = Status::Waiting {
                        reason: reason.clone(),
                    };
                    if k.status != waiting {
                        k.status = waiting;
                        self.note(None, self.cutoff, Some(&id), None, "Waiting", &reason);
                    }
                } else {
                    chosen = Some(id);
                    break;
                }
            }
            let Some(id) = chosen else {
                break;
            };
            self.metrics.attempts += 1;
            let receipt =
                engine::execute(self.s, &mut block.state, &self.state.known[&id].transaction)?;
            block.used = block.used.checked_add(receipt.used)?;
            self.state.pending.remove(&id);
            self.state.known.get_mut(&id).unwrap().status = Status::Included {
                block: block.number,
                outcome: receipt.status.clone(),
            };
            self.note(
                None,
                self.cutoff,
                Some(&id),
                None,
                "Included",
                &format!(
                    "{}; used {}; fee {}",
                    receipt.status, receipt.used, receipt.fee
                ),
            );
            block.receipts.push(receipt);
        }
        block.state.invariant(self.s)?;
        Ok(block)
    }
}

pub(crate) fn advance(
    s: &Scenario,
    parent: &World,
    previous: &State,
    rng: &mut crate::run::Streams,
) -> Result<(Block, State), String> {
    advance_with_rng_check(s, parent, previous, rng, |_| Ok(()), &|_| Ok(()))
}
#[cfg(test)]
fn advance_with_check(
    s: &Scenario,
    parent: &World,
    previous: &State,
    check: impl FnOnce(&Block) -> Result<(), String>,
    probe_check: &dyn Fn(&World) -> Result<(), String>,
) -> Result<(Block, State), String> {
    advance_with_rng_check(
        s,
        parent,
        previous,
        &mut crate::run::Streams::new(s.seed),
        check,
        probe_check,
    )
}
fn advance_with_rng_check(
    s: &Scenario,
    parent: &World,
    previous: &State,
    rng: &mut crate::run::Streams,
    check: impl FnOnce(&Block) -> Result<(), String>,
    probe_check: &dyn Fn(&World) -> Result<(), String>,
) -> Result<(Block, State), String> {
    let mut working_rng = rng.clone();
    let c = s.scheduler.as_ref().ok_or("missing scheduler config")?;
    let round = parent
        .block
        .to_string()
        .parse::<usize>()
        .map_err(|_| "round overflow")?
        + 1;
    let cutoff = parent.timestamp.checked_add(s.chain.interval)?;
    let mut r = Round {
        s,
        c,
        parent,
        state: previous.clone(),
        trace: vec![],
        metrics: Metrics::default(),
        probes: vec![],
        search: backrun::Report::default(),
        liquidators: policy::liquidator::Report::default(),
        baselines: vec![],
        system_preview: None,
        probe_check,
        round,
        cutoff,
        intents: BTreeMap::new(),
    };
    r.expire_visible();
    let mut emissions: Vec<_> = c
        .emissions
        .iter()
        .enumerate()
        .map(|(i, e)| {
            (
                e.at,
                &e.producer_id,
                e.producer_sequence,
                EventKind::Emission { index: i },
                e.transaction.id.as_str(),
                e.transaction.agent.as_str(),
            )
        })
        .collect();
    if let Some(cfg) = &c.bundles {
        emissions.extend(cfg.emissions.iter().enumerate().map(|(i, e)| {
            (
                e.at,
                &e.producer_id,
                e.producer_sequence,
                EventKind::BundleEmission { index: i },
                e.bundle.id.as_str(),
                e.bundle.agent.as_str(),
            )
        }));
    }
    emissions.sort_by_key(|e| (e.0, e.1, e.2));
    while let Some(e) = emissions.get(r.state.emission_cursor) {
        if e.0 > cutoff {
            break;
        }
        r.enqueue(e.0, e.3.clone(), parent.timestamp, Some(e.4), Some(e.5))?;
        r.state.emission_cursor += 1;
    }
    let mut decisions = policy::decisions_unless(s, parent, &|agent, account, nonce| {
        previous.busy(agent, account, nonce)
    })?;
    let periodic: BTreeSet<String> = decisions
        .iter()
        .map(|d| d.agent.clone())
        .chain(s.baseline_policies.keys().cloned())
        .chain(
            c.liquidator
                .iter()
                .flat_map(|cfg| cfg.policies.keys().cloned()),
        )
        .collect();
    for agent in periodic {
        if s.baseline_policies.contains_key(&agent) {
            r.baseline_decision(&agent, &mut working_rng)?;
        } else if c
            .liquidator
            .as_ref()
            .is_some_and(|cfg| cfg.policies.contains_key(&agent))
        {
            r.liquidator_decision(&agent)?;
        } else if let Some(tx) = decisions
            .iter()
            .find(|d| d.agent == agent)
            .and_then(|d| d.transaction.clone())
        {
            *r.intents.entry(agent.clone()).or_default() += 1;
            r.submit(
                tx,
                c.agents[&agent].outgoing.clone(),
                0,
                parent.timestamp,
                None,
            )?;
        }
    }
    loop {
        let next = r
            .state
            .queue
            .iter()
            .enumerate()
            .min_by_key(|(_, e)| (e.due_at.max(parent.timestamp), e.id))
            .map(|(i, e)| (i, e.due_at));
        let Some((index, due)) = next else {
            break;
        };
        if due > cutoff {
            break;
        }
        if r.metrics.events >= c.limits.events_per_round {
            r.note(
                None,
                cutoff,
                None,
                None,
                "EventLimit",
                "due events deferred; original due_at retained",
            );
            break;
        }
        let event = r.state.queue.remove(index);
        let at = event.due_at.max(parent.timestamp);
        r.metrics.events += 1;
        match &event.kind {
            EventKind::BackrunReaction {
                tx_id,
                agent,
                depth,
            } => r.backrun_reaction(tx_id, agent, *depth, &event, at)?,
            EventKind::BundleEmission { index } => {
                let e = &c.bundles.as_ref().ok_or("missing bundle config")?.emissions[*index];
                r.submit_bundle(&e.bundle, &e.delivery, &event, at, false)?;
            }
            EventKind::BundleBuilderDelivery { bundle_id } => {
                r.admit_bundle(bundle_id, &event, at)?
            }
            EventKind::BundleAgentDelivery { bundle_id, agent } => {
                r.deliver_bundle(bundle_id, agent, &event, at)?
            }
            EventKind::Emission { index } => {
                let e = &c.emissions[*index];
                r.submit(
                    e.transaction.clone(),
                    e.delivery.clone(),
                    0,
                    at,
                    Some(&event),
                )?;
            }
            EventKind::BuilderDelivery { tx_id } => r.admission(tx_id, &event, at)?,
            EventKind::AgentDelivery { tx_id, agent } => r.visible(tx_id, agent, &event, at)?,
            EventKind::Reaction {
                tx_id,
                agent,
                depth,
            } => r.reaction(tx_id, agent, *depth, &event, at, &mut decisions)?,
        }
    }
    let mut block = r.build()?;
    block.decisions = decisions;
    // Nothing above mutates the parent world, prior queues, policy memory or RNG.
    check(&block)?;
    for k in r.state.known.values_mut() {
        k.committed_status = k.status.clone();
    }
    for k in r.state.bundles.values_mut() {
        k.committed_status = k.status.clone();
    }
    block.scheduler = Some(Report {
        baselines: r.baselines,
        search: c.backrun.as_ref().map(|_| r.search),
        liquidators: c.liquidator.as_ref().map(|_| r.liquidators),
        probes: r.probes,
        trace: r.trace,
        metrics: r.metrics,
        state: r.state.clone(),
    });
    *rng = working_rng;
    Ok((block, r.state))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn injected_failure_after_real_execution_keeps_parent_and_scheduler_intact() {
        let s = Scenario::parse(include_bytes!(
            "../../../../scenarios/demos/sc-scheduler-public.json"
        ))
        .unwrap();
        let parent = World::genesis(&s).unwrap();
        let state = State::default();
        let before = (parent.clone(), state.clone());
        let result = advance_with_check(
            &s,
            &parent,
            &state,
            |b| {
                assert_eq!(b.receipts.len(), 2);
                assert_eq!(b.state.balance("alice", "Q"), 10016.into());
                Err("injected invariant fault after all deliveries and executions".into())
            },
            &|_| Ok(()),
        );
        assert!(result.is_err());
        assert_eq!((parent, state), before);
    }
    #[test]
    fn admission_status_is_not_exposed_to_agent_until_committed_boundary() {
        let s = Scenario::parse(include_bytes!(
            "../../../../scenarios/demos/sc-scheduler-public.json"
        ))
        .unwrap();
        let c = s.scheduler.as_ref().unwrap();
        let parent = World::genesis(&s).unwrap();
        let mut r = Round {
            s: &s,
            c,
            parent: &parent,
            state: State::default(),
            trace: vec![],
            metrics: Metrics::default(),
            probes: vec![],
            search: backrun::Report::default(),
            liquidators: policy::liquidator::Report::default(),
            baselines: vec![],
            system_preview: None,
            probe_check: &|_| Ok(()),
            round: 1,
            cutoff: 10.into(),
            intents: BTreeMap::new(),
        };
        let tx = c.emissions[0].transaction.clone();
        r.submit(tx, c.emissions[0].delivery.clone(), 0, 7.into(), None)
            .unwrap();
        let event = r
            .state
            .queue
            .iter()
            .find(|e| matches!(e.kind, EventKind::BuilderDelivery { .. }))
            .unwrap()
            .clone();
        r.admission("victim", &event, 9.into()).unwrap();
        assert_eq!(r.state.known["victim"].status, Status::Pending);
        let obs = r
            .state
            .observation(&s, &parent, "recipient", "bob")
            .unwrap();
        assert_eq!(obs.own_outbox[0].status, Status::InFlight);
        assert!(obs.messages.is_empty());
        assert_eq!(obs.market.block, Amount::ZERO);
        assert_eq!(obs.market.balances["Q"], 1000.into());
    }
}

#[cfg(test)]
mod bundle_fault_tests {
    use super::*;
    #[test]
    fn model_error_in_bundle_probe_cannot_commit_prior_candidate_or_queue() {
        let mut s = Scenario::parse(include_bytes!(
            "../../../../scenarios/demos/sc-bundle-fee.json"
        ))
        .unwrap();
        s.chain.block_limit = 1000.into();
        let parent = World::genesis(&s).unwrap();
        let previous = State::default();
        let before = (parent.clone(), previous.clone());
        let checked = std::cell::Cell::new(false);
        let fault = |w: &World| {
            checked.set(true);
            // Ordinary tx already selected in the working prefix; first group member executed in its probe.
            assert_eq!(w.balance("bob", "Q"), 21.into());
            assert_eq!(w.balance("builder", "Q"), 460.into());
            Err("injected ModelError inside bundle probe after successful prefix".into())
        };
        assert!(advance_with_check(&s, &parent, &previous, |_| Ok(()), &fault).is_err());
        assert!(checked.get());
        assert_eq!((parent, previous), before);
    }
}

#[cfg(test)]
mod search_fault_tests {
    use super::*;
    #[test]
    fn model_error_in_prefix_response_or_payment_discards_entire_search_round() {
        for fault_at in [1, 2, 4] {
            let s = Scenario::parse(include_bytes!(
                "../../../../scenarios/demos/sc-backrun-public.json"
            ))
            .unwrap();
            let parent = World::genesis(&s).unwrap();
            let previous = State::default();
            let before = (parent.clone(), previous.clone());
            let calls = std::cell::Cell::new(0);
            let fault = |_w: &World| {
                calls.set(calls.get() + 1);
                if calls.get() == fault_at {
                    Err("injected search ModelError".into())
                } else {
                    Ok(())
                }
            };
            assert!(advance_with_check(&s, &parent, &previous, |_| Ok(()), &fault).is_err());
            assert_eq!(calls.get(), fault_at);
            assert_eq!((parent, previous), before);
        }
    }
}

#[cfg(test)]
mod liquidator_fault_tests {
    use super::*;
    #[test]
    fn model_error_after_liquidation_and_writeoff_discards_entire_round() {
        let s = Scenario::parse(include_bytes!(
            "../../../../scenarios/demos/sc-liquidator-own.json"
        ))
        .unwrap();
        let mut run = crate::Run::new(s).unwrap();
        run.step().unwrap();
        run.step().unwrap();
        let parent = &run.state;
        let previous = run.scheduler.as_ref().unwrap();
        let before = run.clone();
        let checks = std::cell::Cell::new(0);
        let fault = |w: &World| {
            checks.set(checks.get() + 1);
            if w.lending["credit"].debt.is_zero() {
                assert_eq!(w.lending["credit"].losses[0].amount, 20.into());
                assert_eq!(w.balance("carol", "Q"), 1024.into());
                return Err("injected ModelError after liquidation write-off and sale".into());
            }
            Ok(())
        };
        assert!(advance_with_check(&run.scenario, parent, previous, |_| Ok(()), &fault).is_err());
        assert_eq!(checks.get(), 3);
        assert_eq!(run, before);
        run.step().unwrap();
        assert_eq!(run.state.balance("carol", "Q"), 1024.into());
    }
}

#[cfg(test)]
mod baseline_fault_tests {
    use super::*;
    #[test]
    fn failure_after_random_execution_or_probe_rolls_back_rng_world_and_queue() {
        let s = Scenario::parse(include_bytes!(
            "../../../../scenarios/demos/sc-agent-random.json"
        ))
        .unwrap();
        let parent = World::genesis(&s).unwrap();
        let previous = State::default();
        let mut rng = crate::run::Streams::new(s.seed);
        let old = (parent.clone(), previous.clone(), rng.clone());
        let result = advance_with_rng_check(
            &s,
            &parent,
            &previous,
            &mut rng,
            |b| {
                assert_eq!(b.state.balance("alice", "Q"), 865.into());
                assert_eq!(b.receipts.len(), 1);
                Err("fault after committed candidate and random decisions".into())
            },
            &|_| Ok(()),
        );
        assert!(result.is_err());
        assert_eq!((parent.clone(), previous.clone(), rng.clone()), old);
        let result = advance_with_rng_check(&s, &parent, &previous, &mut rng, |_| Ok(()), &|w| {
            assert_eq!(w.balance("alice", "Q"), 865.into());
            Err("fault in preflight".into())
        });
        assert!(result.is_err());
        assert_eq!((parent.clone(), previous.clone(), rng.clone()), old);
        let (block, _) = advance(&s, &parent, &previous, &mut rng).unwrap();
        assert_eq!(block.state.balance("alice", "Q"), 865.into());
        assert_eq!(rng.position("agent/trader"), 4.into());
    }
    #[test]
    fn physical_stream_exhaustion_never_commits_a_partial_selection() {
        let s = Scenario::parse(include_bytes!(
            "../../../../scenarios/demos/sc-agent-random.json"
        ))
        .unwrap();
        let parent = World::genesis(&s).unwrap();
        let previous = State::default();
        let mut raw = serde_json::to_value(crate::run::Streams::new(s.seed)).unwrap();
        raw["positions"]["agent/trader"] = serde_json::json!(((1u128 << 68) - 4).to_string());
        let mut rng: crate::run::Streams = serde_json::from_value(raw).unwrap();
        let old = rng.clone();
        let error = advance(&s, &parent, &previous, &mut rng).unwrap_err();
        assert!(error.contains("RNG stream exhausted"), "{error}");
        assert_eq!(rng, old);
    }
}
