use chainarium_core::{scheduler::*, *};
use std::collections::BTreeSet;
fn fixture(name: &str) -> Scenario {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../scenarios/demos/sc-scheduler-{name}.json"));
    Scenario::parse(&std::fs::read(path).unwrap()).unwrap()
}
fn transfer(id: &str, nonce: u64, amount: u64) -> Transaction {
    Transaction {
        id: id.into(),
        agent: "trader".into(),
        sender: "alice".into(),
        chain: "lab-1".into(),
        nonce: nonce.into(),
        limit: 200.into(),
        price: 1.into(),
        deadline: 100.into(),
        program: vec![Call::Transfer {
            to: "bob".into(),
            asset: "Q".into(),
            amount: amount.into(),
        }],
    }
}
fn emit(at: u64, tx: Transaction) -> Emission {
    Emission {
        at: at.into(),
        producer_id: tx.id.clone(),
        producer_sequence: at.into(),
        transaction: tx,
        delivery: Delivery {
            channel: Channel::Public,
            builder_delay: Amount::ZERO,
        },
    }
}
fn actions(r: &Run, action: &str) -> usize {
    r.blocks
        .iter()
        .flat_map(|b| &b.scheduler.as_ref().unwrap().trace)
        .filter(|t| t.action == action)
        .count()
}
#[test]
fn cutoff_public_private_delayed_exact_reference_and_replay() {
    for name in ["public", "private", "delayed"] {
        let s = fixture(name);
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert_eq!(
            r.blocks[0].receipts.len(),
            if name == "public" { 2 } else { 1 }
        );
        assert_eq!(r.state.balance("bob", "X"), 166.into());
        let reaction = r.blocks[0].decisions.first();
        if name == "private" {
            assert!(reaction.is_none());
        } else {
            let tx = reaction.unwrap().transaction.as_ref().unwrap();
            assert_eq!(tx.deadline, 39.into());
            assert_eq!(tx.nonce, Amount::ZERO);
            assert_eq!(r.blocks[0].decisions[0].observed_block, Amount::ZERO);
            if name == "delayed" {
                assert!(
                    r.scheduler
                        .as_ref()
                        .unwrap()
                        .queue
                        .iter()
                        .any(|e| e.due_at == 11.into())
                );
            }
        }
        let cp = r.checkpoint().unwrap();
        let mut resumed = Run::restore(&cp).unwrap();
        r.finish().unwrap();
        resumed.finish().unwrap();
        assert_eq!(r, resumed);
        if name == "private" {
            assert_eq!(r.state.balance("alice", "Q"), 10000.into());
            let obs = r
                .scheduler
                .as_ref()
                .unwrap()
                .observation(&r.scenario, &r.state, "trader", "alice")
                .unwrap();
            assert!(obs.messages.is_empty());
            assert!(obs.own_outbox.is_empty());
        } else {
            assert_eq!(r.state.balance("alice", "Q"), 10016.into());
            assert_eq!(r.state.balance("pool", "X"), 924.into());
            assert_eq!(r.state.balance("pool", "Q"), 1084.into());
            assert_eq!(r.state.balance("pool-b", "X"), 910.into());
            assert_eq!(r.state.balance("pool-b", "Q"), 1100.into());
            assert_eq!(r.blocks.iter().map(|b| b.receipts.len()).sum::<usize>(), 2);
            let units = r
                .blocks
                .iter()
                .try_fold(Amount::ZERO, |a, b| a.checked_add(b.used))
                .unwrap();
            assert_eq!(units, 280.into());
        }
    }
}
#[test]
fn zero_delay_cutoff_and_equal_time_event_order_are_deterministic() {
    let mut s = fixture("public");
    let c = s.scheduler.as_mut().unwrap();
    c.emissions[0].at = 10.into();
    c.emissions[0].delivery.builder_delay = Amount::ZERO;
    c.agents.get_mut("trader").unwrap().observation_delay = Amount::ZERO;
    c.agents.get_mut("trader").unwrap().outgoing.builder_delay = Amount::ZERO;
    c.reactive.get_mut("trader").unwrap().reaction_delay = Amount::ZERO;
    let mut a = Run::new(s.clone()).unwrap();
    let mut b = Run::new(s).unwrap();
    a.finish().unwrap();
    b.finish().unwrap();
    assert_eq!(a, b);
    assert_eq!(a.blocks[0].receipts.len(), 2);
    assert!(
        a.blocks[0]
            .scheduler
            .as_ref()
            .unwrap()
            .trace
            .iter()
            .all(|t| t.at == 10.into())
    );
}
#[test]
fn fifo_nonce_gap_and_fee_priority_have_analytical_balances() {
    for (name, balance, fee, nonce, ids) in [
        ("nonce", 740, 230, 2, vec!["nonce-0", "nonce-1"]),
        ("fifo", 875, 115, 1, vec!["early"]),
        ("fee", 760, 230, 1, vec!["expensive"]),
    ] {
        let mut r = Run::new(fixture(name)).unwrap();
        r.finish().unwrap();
        assert_eq!(r.state.balance("alice", "Q"), balance.into());
        assert_eq!(r.state.balance("builder", "Q"), fee.into());
        assert_eq!(r.state.accounts["alice"].nonce, nonce.into());
        assert_eq!(
            r.blocks[0]
                .receipts
                .iter()
                .map(|x| x.tx_id.as_str())
                .collect::<Vec<_>>(),
            ids
        );
    }
}
#[test]
fn funding_wakeup_rechecks_candidates_and_only_actual_used_reduces_block() {
    let mut s = fixture("nonce");
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), 0.into());
    s.accounts
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("Q".into(), 1000.into());
    let mut fund = transfer("fund", 0, 500);
    fund.sender = "bob".into();
    fund.agent = "recipient".into();
    fund.program = vec![Call::Transfer {
        to: "alice".into(),
        asset: "Q".into(),
        amount: 500.into(),
    }];
    s.scheduler.as_mut().unwrap().emissions = vec![
        emit(1, transfer("wait", 0, 10)),
        emit(2, fund),
        emit(3, transfer("next", 1, 10)),
        emit(4, transfer("spill", 2, 10)),
    ];
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert_eq!(
        r.blocks[0]
            .receipts
            .iter()
            .map(|x| x.tx_id.as_str())
            .collect::<Vec<_>>(),
        vec!["fund", "wait", "next"]
    );
    assert_eq!(r.blocks[0].used, 345.into());
    assert!(r.scheduler.as_ref().unwrap().pending.contains_key("spill"));
    assert!(actions(&r, "Waiting") > 0);
    r.step().unwrap();
    assert_eq!(r.blocks[1].receipts[0].tx_id, "spill");
    assert_eq!(r.state.balance("alice", "Q"), 125.into());
}
#[test]
fn included_revert_consumes_nonce_fee_and_stales_competitor() {
    let mut s = fixture("fifo");
    let c = s.scheduler.as_mut().unwrap();
    c.emissions[0]
        .transaction
        .program
        .push(Call::AssertCondition {
            condition: false,
            message: "late".into(),
        });
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let receipt = &r.blocks[0].receipts[0];
    assert_eq!(receipt.status, "ProtocolRevert");
    assert_eq!(receipt.fee, 125.into());
    assert_eq!(r.state.balance("alice", "Q"), 875.into());
    assert_eq!(r.state.balance("bob", "Q"), Amount::ZERO);
    assert_eq!(r.state.accounts["alice"].nonce, 1.into());
    assert_eq!(r.blocks[0].rejected[0].reason, "StaleNonce");
}
#[test]
fn admission_rejects_invalid_envelopes_without_fees_and_deadline_is_inclusive() {
    let mut s = fixture("nonce");
    let mut txs = vec![];
    let mut wrong = transfer("wrong-chain", 0, 1);
    wrong.chain = "wrong".into();
    txs.push(wrong);
    let mut foreign = transfer("foreign", 0, 1);
    foreign.sender = "bob".into();
    txs.push(foreign);
    let mut huge = transfer("unfit", 0, 1);
    huge.limit = 501.into();
    txs.push(huge);
    let mut expired = transfer("expired", 0, 1);
    expired.deadline = Amount::ZERO;
    txs.push(expired);
    let mut valid = transfer("boundary", 0, 1);
    valid.deadline = 10.into();
    txs.push(valid);
    s.scheduler.as_mut().unwrap().emissions = txs
        .into_iter()
        .enumerate()
        .map(|(i, t)| emit(i as u64 + 1, t))
        .collect();
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(r.blocks[0].receipts.len(), 1);
    assert_eq!(r.blocks[0].receipts[0].tx_id, "boundary");
    assert_eq!(r.state.balance("builder", "Q"), 115.into());
    assert_eq!(actions(&r, "Rejected"), 4);
}
#[test]
fn duplicate_and_conflicting_ids_preserve_original_arrival_and_terminal_status() {
    let mut s = fixture("nonce");
    let original = transfer("same", 0, 10);
    let mut conflict = original.clone();
    conflict.price = 2.into();
    s.scheduler.as_mut().unwrap().emissions = vec![
        emit(1, original.clone()),
        emit(2, original.clone()),
        emit(3, conflict),
        emit(11, original),
    ];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(actions(&r, "Duplicate"), 2);
    assert_eq!(actions(&r, "ConflictingTxId"), 1);
    assert_eq!(actions(&r, "Included"), 1);
    assert_eq!(
        r.scheduler.as_ref().unwrap().known["same"].submitted_at,
        1.into()
    );
    assert_eq!(r.state.balance("builder", "Q"), 115.into());
}
#[test]
fn private_only_named_recipient_and_own_status_not_global_orderflow() {
    let mut s = fixture("private");
    let c = s.scheduler.as_mut().unwrap();
    c.emissions[0].delivery.channel = Channel::Private {
        recipients: BTreeSet::from(["trader".into()]),
    };
    c.agents.get_mut("trader").unwrap().observe_public = false;
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert_eq!(r.blocks[0].receipts.len(), 2);
    let state = r.scheduler.as_ref().unwrap();
    let obs = state
        .observation(&r.scenario, &r.state, "trader", "alice")
        .unwrap();
    assert_eq!(obs.messages.len(), 1);
    assert_eq!(obs.own_outbox.len(), 1);
    assert_eq!(obs.market.account, "alice");
    assert!(
        state
            .observation(&r.scenario, &r.state, "recipient", "alice")
            .is_err()
    );
    let bob = state
        .observation(&r.scenario, &r.state, "recipient", "bob")
        .unwrap();
    assert!(bob.messages.is_empty());
    assert_eq!(bob.own_outbox.len(), 1);
    let wire = serde_json::to_string(&obs).unwrap();
    assert!(!wire.contains("emissions"));
    assert!(!wire.contains("pool-b\":{\"balances"));
}
#[test]
fn fixed_empty_and_retention_prune_pending_and_visible_at_exact_age() {
    let mut s = fixture("nonce");
    let c = s.scheduler.as_mut().unwrap();
    c.builder = Builder::Fixed { priority: vec![] };
    c.limits.retention = 2;
    c.agents.get_mut("trader").unwrap().observe_public = true;
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    r.step().unwrap();
    assert_eq!(r.scheduler.as_ref().unwrap().pending.len(), 2);
    assert_eq!(r.scheduler.as_ref().unwrap().visible["trader"].len(), 2);
    r.step().unwrap();
    assert!(r.scheduler.as_ref().unwrap().pending.is_empty());
    assert!(r.scheduler.as_ref().unwrap().visible["trader"].is_empty());
    assert_eq!(r.state.accounts["alice"].nonce, Amount::ZERO);
    assert_eq!(r.state.balance("builder", "Q"), Amount::ZERO);
    assert!(
        r.blocks[2]
            .rejected
            .iter()
            .all(|t| t.reason == "TTLExpired")
    );
}
#[test]
fn limits_bound_queue_known_pool_visible_and_defer_original_due_time() {
    for (limit, action) in [
        ("queue", "QueueLimit"),
        ("known", "KnownTransactionsLimit"),
        ("pending", "Rejected"),
        ("visible", "VisibleLimit"),
    ] {
        let mut s = fixture("nonce");
        let c = s.scheduler.as_mut().unwrap();
        c.builder = Builder::Fixed { priority: vec![] };
        c.agents.get_mut("trader").unwrap().observe_public = true;
        match limit {
            "queue" => c.limits.queue = 1,
            "known" => c.limits.known = 1,
            "pending" => c.limits.pending = 1,
            _ => c.limits.visible = 1,
        }
        let mut r = Run::new(s).unwrap();
        r.finish().unwrap();
        assert!(actions(&r, action) > 0, "{limit}");
        let state = r.scheduler.as_ref().unwrap();
        assert_eq!(r.state.accounts["alice"].nonce, Amount::ZERO);
        if limit == "known" {
            assert_eq!(state.known.len(), 1);
        }
        if limit == "pending" {
            assert_eq!(state.pending.len(), 1);
        }
        if limit == "visible" {
            assert_eq!(state.visible["trader"].len(), 1);
        }
    }
    let mut s = fixture("nonce");
    let c = s.scheduler.as_mut().unwrap();
    c.emissions.truncate(1);
    c.emissions[0].transaction = transfer("defer", 0, 1);
    c.limits.events_per_round = 1;
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert!(r.blocks[0].receipts.is_empty());
    assert_eq!(r.scheduler.as_ref().unwrap().queue[0].due_at, 1.into());
    let mut resumed = Run::restore(&r.checkpoint().unwrap()).unwrap();
    r.step().unwrap();
    resumed.step().unwrap();
    assert_eq!(r, resumed);
    let trace = &r.blocks[1].scheduler.as_ref().unwrap().trace;
    let delivery = trace.iter().find(|t| t.action == "Admitted").unwrap();
    assert_eq!(delivery.due_at, 1.into());
    assert_eq!(delivery.at, 10.into());
}
#[test]
fn builder_attempt_and_eligibility_budgets_preserve_waiting_entries() {
    for attempts in [true, false] {
        let mut s = fixture("nonce");
        let c = s.scheduler.as_mut().unwrap();
        if attempts {
            c.limits.inclusion_attempts = 1;
        } else {
            c.limits.eligibility_checks = 1;
        }
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert_eq!(r.blocks[0].receipts.len(), if attempts { 1 } else { 0 });
        assert_eq!(
            r.scheduler.as_ref().unwrap().pending.len(),
            if attempts { 1 } else { 2 }
        );
        assert_eq!(
            actions(
                &r,
                if attempts {
                    "InclusionLimit"
                } else {
                    "EligibilityLimit"
                }
            ),
            1
        );
    }
}
#[test]
fn model_profiles_caps_and_policy_overlap_are_validated() {
    let base = fixture("public");
    let mut s = base.clone();
    s.model_versions.push("scheduler-m1-v1".into());
    assert!(s.validate().is_err());
    let mut s = base.clone();
    s.scheduler.as_mut().unwrap().limits.queue = 4097;
    assert!(s.validate().is_err());
    let mut s = base.clone();
    s.scheduler.as_mut().unwrap().limits.queue = 0;
    assert!(s.validate().is_err());
    let mut s = base.clone();
    s.scheduler.as_mut().unwrap().agents.remove("trader");
    assert!(s.validate().is_err());
    let mut s = base.clone();
    let e = s.scheduler.as_ref().unwrap().emissions[0].clone();
    s.scheduler.as_mut().unwrap().emissions.push(e);
    assert!(s.validate().is_err());
    let mut s = base;
    s.scheduler
        .as_mut()
        .unwrap()
        .reactive
        .get_mut("trader")
        .unwrap()
        .account = "bob".into();
    assert!(s.validate().is_err());
}
#[test]
fn checkpoint_inflight_replay_and_readonly_observation_do_not_change_run() {
    let mut r = Run::new(fixture("delayed")).unwrap();
    r.step().unwrap();
    let cp = r.checkpoint().unwrap();
    for _ in 0..10 {
        r.scheduler
            .as_ref()
            .unwrap()
            .observation(&r.scenario, &r.state, "trader", "alice")
            .unwrap();
    }
    assert_eq!(cp, r.checkpoint().unwrap());
    let mut tampered: serde_json::Value = serde_json::from_slice(&cp).unwrap();
    tampered["run"]["scheduler"]["next_event_id"] = "999".into();
    // A freshly signed digest still cannot bypass deterministic replay validation.
    let parsed: Run = serde_json::from_value(tampered["run"].clone()).unwrap();
    tampered["hash"] = chainarium_core::run::digest(&serde_json::to_vec(&parsed).unwrap()).into();
    assert!(
        Run::restore(&serde_json::to_vec(&tampered).unwrap())
            .unwrap_err()
            .contains("replay mismatch")
    );
    let mut fork = r.fork("pending").unwrap();
    let mut resumed = Run::restore(&cp).unwrap();
    r.finish().unwrap();
    resumed.finish().unwrap();
    fork.finish().unwrap();
    assert_eq!(r, resumed);
    assert_eq!(r.state, fork.state);
    assert_eq!(r.scheduler, fork.scheduler);
}
#[test]
fn event_id_overflow_aborts_whole_round_without_consuming_emissions_or_rng() {
    let mut r = Run::new(fixture("public")).unwrap();
    r.scheduler.as_mut().unwrap().next_event_id = Amount::MAX;
    let before = r.clone();
    assert!(r.step().unwrap_err().contains("ModelError"));
    assert_eq!(r, before);
}
#[test]
fn system_accrual_once_matches_legacy_blocks_even_with_waiting_candidate_checks() {
    let bytes = include_bytes!("../../../scenarios/demos/sc-05-credit.json");
    let mut s = Scenario::parse(bytes).unwrap();
    let mut legacy = Run::new(s.clone()).unwrap();
    legacy.finish().unwrap();
    let mut c = fixture("nonce").scheduler.unwrap();
    c.rounds = s.blocks.len();
    c.emissions.clear();
    c.reactive.clear();
    c.builder = Builder::Fifo;
    c.agents = s
        .agents
        .keys()
        .map(|id| {
            (
                id.clone(),
                Network {
                    observe_public: false,
                    observation_delay: 0.into(),
                    outgoing: Delivery {
                        channel: Channel::Public,
                        builder_delay: 0.into(),
                    },
                },
            )
        })
        .collect();
    let interval = s.chain.interval.to_string().parse::<u64>().unwrap();
    for (i, block) in s.blocks.iter().enumerate() {
        for (j, tx) in block.iter().enumerate() {
            let mut e = emit((i as u64 + 1) * interval, tx.clone());
            e.producer_id = "script".into();
            e.producer_sequence = (i as u64 * 100 + j as u64).into();
            c.emissions.push(e);
        }
    }
    s.blocks.clear();
    s.model_versions.retain(|v| v != "scheduler-m1-v1");
    s.model_versions.extend(
        chainarium_core::scheduler::MODELS[..3]
            .iter()
            .map(|m| m.to_string()),
    );
    s.scheduler = Some(c);
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(r.state, legacy.state);
    for (a, b) in r.blocks.iter().zip(&legacy.blocks) {
        assert_eq!(a.system_events, b.system_events);
        assert_eq!(a.receipts, b.receipts);
    }
}
#[test]
fn reaction_intent_and_depth_caps_stop_zero_delay_response_chains() {
    for cap in ["intent", "reaction"] {
        let mut s = fixture("public");
        let c = s.scheduler.as_mut().unwrap();
        let mut second = c.emissions[0].clone();
        second.at = 8.into();
        second.producer_sequence = 1.into();
        second.transaction.id = "victim-2".into();
        second.transaction.nonce = 1.into();
        c.emissions.push(second);
        let p = c.reactive.get_mut("trader").unwrap();
        p.rules.insert("victim-2".into(), p.rules["victim"].clone());
        if cap == "reaction" {
            c.limits.reactions_per_round = 1;
        }
        let mut r = Run::new(s).unwrap();
        r.finish().unwrap();
        assert_eq!(
            actions(
                &r,
                if cap == "intent" {
                    "IntentLimit"
                } else {
                    "ReactionLimit"
                }
            ),
            1
        );
        assert_eq!(
            r.blocks
                .iter()
                .flat_map(|b| &b.decisions)
                .filter(|d| d.transaction.is_some())
                .count(),
            1
        );
    }
    let mut s = fixture("public");
    let mut first = Run::new(s.clone()).unwrap();
    first.step().unwrap();
    let response = first.blocks[0].decisions[0]
        .transaction
        .as_ref()
        .unwrap()
        .id
        .clone();
    let c = s.scheduler.as_mut().unwrap();
    c.limits.depth = 1;
    let p = c.reactive.get_mut("trader").unwrap();
    p.rules.insert(response, p.rules["victim"].clone());
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(actions(&r, "DepthLimit"), 1);
    assert_eq!(actions(&r, "Reaction"), 1);
}
#[test]
fn periodic_policies_skip_current_nonce_already_in_flight() {
    let mut s = Scenario::parse(include_bytes!(
        "../../../scenarios/demos/sc-lp-policies.json"
    ))
    .unwrap();
    let mut c = fixture("nonce").scheduler.unwrap();
    c.rounds = 5;
    c.emissions.clear();
    c.agents = s
        .agents
        .keys()
        .map(|id| {
            (
                id.clone(),
                Network {
                    observe_public: false,
                    observation_delay: 0.into(),
                    outgoing: Delivery {
                        channel: Channel::Public,
                        builder_delay: 20.into(),
                    },
                },
            )
        })
        .collect();
    s.blocks.clear();
    s.model_versions.retain(|v| v != "scheduler-m1-v1");
    s.model_versions.extend(
        chainarium_core::scheduler::MODELS[..3]
            .iter()
            .map(|m| m.to_string()),
    );
    s.scheduler = Some(c);
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(r.scheduler.as_ref().unwrap().known.len(), 2);
    assert_eq!(
        r.blocks[0]
            .decisions
            .iter()
            .filter(|d| d.transaction.is_some())
            .count(),
        2
    );
    for b in &r.blocks[1..] {
        assert!(b.decisions.iter().all(
            |d| d.transaction.is_none() && d.reason == "unfinished own intent at current nonce"
        ));
    }
    assert!(r.blocks.iter().all(|b| b.receipts.is_empty()));
}
#[test]
fn fixed_priority_rechecks_nonce_and_nonlisted_entries_wait() {
    let mut s = fixture("nonce");
    s.scheduler.as_mut().unwrap().builder = Builder::Fixed {
        priority: vec!["nonce-1".into(), "nonce-0".into()],
    };
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert_eq!(
        r.blocks[0]
            .receipts
            .iter()
            .map(|t| t.tx_id.as_str())
            .collect::<Vec<_>>(),
        vec!["nonce-0", "nonce-1"]
    );
}
