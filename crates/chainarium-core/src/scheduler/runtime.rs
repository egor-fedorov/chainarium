use super::*;
use crate::{Block, Rejected, engine, policy};

struct Round<'a> {
    s: &'a Scenario,
    c: &'a Config,
    parent: &'a World,
    state: State,
    trace: Vec<Trace>,
    metrics: Metrics,
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
        if let Some(k) = self.state.known.get(&id) {
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
        if self.state.known.len() >= self.c.limits.known {
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
                (self.state.pending.len() >= self.c.limits.pending).then(|| "MempoolLimit".into())
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
        let list = self.state.visible.entry(agent.into()).or_default();
        let evicted = if list.len() >= self.c.limits.visible {
            Some(list.remove(0).transaction.id)
        } else {
            None
        };
        list.push(message);
        if let Some(old) = evicted {
            self.note(
                Some(event),
                at,
                Some(&old),
                Some(agent),
                "VisibleLimit",
                "oldest delivered message evicted",
            );
        }
        self.note(
            Some(event),
            at,
            Some(id),
            Some(agent),
            "Visible",
            "authorized message delivery",
        );
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
) -> Result<(Block, State), String> {
    advance_with_check(s, parent, previous, |_| Ok(()))
}
fn advance_with_check(
    s: &Scenario,
    parent: &World,
    previous: &State,
    check: impl FnOnce(&Block) -> Result<(), String>,
) -> Result<(Block, State), String> {
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
        round,
        cutoff,
        intents: BTreeMap::new(),
    };
    r.expire_visible();
    let mut emissions: Vec<_> = c.emissions.iter().enumerate().collect();
    emissions.sort_by_key(|(_, e)| (e.at, &e.producer_id, e.producer_sequence));
    while let Some((index, e)) = emissions.get(r.state.emission_cursor) {
        if e.at > cutoff {
            break;
        }
        r.enqueue(
            e.at,
            EventKind::Emission { index: *index },
            parent.timestamp,
            Some(&e.transaction.id),
            Some(&e.transaction.agent),
        )?;
        r.state.emission_cursor += 1;
    }
    let mut decisions = policy::decisions_unless(s, parent, &|agent, account, nonce| {
        previous.known.values().any(|k| {
            k.transaction.agent == agent
                && k.transaction.sender == account
                && k.transaction.nonce == nonce
                && k.status.unfinished()
        })
    })?;
    for d in &decisions {
        if let Some(tx) = &d.transaction {
            *r.intents.entry(d.agent.clone()).or_default() += 1;
            r.submit(
                tx.clone(),
                c.agents[&d.agent].outgoing.clone(),
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
    block.scheduler = Some(Report {
        trace: r.trace,
        metrics: r.metrics,
        state: r.state.clone(),
    });
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
        let result = advance_with_check(&s, &parent, &state, |b| {
            assert_eq!(b.receipts.len(), 2);
            assert_eq!(b.state.balance("alice", "Q"), 10016.into());
            Err("injected invariant fault after all deliveries and executions".into())
        });
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
