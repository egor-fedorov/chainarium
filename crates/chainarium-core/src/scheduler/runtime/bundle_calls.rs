use super::*;
use crate::scheduler::bundles as g;

struct Evaluation {
    state: World,
    used: Amount,
    trace: usize,
}
impl Round<'_> {
    fn bundle_note(
        &mut self,
        event: Option<&Queued>,
        at: Amount,
        id: &str,
        action: &str,
        detail: &str,
    ) {
        self.note(
            event,
            at,
            None,
            None,
            action,
            &format!("bundle:{id}; {detail}"),
        );
    }
    fn reject_bundle(&mut self, id: &str, reason: &str) {
        self.state.bundle_pending.remove(id);
        if let Some(k) = self.state.bundles.get_mut(id) {
            k.status = Status::Rejected {
                reason: reason.into(),
            };
        }
    }
    fn resolve_bundle(&self, b: &g::Bundle, at: Amount) -> Result<g::Resolved, String> {
        let cfg = self.c.bundles.as_ref().ok_or("bundle config")?;
        if b.id.is_empty()
            || !self.s.agents.contains_key(&b.agent)
            || b.members.is_empty()
            || b.members.len() > cfg.limits.members
            || b.min_block.is_zero()
            || b.max_block < b.min_block
            || b.deadline < at
        {
            return Err("InvalidBundleEnvelope".into());
        }
        let mut members = vec![];
        let mut ids = BTreeSet::new();
        for member in &b.members {
            let tx = match member {
                g::Member::Own { transaction } => {
                    if transaction.agent != b.agent {
                        return Err("ForeignBundleAuthor".into());
                    }
                    if transaction.id.starts_with("policy:")
                        || transaction.id.starts_with("reactive:")
                    {
                        return Err("ReservedMemberTxId".into());
                    }
                    transaction.as_ref().clone()
                }
                g::Member::VisibleTxId { tx_id } => self
                    .state
                    .visible_transaction(&b.agent, tx_id)
                    .ok_or("UnknownVisibleTxId")?,
            };
            self.s.validate_tx(&tx)?;
            if !ids.insert(tx.id.clone()) {
                return Err("DuplicateMemberTxId".into());
            }
            if tx.limit > self.s.chain.block_limit {
                return Err("UnfitMemberDeclaredLimit".into());
            }
            if let Some(k) = self.state.known.get(&tx.id) {
                if k.transaction != tx {
                    return Err("ConflictingMemberTxId".into());
                }
                if matches!(k.status, Status::Included { .. } | Status::Rejected { .. }) {
                    return Err("TerminalMemberTxId".into());
                }
            }
            members.push(tx);
        }
        if b.allowed_reverts.iter().any(|id| !ids.contains(id)) {
            return Err("UnknownAllowedRevert".into());
        }
        match (b.bid.is_zero(), &b.payment_tx_id) {
            (true, None) => {}
            (false, Some(id)) => {
                let payment = members.last().ok_or("empty bundle")?;
                if &payment.id != id
                    || payment.agent != b.agent
                    || payment.sender == self.s.chain.fee_account
                    || b.allowed_reverts.contains(id)
                {
                    return Err("InvalidBidPayerOrPosition".into());
                }
                if payment.program
                    != vec![Call::Transfer {
                        to: self.s.chain.fee_account.clone(),
                        asset: self.s.chain.fee_asset.clone(),
                        amount: b.bid,
                    }]
                {
                    return Err("UnbackedBidDeclaration".into());
                }
            }
            _ => return Err("InvalidBidPayment".into()),
        }
        let resolved = g::Resolved {
            id: b.id.clone(),
            agent: b.agent.clone(),
            members,
            min_block: b.min_block,
            max_block: b.max_block,
            deadline: b.deadline,
            allowed_reverts: b.allowed_reverts.clone(),
            bid: b.bid,
            payment_tx_id: b.payment_tx_id.clone(),
        };
        if serde_json::to_vec(&resolved)
            .map_err(|e| e.to_string())?
            .len()
            > cfg.limits.bytes
        {
            return Err("BundleSizeLimit".into());
        }
        let new = resolved
            .members
            .iter()
            .filter(|t| !self.state.known.contains_key(&t.id))
            .count();
        if self.state.known.len() + new > self.c.limits.known {
            return Err("KnownTransactionsLimit".into());
        }
        Ok(resolved)
    }
    pub(super) fn submit_bundle(
        &mut self,
        b: &g::Bundle,
        delivery: &Delivery,
        event: &Queued,
        at: Amount,
    ) -> Result<(), String> {
        let fingerprint = crate::run::digest(&serde_json::to_vec(b).map_err(|e| e.to_string())?);
        if let Some(k) = self.state.bundles.get(&b.id) {
            let action = if k.fingerprint == fingerprint {
                "DuplicateBundle"
            } else {
                "ConflictingBundleId"
            };
            self.bundle_note(
                Some(event),
                at,
                &b.id,
                action,
                "first group retained; no retry",
            );
            return Ok(());
        }
        if self.state.bundles.len() >= self.c.bundles.as_ref().ok_or("bundle config")?.limits.known
        {
            self.bundle_note(
                Some(event),
                at,
                &b.id,
                "KnownBundlesLimit",
                "new group dropped",
            );
            return Ok(());
        }
        let resolved = self.resolve_bundle(b, at);
        self.state.bundles.insert(
            b.id.clone(),
            g::KnownBundle {
                fingerprint,
                agent: b.agent.clone(),
                submitted_at: at,
                delivery: delivery.clone(),
                resolved: resolved.as_ref().ok().cloned(),
                status: Status::InFlight,
                committed_status: Status::InFlight,
            },
        );
        self.bundle_note(
            Some(event),
            at,
            &b.id,
            "BundleSent",
            &format!("{:?}", delivery.channel),
        );
        let resolved = match resolved {
            Ok(v) => v,
            Err(reason) => {
                self.reject_bundle(&b.id, &reason);
                self.bundle_note(Some(event), at, &b.id, "BundleRejected", &reason);
                return Ok(());
            }
        };
        // All validation and capacity checks succeeded; insert the complete member set atomically.
        for tx in &resolved.members {
            self.state
                .known
                .entry(tx.id.clone())
                .or_insert_with(|| Known {
                    standalone: false,
                    transaction: tx.clone(),
                    delivery: delivery.clone(),
                    submitted_at: at,
                    depth: 0,
                    status: Status::BundleMember,
                    committed_status: Status::BundleMember,
                });
        }
        if !self.enqueue(
            at.checked_add(delivery.builder_delay)?,
            EventKind::BundleBuilderDelivery {
                bundle_id: b.id.clone(),
            },
            at,
            None,
            None,
        )? {
            self.reject_bundle(&b.id, "QueueLimit");
        }
        for (agent, n) in &self.c.agents {
            let visible = match &delivery.channel {
                Channel::Public => n.observe_public,
                Channel::Private { recipients } => recipients.contains(agent),
            };
            if visible {
                self.enqueue(
                    at.checked_add(n.observation_delay)?,
                    EventKind::BundleAgentDelivery {
                        bundle_id: b.id.clone(),
                        agent: agent.clone(),
                    },
                    at,
                    None,
                    Some(agent),
                )?;
            }
        }
        Ok(())
    }
    fn terminal_member(&self, b: &g::Resolved, w: &World, at: Amount) -> Option<&'static str> {
        for tx in &b.members {
            if matches!(self.state.known[&tx.id].status, Status::Included { .. }) {
                return Some("MemberAlreadyIncluded");
            }
            if matches!(self.state.known[&tx.id].status, Status::Rejected { .. }) {
                return Some("TerminalStandaloneMember");
            }
            if tx.nonce < w.accounts[&tx.sender].nonce {
                return Some("StaleMemberNonce");
            }
            if tx.deadline < at {
                return Some("ExpiredMemberDeadline");
            }
        }
        None
    }
    pub(super) fn admit_bundle(
        &mut self,
        id: &str,
        event: &Queued,
        at: Amount,
    ) -> Result<(), String> {
        let cfg = self.c.bundles.as_ref().ok_or("bundle config")?;
        let b = self.state.bundles[id]
            .resolved
            .as_ref()
            .ok_or("unresolved delivery")?;
        let reason = if b.deadline < at {
            Some("ExpiredBundleDeadline")
        } else if b.max_block < Amount::from(self.round as u64) {
            Some("ExpiredTargetRange")
        } else if self.state.bundle_pending.len() >= cfg.limits.pending {
            Some("BundleMempoolLimit")
        } else if self.state.bundle_pending.len() + self.state.pending.len()
            >= self.c.limits.pending
        {
            Some("MempoolLimit")
        } else {
            self.terminal_member(b, self.parent, at)
        };
        if let Some(reason) = reason {
            self.reject_bundle(id, reason);
            self.bundle_note(Some(event), at, id, "BundleRejected", reason);
            return Ok(());
        }
        self.state.bundle_pending.insert(
            id.into(),
            Pending {
                arrival: at,
                event_id: event.id,
                round: self.round,
            },
        );
        self.state.bundles.get_mut(id).unwrap().status = Status::Pending;
        self.bundle_note(
            Some(event),
            at,
            id,
            "BundleAdmitted",
            "no native reserve, fee or nonce",
        );
        Ok(())
    }
    pub(super) fn deliver_bundle(
        &mut self,
        id: &str,
        agent: &str,
        event: &Queued,
        at: Amount,
    ) -> Result<(), String> {
        let k = &self.state.bundles[id];
        let message = g::BundleMessage {
            bundle: k.resolved.clone().ok_or("unresolved visible bundle")?,
            channel: k.delivery.channel.clone(),
            submitted_at: k.submitted_at,
            delivered_at: at,
            event_id: event.id,
            round: self.round,
        };
        self.state
            .bundle_visible
            .entry(agent.into())
            .or_default()
            .push(message);
        self.trim_visible(agent, event, at);
        self.note(
            Some(event),
            at,
            None,
            Some(agent),
            "BundleVisible",
            &format!("bundle:{id}; authorized group; no implicit member messages"),
        );
        Ok(())
    }
    pub(super) fn trim_visible(&mut self, agent: &str, event: &Queued, at: Amount) {
        while self.state.visible.get(agent).map_or(0, Vec::len)
            + self.state.bundle_visible.get(agent).map_or(0, Vec::len)
            > self.c.limits.visible
        {
            let native = self
                .state
                .visible
                .get(agent)
                .and_then(|v| v.first())
                .map(|m| (m.delivered_at, m.event_id));
            let bundle = self
                .state
                .bundle_visible
                .get(agent)
                .and_then(|v| v.first())
                .map(|m| (m.delivered_at, m.event_id));
            if native.is_some() && (bundle.is_none() || native < bundle) {
                let old = self.state.visible.get_mut(agent).unwrap().remove(0);
                self.note(
                    Some(event),
                    at,
                    Some(&old.transaction.id),
                    Some(agent),
                    "VisibleLimit",
                    "oldest delivered message evicted",
                );
            } else {
                let old = self.state.bundle_visible.get_mut(agent).unwrap().remove(0);
                self.note(
                    Some(event),
                    at,
                    None,
                    Some(agent),
                    "VisibleLimit",
                    &format!("bundle:{}; oldest delivered message evicted", old.bundle.id),
                );
            }
        }
    }
    pub(super) fn expire_bundle_visible(&mut self) {
        let mut removed = vec![];
        for (agent, messages) in &mut self.state.bundle_visible {
            messages.retain(|m| {
                let keep = self.round - m.round < self.c.limits.retention;
                if !keep {
                    removed.push((agent.clone(), m.bundle.id.clone()));
                }
                keep
            });
        }
        for (agent, id) in removed {
            self.note(
                None,
                self.parent.timestamp,
                None,
                Some(&agent),
                "TTLExpired",
                &format!("bundle:{id}; visible message retention"),
            );
        }
    }
    fn prune_bundles(&mut self, block: &Block) {
        let mut removed = vec![];
        for (id, pending) in &self.state.bundle_pending {
            let b = self.state.bundles[id].resolved.as_ref().unwrap();
            let reason = if b.deadline < block.timestamp {
                Some("ExpiredBundleDeadline")
            } else if b.max_block < block.number {
                Some("ExpiredTargetRange")
            } else if self.round - pending.round >= self.c.limits.retention {
                Some("TTLExpired")
            } else {
                self.terminal_member(b, &block.state, block.timestamp)
            };
            if let Some(reason) = reason {
                removed.push((id.clone(), reason));
            }
        }
        for (id, reason) in removed {
            self.reject_bundle(&id, reason);
            self.bundle_note(None, self.cutoff, &id, "BundleRejected", reason);
        }
    }
    fn fifo_key(&self, id: &g::CandidateId) -> (Amount, Amount, g::CandidateId) {
        let p = match id {
            g::CandidateId::Transaction { id } => &self.state.pending[id],
            g::CandidateId::Bundle { id } => &self.state.bundle_pending[id],
        };
        (p.arrival, p.event_id, id.clone())
    }
    fn price(&self, id: &g::CandidateId) -> Amount {
        match id {
            g::CandidateId::Transaction { id } => self.state.known[id].transaction.price,
            g::CandidateId::Bundle { id } => self.state.bundles[id]
                .resolved
                .as_ref()
                .unwrap()
                .members
                .iter()
                .map(|t| t.price)
                .min()
                .unwrap_or_default(),
        }
    }
    fn waiting(&mut self, id: &g::CandidateId, reason: &str) {
        let status = match id {
            g::CandidateId::Transaction { id } => &mut self.state.known.get_mut(id).unwrap().status,
            g::CandidateId::Bundle { id } => &mut self.state.bundles.get_mut(id).unwrap().status,
        };
        let waiting = Status::Waiting {
            reason: reason.into(),
        };
        if *status != waiting {
            *status = waiting;
            self.note(
                None,
                self.cutoff,
                None,
                None,
                "CandidateWaiting",
                &format!("{}; {reason}", id.label()),
            );
        }
    }
    fn ordered_candidates(&mut self) -> Vec<g::CandidateId> {
        let all: Vec<_> = self
            .state
            .pending
            .keys()
            .map(|id| g::CandidateId::Transaction { id: id.clone() })
            .chain(
                self.state
                    .bundle_pending
                    .keys()
                    .map(|id| g::CandidateId::Bundle { id: id.clone() }),
            )
            .collect();
        let priority = match &self.c.builder {
            Builder::Fixed { priority } => Some(
                priority
                    .iter()
                    .map(|id| g::CandidateId::Transaction { id: id.clone() })
                    .collect::<Vec<_>>(),
            ),
            Builder::FixedItems { priority } => Some(priority.clone()),
            _ => None,
        };
        if let Some(priority) = priority {
            let rank: BTreeMap<_, _> = priority
                .iter()
                .enumerate()
                .map(|(i, id)| (id.clone(), i))
                .collect();
            let mut chosen = vec![];
            for id in all {
                if rank.contains_key(&id) {
                    chosen.push(id);
                } else {
                    self.waiting(&id, "not in Fixed priority list");
                }
            }
            chosen.sort_by_key(|id| rank[id]);
            return chosen;
        }
        let mut sorted = all;
        sorted.sort_by(|a, b| {
            let fee = if matches!(self.c.builder, Builder::FeePriority) {
                self.price(b).cmp(&self.price(a))
            } else {
                std::cmp::Ordering::Equal
            };
            fee.then(self.fifo_key(a).cmp(&self.fifo_key(b)))
        });
        sorted
    }
    fn deferred_probe(&mut self, id: &g::CandidateId, mode: &str, reason: &str, budget: bool) {
        self.waiting(id, reason);
        self.probes.push(g::Probe::empty(id.clone(), mode, reason));
        if budget {
            self.metrics.truncated = true;
            self.note(
                None,
                self.cutoff,
                None,
                None,
                "ProbeBudget",
                &format!("{}; {reason}", id.label()),
            );
        }
    }
    fn evaluate(
        &mut self,
        id: &g::CandidateId,
        block: &Block,
    ) -> Result<Option<Evaluation>, String> {
        let cfg = &self.c.bundles.as_ref().ok_or("bundle config")?.limits;
        let group = match id {
            g::CandidateId::Bundle { id } => self.state.bundles[id].resolved.clone(),
            _ => None,
        };
        let trial = group.is_some() || matches!(self.c.builder, Builder::Greedy);
        let mode = if trial { "Probe" } else { "Direct" };
        if let Some(g) = &group
            && g.min_block > block.number
        {
            self.deferred_probe(id, mode, "TargetBlockWaiting", false);
            return Ok(None);
        }
        let txs = match &group {
            Some(g) => g.members.clone(),
            None => {
                let g::CandidateId::Transaction { id } = id else {
                    return Err("unresolved bundle candidate".into());
                };
                vec![self.state.known[id].transaction.clone()]
            }
        };
        if self.metrics.attempts + txs.len() > self.c.limits.inclusion_attempts {
            self.deferred_probe(id, mode, "InclusionLimit", true);
            return Ok(None);
        }
        if self.metrics.checks >= self.c.limits.eligibility_checks {
            self.deferred_probe(id, mode, "EligibilityLimit", true);
            return Ok(None);
        }
        if trial
            && (self.metrics.probes >= cfg.probes
                || self.metrics.probe_members + txs.len() > cfg.probe_members)
        {
            self.deferred_probe(id, mode, "ProbeLimit", true);
            return Ok(None);
        }
        if trial {
            self.metrics.probes += 1;
        }
        let mut w = block.state.clone();
        let before = w.balance(&self.s.chain.fee_account, &self.s.chain.fee_asset);
        let mut probe = g::Probe::empty(id.clone(), mode, "Executable");
        for tx in &txs {
            if self.metrics.checks >= self.c.limits.eligibility_checks {
                probe.outcome = "EligibilityLimit".into();
                self.metrics.truncated = true;
                break;
            }
            self.metrics.checks += 1;
            if trial {
                self.metrics.probe_members += 1;
            }
            let remaining = self
                .s
                .chain
                .block_limit
                .checked_sub(block.used)?
                .checked_sub(probe.used)?;
            if let Err(reason) = engine::inclusion(self.s, &w, tx, remaining) {
                probe.outcome = format!("Admission: {reason}");
                break;
            }
            let receipt = engine::execute(self.s, &mut w, tx)?;
            if trial {
                (self.probe_check)(&w)?;
            }
            probe.used = probe.used.checked_add(receipt.used)?;
            if trial {
                self.metrics.probe_used = self.metrics.probe_used.checked_add(receipt.used)?;
            }
            let allowed = group
                .as_ref()
                .is_none_or(|g| receipt.status == "Success" || g.allowed_reverts.contains(&tx.id));
            probe.receipts.push(receipt);
            if !allowed {
                probe.outcome = format!("DisallowedRevert: {}", tx.id);
                break;
            }
        }
        if probe.outcome == "Executable" {
            for receipt in &probe.receipts {
                if receipt.sender != self.s.chain.fee_account {
                    probe.paid_fees = probe.paid_fees.checked_add(receipt.fee)?;
                }
            }
            probe.bid = group.as_ref().map_or(Amount::ZERO, |g| g.bid);
            probe.score = probe.paid_fees.checked_add(probe.bid)?;
            let after = w.balance(&self.s.chain.fee_account, &self.s.chain.fee_asset);
            probe.builder_delta = crate::clmm::SignedAmount {
                negative: after < before,
                magnitude: if after < before {
                    before.checked_sub(after)?
                } else {
                    after.checked_sub(before)?
                },
            };
            if matches!(self.c.builder, Builder::Greedy) && probe.builder_delta.negative {
                probe.outcome = "NegativeBuilderDelta".into();
            }
        }
        let success = probe.outcome == "Executable";
        if !success {
            self.waiting(id, &probe.outcome);
        }
        let used = probe.used;
        let index = self.probes.len();
        self.probes.push(probe);
        Ok(success.then_some(Evaluation {
            state: w,
            used,
            trace: index,
        }))
    }
    fn commit_candidate(
        &mut self,
        evaluation: Evaluation,
        block: &mut Block,
    ) -> Result<(), String> {
        let probe = &mut self.probes[evaluation.trace];
        probe.selected = true;
        let candidate = probe.candidate.clone();
        let receipts = probe.receipts.clone();
        block.state = evaluation.state;
        block.used = block.used.checked_add(evaluation.used)?;
        self.metrics.attempts += receipts.len();
        for receipt in &receipts {
            self.state.pending.remove(&receipt.tx_id);
            self.state
                .known
                .get_mut(&receipt.tx_id)
                .ok_or("missing selected TxId")?
                .status = Status::Included {
                block: block.number,
                outcome: receipt.status.clone(),
            };
        }
        if let g::CandidateId::Bundle { id } = &candidate {
            self.state.bundle_pending.remove(id);
            self.state.bundles.get_mut(id).unwrap().status = Status::Included {
                block: block.number,
                outcome: "BundleSelected".into(),
            };
        }
        self.note(
            None,
            self.cutoff,
            None,
            None,
            "CandidateIncluded",
            &format!(
                "{}; {} consecutive receipts; used {}",
                candidate.label(),
                receipts.len(),
                evaluation.used
            ),
        );
        block.receipts.extend(receipts);
        Ok(())
    }
    pub(super) fn build_candidates(&mut self) -> Result<Block, String> {
        let mut block = engine::begin_block(self.s, self.parent)?;
        loop {
            self.prune(&mut block);
            self.prune_bundles(&block);
            if self.state.pending.is_empty() && self.state.bundle_pending.is_empty() {
                break;
            }
            if self.metrics.attempts >= self.c.limits.inclusion_attempts
                || self.metrics.checks >= self.c.limits.eligibility_checks
            {
                self.metrics.truncated = true;
                self.note(
                    None,
                    self.cutoff,
                    None,
                    None,
                    "SelectionLimit",
                    "pending candidates retained",
                );
                break;
            }
            let mut best: Option<Evaluation> = None;
            for id in self.ordered_candidates() {
                let Some(e) = self.evaluate(&id, &block)? else {
                    continue;
                };
                if !matches!(self.c.builder, Builder::Greedy) {
                    best = Some(e);
                    break;
                }
                let better = best.as_ref().is_none_or(|old| {
                    let a = &self.probes[e.trace];
                    let b = &self.probes[old.trace];
                    a.score > b.score
                        || (a.score == b.score
                            && (a.used < b.used
                                || (a.used == b.used
                                    && self.fifo_key(&a.candidate) < self.fifo_key(&b.candidate))))
                });
                if better {
                    best = Some(e);
                }
            }
            let Some(chosen) = best else {
                break;
            };
            self.commit_candidate(chosen, &mut block)?;
        }
        block.state.invariant(self.s)?;
        Ok(block)
    }
}
