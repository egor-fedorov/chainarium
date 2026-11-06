use chainarium_core::{
    scheduler::bundles::{self as g, CandidateId, Member},
    scheduler::{Builder, Channel, Delivery, EventKind, Status},
    *,
};
use std::collections::BTreeSet;
fn fixture(name: &str) -> Scenario {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../scenarios/demos/sc-bundle-{name}.json"));
    Scenario::parse(&std::fs::read(p).unwrap()).unwrap()
}
fn bundle(s: &mut Scenario) -> &mut g::Bundle {
    &mut s
        .scheduler
        .as_mut()
        .unwrap()
        .bundles
        .as_mut()
        .unwrap()
        .emissions[0]
        .bundle
}
fn own(s: &mut Scenario, i: usize) -> &mut Transaction {
    let Member::Own { transaction } = &mut bundle(s).members[i] else {
        panic!("own member")
    };
    transaction
}
fn transfer(id: &str, nonce: u64, amount: u64, to: &str) -> Transaction {
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
            to: to.into(),
            asset: "Q".into(),
            amount: amount.into(),
        }],
    }
}
fn selected(r: &Run) -> Vec<&g::Probe> {
    r.blocks[0]
        .scheduler
        .as_ref()
        .unwrap()
        .probes
        .iter()
        .filter(|p| p.selected)
        .collect()
}
#[test]
fn allowed_revert_preserves_first_transaction_and_bid_is_paid_once() {
    let mut r = Run::new(fixture("allowed")).unwrap();
    r.finish().unwrap();
    let b = &r.blocks[0];
    assert_eq!(
        b.receipts
            .iter()
            .map(|r| r.status.as_str())
            .collect::<Vec<_>>(),
        vec!["Success", "ProtocolRevert", "Success"]
    );
    assert_eq!(b.used, 355.into());
    assert_eq!(r.state.balance("alice", "Q"), 575.into());
    assert_eq!(r.state.balance("bob", "Q"), 20.into());
    assert_eq!(r.state.balance("builder", "Q"), 405.into());
    assert_eq!(r.state.accounts["alice"].nonce, 3.into());
    let p = selected(&r)[0];
    assert_eq!(p.paid_fees, 355.into());
    assert_eq!(p.bid, 50.into());
    assert_eq!(p.score, 405.into());
    assert_eq!(p.builder_delta.magnitude, 405.into());
}
#[test]
fn forbidden_late_revert_has_no_canonical_receipts_fees_or_nonce() {
    let mut r = Run::new(fixture("forbidden")).unwrap();
    r.finish().unwrap();
    assert!(r.blocks.iter().all(|b| b.receipts.is_empty()));
    assert_eq!(r.state.accounts, r.initial.accounts);
    let probe = &r.blocks[0].scheduler.as_ref().unwrap().probes[0];
    assert!(!probe.selected);
    assert_eq!(probe.receipts.len(), 2);
    assert_eq!(probe.receipts[0].status, "Success");
    assert_eq!(probe.receipts[1].fee, 125.into());
    assert!(
        matches!(&r.scheduler.as_ref().unwrap().bundles["group"].status,Status::Rejected{reason} if reason=="ExpiredTargetRange")
    );
}
#[test]
fn greedy_and_fee_priority_compare_same_inputs_by_explicit_score() {
    for (name, fee, alice, carol, bob, count) in [
        ("greedy", 405, 575, 1000, 20, 3),
        ("fee", 345, 1000, 654, 1, 1),
    ] {
        let mut r = Run::new(fixture(name)).unwrap();
        r.finish().unwrap();
        assert_eq!(r.blocks[0].receipts.len(), count);
        for (who, n) in [
            ("builder", fee),
            ("alice", alice),
            ("carol", carol),
            ("bob", bob),
        ] {
            assert_eq!(r.state.balance(who, "Q"), n.into());
        }
        assert_eq!(
            selected(&r).iter().map(|p| p.score).collect::<Vec<_>>(),
            vec![fee.into()]
        );
    }
}
#[test]
fn each_declared_limit_must_fit_boundary_439_440_despite_actual_355() {
    for limit in [439, 440] {
        let mut s = fixture("allowed");
        s.chain.block_limit = limit.into();
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert_eq!(r.blocks[0].receipts.len(), if limit == 440 { 3 } else { 0 });
        if limit == 439 {
            assert_eq!(r.state.accounts, r.initial.accounts);
            assert!(
                r.blocks[0].scheduler.as_ref().unwrap().probes[0]
                    .outcome
                    .contains("declared limit")
            );
        }
    }
}
#[test]
fn invalid_bid_authorship_or_membership_rejects_whole_registration() {
    for mutation in [
        "amount",
        "payer",
        "self",
        "position",
        "allowed-payment",
        "unknown-revert",
        "duplicate",
        "foreign",
        "reserved",
    ] {
        let mut s = fixture("allowed");
        match mutation {
            "amount" => bundle(&mut s).bid = 51.into(),
            "payer" => own(&mut s, 2).sender = "bob".into(),
            "self" => {
                s.agents
                    .get_mut("trader")
                    .unwrap()
                    .controlled_accounts
                    .insert("builder".into());
                own(&mut s, 2).sender = "builder".into();
            }
            "position" => bundle(&mut s).payment_tx_id = Some("first".into()),
            "allowed-payment" => {
                bundle(&mut s).allowed_reverts.insert("payment".into());
            }
            "unknown-revert" => {
                bundle(&mut s).allowed_reverts.insert("missing".into());
            }
            "duplicate" => {
                let first = bundle(&mut s).members[0].clone();
                bundle(&mut s).members[1] = first;
            }
            "foreign" => own(&mut s, 0).agent = "recipient".into(),
            _ => own(&mut s, 0).id = "policy:future".into(),
        }
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert!(r.blocks[0].receipts.is_empty(), "{mutation}");
        assert!(
            r.scheduler.as_ref().unwrap().known.is_empty(),
            "partial registration: {mutation}"
        );
        assert_eq!(r.state.accounts, r.initial.accounts);
    }
}
#[test]
fn late_unfunded_payment_discards_successful_prefix_and_simulated_fees() {
    let mut s = fixture("allowed");
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), 600.into());
    bundle(&mut s).bid = 300.into();
    own(&mut s, 2).program = vec![Call::Transfer {
        to: "builder".into(),
        asset: "Q".into(),
        amount: 300.into(),
    }];
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert_eq!(r.state.accounts, r.initial.accounts);
    assert!(r.blocks[0].receipts.is_empty());
    let probe = &r.blocks[0].scheduler.as_ref().unwrap().probes[0];
    assert_eq!(probe.receipts.len(), 3);
    assert_eq!(probe.receipts[2].status, "ProtocolRevert");
    assert!(!probe.selected);
}
#[test]
fn allowed_exhaustion_is_included_with_real_fee_and_nonce() {
    let mut s = fixture("allowed");
    own(&mut s, 1).limit = 100.into();
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert_eq!(r.blocks[0].receipts[1].status, "ResourceExhausted");
    assert_eq!(r.blocks[0].used, 330.into());
    assert_eq!(r.state.balance("alice", "Q"), 600.into());
    assert_eq!(r.state.balance("builder", "Q"), 380.into());
    assert_eq!(r.state.accounts["alice"].nonce, 3.into());
}
#[test]
fn visible_reference_never_uses_future_or_private_global_payload() {
    for name in ["visible", "private"] {
        let mut r = Run::new(fixture(name)).unwrap();
        r.finish().unwrap();
        assert_eq!(
            r.state.balance("alice", "Q"),
            if name == "visible" {
                10011.into()
            } else {
                10000.into()
            }
        );
        assert_eq!(
            r.state.balance("builder", "Q"),
            if name == "visible" {
                5.into()
            } else {
                Amount::ZERO
            }
        );
        assert_eq!(
            r.blocks[0].receipts.len(),
            if name == "visible" { 3 } else { 1 }
        );
        if name == "private" {
            assert!(!r.scheduler.as_ref().unwrap().known.contains_key("response"));
            assert!(
                matches!(&r.scheduler.as_ref().unwrap().bundles["visible-group"].status,Status::Rejected{reason} if reason=="UnknownVisibleTxId")
            );
        }
    }
    let mut s = fixture("visible");
    s.scheduler
        .as_mut()
        .unwrap()
        .bundles
        .as_mut()
        .unwrap()
        .emissions[0]
        .at = 8.into();
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert_eq!(r.blocks[0].receipts.len(), 1); // The emission event precedes equal-time delivery by EventId.
}
#[test]
fn bundle_member_consumption_survives_later_standalone_builder_delivery() {
    let mut s = fixture("visible");
    let c = s.scheduler.as_mut().unwrap();
    c.rounds = 4;
    c.emissions[0].delivery.builder_delay = 30.into();
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let cp = r.checkpoint().unwrap();
    let mut resumed = Run::restore(&cp).unwrap();
    r.finish().unwrap();
    resumed.finish().unwrap();
    assert_eq!(r, resumed);
    assert_eq!(
        r.blocks
            .iter()
            .flat_map(|b| &b.receipts)
            .filter(|t| t.tx_id == "victim")
            .count(),
        1
    );
    assert!(
        matches!(&r.scheduler.as_ref().unwrap().known["victim"].status,Status::Included{block,..} if *block==1.into())
    );
    assert!(
        r.blocks[3]
            .scheduler
            .as_ref()
            .unwrap()
            .trace
            .iter()
            .any(|t| t.action == "AlreadyIncluded")
    );
}
fn overlap(s: &mut Scenario) {
    let cfg = s.scheduler.as_mut().unwrap().bundles.as_mut().unwrap();
    let mut e = cfg.emissions[0].clone();
    e.at = 2.into();
    e.producer_sequence = 1.into();
    e.bundle.id = "other".into();
    e.bundle.members.truncate(1);
    e.bundle.members.push(Member::Own {
        transaction: Box::new(transfer("other-payment", 1, 100, "builder")),
    });
    e.bundle.allowed_reverts.clear();
    e.bundle.bid = 100.into();
    e.bundle.payment_tx_id = Some("other-payment".into());
    cfg.emissions.push(e);
}
#[test]
fn overlap_does_not_repeat_shared_tx_and_failed_probe_does_not_destroy_other_group() {
    for name in ["allowed", "forbidden"] {
        let mut s = fixture(name);
        overlap(&mut s);
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert_eq!(
            r.blocks[0]
                .receipts
                .iter()
                .filter(|t| t.tx_id == "first")
                .count(),
            1
        );
        let winner = if name == "allowed" { "group" } else { "other" };
        assert_eq!(
            selected(&r)[0].candidate,
            CandidateId::Bundle { id: winner.into() }
        );
        assert_eq!(
            r.state.balance("builder", "Q"),
            if name == "allowed" {
                405.into()
            } else {
                330.into()
            }
        );
    }
}
#[test]
fn conflicting_payload_and_duplicate_bundle_keep_first_and_register_no_partial_members() {
    let mut s = fixture("allowed");
    overlap(&mut s);
    let cfg = s.scheduler.as_mut().unwrap().bundles.as_mut().unwrap();
    let Member::Own { transaction } = &mut cfg.emissions[1].bundle.members[0] else {
        panic!()
    };
    transaction.program = vec![Call::Transfer {
        to: "bob".into(),
        asset: "Q".into(),
        amount: 21.into(),
    }];
    let mut duplicate = cfg.emissions[0].clone();
    duplicate.at = 3.into();
    duplicate.producer_sequence = 2.into();
    cfg.emissions.push(duplicate);
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert!(
        !r.scheduler
            .as_ref()
            .unwrap()
            .known
            .contains_key("other-payment")
    );
    assert!(
        matches!(&r.scheduler.as_ref().unwrap().bundles["other"].status,Status::Rejected{reason} if reason=="ConflictingMemberTxId")
    );
    assert_eq!(r.blocks[0].receipts.len(), 3);
    assert!(
        r.blocks[0]
            .scheduler
            .as_ref()
            .unwrap()
            .trace
            .iter()
            .any(|t| t.action == "DuplicateBundle")
    );
}
#[test]
fn fixed_items_has_separate_namespaces_and_native_fixed_keeps_original_semantics() {
    for typed in [false, true] {
        let mut s = fixture("allowed");
        let first = own(&mut s, 0).clone();
        bundle(&mut s).id = "first".into();
        let c = s.scheduler.as_mut().unwrap();
        c.builder = if typed {
            Builder::FixedItems {
                priority: vec![CandidateId::Bundle { id: "first".into() }],
            }
        } else {
            Builder::Fixed {
                priority: vec!["first".into()],
            }
        };
        c.emissions.push(chainarium_core::scheduler::Emission {
            at: 2.into(),
            producer_id: "script".into(),
            producer_sequence: 0.into(),
            transaction: first,
            delivery: Delivery {
                channel: Channel::Public,
                builder_delay: 0.into(),
            },
        });
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert_eq!(r.blocks[0].receipts.len(), if typed { 3 } else { 1 });
        assert_eq!(
            r.state.balance("builder", "Q"),
            if typed { 405.into() } else { 115.into() }
        );
    }
}
#[test]
fn budgets_are_global_reserve_full_group_and_charge_only_actual_attempts() {
    for (probes, members, winner, used_attempts) in [(1, 256, "group", 3), (64, 2, "ordinary", 1)] {
        let mut s = fixture("greedy");
        let l = &mut s
            .scheduler
            .as_mut()
            .unwrap()
            .bundles
            .as_mut()
            .unwrap()
            .limits;
        l.probes = probes;
        l.probe_members = members;
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        let report = r.blocks[0].scheduler.as_ref().unwrap();
        assert_eq!(report.metrics.probes, 1);
        assert_eq!(report.metrics.probe_members, used_attempts);
        assert!(report.metrics.truncated);
        assert!(selected(&r)[0].candidate.label().ends_with(winner));
    }
    let mut s = fixture("greedy");
    bundle(&mut s).allowed_reverts.clear();
    s.scheduler
        .as_mut()
        .unwrap()
        .bundles
        .as_mut()
        .unwrap()
        .limits
        .probe_members = 3;
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let m = &r.blocks[0].scheduler.as_ref().unwrap().metrics;
    assert_eq!(m.probes, 2);
    assert_eq!(m.probe_members, 3);
    assert_eq!(r.state.balance("builder", "Q"), 345.into());
}
#[test]
fn native_registry_capacity_and_resolved_byte_caps_are_atomic() {
    for bytes in [false, true] {
        let mut s = fixture("allowed");
        let c = s.scheduler.as_mut().unwrap();
        if bytes {
            c.bundles.as_mut().unwrap().limits.bytes = 100;
        } else {
            c.limits.known = 2;
        }
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert!(r.scheduler.as_ref().unwrap().known.is_empty());
        assert_eq!(r.state.accounts, r.initial.accounts);
        assert!(r.blocks[0].receipts.is_empty());
    }
}
#[test]
fn target_range_ttl_and_inflight_bundle_checkpoint_are_exact() {
    for ttl in [1, 32] {
        let mut s = fixture("allowed");
        bundle(&mut s).min_block = 2.into();
        bundle(&mut s).max_block = 2.into();
        s.scheduler.as_mut().unwrap().limits.retention = ttl;
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert!(r.blocks[0].receipts.is_empty());
        let mut resumed = Run::restore(&r.checkpoint().unwrap()).unwrap();
        r.finish().unwrap();
        resumed.finish().unwrap();
        assert_eq!(r, resumed);
        assert_eq!(r.blocks[1].receipts.len(), if ttl == 1 { 0 } else { 3 });
    }
    let mut s = fixture("allowed");
    s.scheduler
        .as_mut()
        .unwrap()
        .bundles
        .as_mut()
        .unwrap()
        .emissions[0]
        .delivery
        .builder_delay = 10.into();
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert!(
        r.scheduler
            .as_ref()
            .unwrap()
            .queue
            .iter()
            .any(|e| matches!(e.kind, EventKind::BundleBuilderDelivery { .. }))
    );
    let mut resumed = Run::restore(&r.checkpoint().unwrap()).unwrap();
    r.finish().unwrap();
    resumed.finish().unwrap();
    assert_eq!(r, resumed);
    assert!(r.blocks[1].receipts.is_empty());
}
#[test]
fn bundle_messages_share_visible_cap_and_do_not_trigger_member_rules() {
    let mut s = fixture("visible");
    let c = s.scheduler.as_mut().unwrap();
    c.limits.visible = 1;
    c.reactive.insert(
        "trader".into(),
        chainarium_core::scheduler::ReactivePolicy {
            account: "alice".into(),
            resource_limit: 200.into(),
            resource_price: 0.into(),
            reaction_delay: 0.into(),
            lifetime: 20.into(),
            rules: std::collections::BTreeMap::from([("payment".into(), vec![])]),
        },
    );
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    let state = r.scheduler.as_ref().unwrap();
    assert!(state.visible["trader"].is_empty());
    assert_eq!(state.bundle_visible["trader"].len(), 1);
    assert!(r.blocks.iter().all(|b| b.decisions.is_empty()));
    let obs = state
        .observation(&r.scenario, &r.state, "trader", "alice")
        .unwrap();
    assert_eq!(obs.bundle_messages.len(), 1);
    assert_eq!(obs.own_bundles.len(), 1);
    let foreign = state
        .observation(&r.scenario, &r.state, "recipient", "bob")
        .unwrap();
    assert!(foreign.bundle_messages.is_empty());
    assert!(foreign.own_bundles.is_empty());
}
#[test]
fn self_fee_and_arbitrary_builder_transfers_are_not_bids_or_greedy_income() {
    for self_fee in [false, true] {
        let mut s = fixture("allowed");
        let c = s.scheduler.as_mut().unwrap();
        c.bundles.as_mut().unwrap().emissions.clear();
        let mut tx = transfer("ordinary", 0, 100, "builder");
        if self_fee {
            tx.sender = "builder".into();
            tx.price = 3.into();
            tx.program.clear();
        }
        c.emissions.push(chainarium_core::scheduler::Emission {
            at: 1.into(),
            producer_id: "script".into(),
            producer_sequence: 0.into(),
            transaction: tx,
            delivery: Delivery {
                channel: Channel::Public,
                builder_delay: 0.into(),
            },
        });
        if self_fee {
            s.agents
                .get_mut("trader")
                .unwrap()
                .controlled_accounts
                .insert("builder".into());
            s.accounts
                .get_mut("builder")
                .unwrap()
                .balances
                .insert("Q".into(), 1000.into());
        }
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        let p = selected(&r)[0];
        assert_eq!(p.score, if self_fee { Amount::ZERO } else { 115.into() });
        assert_eq!(
            p.builder_delta.magnitude,
            if self_fee { Amount::ZERO } else { 215.into() }
        );
    }
}
#[test]
fn user_amount_overflow_is_allowed_protocol_revert_not_model_error() {
    let mut s = fixture("greedy");
    s.scheduler.as_mut().unwrap().builder = Builder::FeePriority;
    own(&mut s, 1).program = vec![Call::AssertBalanceDelta {
        account: "alice".into(),
        asset: "Q".into(),
        baseline: Amount::MAX,
        minimum: 1.into(),
    }];
    s.chain.block_limit = 1000.into();
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert_eq!(r.blocks[0].receipts.len(), 4);
    assert_eq!(r.blocks[0].receipts[2].status, "ProtocolRevert");
    assert_eq!(
        r.blocks[0].receipts[2].error.as_deref(),
        Some("amount overflow")
    );
    assert_eq!(r.state.balance("builder", "Q"), 735.into());
}
#[test]
fn profiles_caps_and_combined_producer_namespace_are_validated() {
    let mut combined = fixture("greedy");
    combined
        .scheduler
        .as_mut()
        .unwrap()
        .bundles
        .as_mut()
        .unwrap()
        .emissions[0]
        .producer_id = "ordinary-script".into();
    assert!(combined.validate().is_err());
    let mut capped = fixture("greedy");
    capped.scheduler.as_mut().unwrap().limits.emissions = 1;
    assert!(capped.validate().is_err());
    let base = fixture("allowed");
    let mut s = base.clone();
    s.model_versions.retain(|m| m != "native-bundles-v1");
    assert!(s.validate().is_err());
    let mut s = base.clone();
    s.model_versions.retain(|m| m != "bounded-greedy-v1");
    assert!(s.validate().is_err());
    let mut s = base.clone();
    s.scheduler
        .as_mut()
        .unwrap()
        .bundles
        .as_mut()
        .unwrap()
        .limits
        .members = 17;
    assert!(s.validate().is_err());
    let mut s = base.clone();
    s.scheduler
        .as_mut()
        .unwrap()
        .bundles
        .as_mut()
        .unwrap()
        .limits
        .probes = 0;
    assert!(s.validate().is_err());
    let mut s = base;
    s.scheduler
        .as_mut()
        .unwrap()
        .agents
        .get_mut("trader")
        .unwrap()
        .outgoing
        .channel = Channel::Private {
        recipients: BTreeSet::from(["unknown".into()]),
    };
    assert!(s.validate().is_err());
}

#[test]
fn exact_sixteen_member_cap_and_pending_known_bundle_limits() {
    for count in [16, 17] {
        let mut s = fixture("allowed");
        s.chain.block_limit = 2000.into();
        let b = bundle(&mut s);
        b.bid = 0.into();
        b.payment_tx_id = None;
        b.allowed_reverts.clear();
        b.members = (0..count)
            .map(|i| {
                let mut t = transfer(&format!("member-{i}"), i, 1, "bob");
                t.price = 0.into();
                Member::Own {
                    transaction: Box::new(t),
                }
            })
            .collect();
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert_eq!(r.blocks[0].receipts.len(), if count == 16 { 16 } else { 0 });
        assert_eq!(
            r.state.balance("bob", "Q"),
            if count == 16 { 16.into() } else { 0.into() }
        );
        if count == 17 {
            assert!(r.scheduler.as_ref().unwrap().known.is_empty());
        }
    }
    for known_limit in [false, true] {
        let mut s = fixture("allowed");
        overlap(&mut s);
        let c = s.scheduler.as_mut().unwrap();
        c.builder = Builder::FixedItems { priority: vec![] };
        if known_limit {
            c.bundles.as_mut().unwrap().limits.known = 1;
        } else {
            c.bundles.as_mut().unwrap().limits.pending = 1;
        }
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        let state = r.scheduler.as_ref().unwrap();
        assert_eq!(state.bundle_pending.len(), 1);
        assert_eq!(state.bundles.len(), if known_limit { 1 } else { 2 });
        assert!(r.blocks[0].receipts.is_empty());
    }
}
#[test]
fn active_group_blocks_periodic_nonce_and_closed_group_releases_busy_guard() {
    let mut s = fixture("allowed");
    s.scheduler.as_mut().unwrap().rounds = 3;
    s.scheduler.as_mut().unwrap().builder = Builder::FixedItems { priority: vec![] };
    s.policies.insert(
        "trader".into(),
        chainarium_core::policy::ArbitragePolicy {
            account: "alice".into(),
            asset: "Q".into(),
            input: 100.into(),
            min_profit: 1.into(),
            resource_limit: 500.into(),
            resource_price: 0.into(),
            route_limits: None,
        },
    );
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(
        r.blocks[1].decisions[0].reason,
        "unfinished own intent at current nonce"
    );
    assert_ne!(
        r.blocks[2].decisions[0].reason,
        "unfinished own intent at current nonce"
    );
    assert_eq!(r.state.accounts["alice"].nonce, 0.into());
    let obs = r
        .scheduler
        .as_ref()
        .unwrap()
        .observation(&r.scenario, &r.state, "trader", "alice")
        .unwrap();
    assert!(obs.own_outbox.is_empty());
    assert_eq!(obs.own_bundles.len(), 1);
}
#[test]
fn terminal_standalone_id_cannot_be_reintroduced_through_bundle() {
    let mut s = fixture("visible");
    let c = s.scheduler.as_mut().unwrap();
    c.limits.pending = 1;
    let mut blocker = c.emissions[0].clone();
    blocker.at = 1.into();
    blocker.producer_id = "blocker".into();
    blocker.transaction.id = "blocker".into();
    blocker.transaction.nonce = 9.into();
    blocker.delivery.builder_delay = 0.into();
    c.emissions.insert(0, blocker);
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert!(r.blocks[0].receipts.is_empty());
    assert!(
        matches!(&r.scheduler.as_ref().unwrap().known["victim"].status,Status::Rejected{reason} if reason=="MempoolLimit")
    );
    // Bundle resolves before the same-time victim arrival (EventId); terminal rejection is checked again before selection.
    assert!(matches!(
        &r.scheduler.as_ref().unwrap().bundles["visible-group"].status,
        Status::Rejected { .. }
    ));
    assert_eq!(r.state.accounts, r.initial.accounts);
}
#[test]
fn greedy_ties_use_actual_work_then_fifo_and_do_not_select_negative_cash_delta() {
    let mut s = fixture("allowed");
    let c = s.scheduler.as_mut().unwrap();
    c.bundles.as_mut().unwrap().emissions.clear();
    let mut slow = transfer("slow-first", 0, 0, "bob");
    slow.price = 0.into();
    slow.program.push(Call::AssertCondition {
        condition: true,
        message: "work".into(),
    });
    let mut fast = slow.clone();
    fast.id = "fast-later".into();
    fast.program.truncate(1);
    c.emissions = [slow, fast]
        .into_iter()
        .enumerate()
        .map(|(i, t)| chainarium_core::scheduler::Emission {
            at: (i as u64 + 1).into(),
            producer_id: "script".into(),
            producer_sequence: (i as u64).into(),
            transaction: t,
            delivery: Delivery {
                channel: Channel::Public,
                builder_delay: 0.into(),
            },
        })
        .collect();
    let mut r = Run::new(s.clone()).unwrap();
    r.step().unwrap();
    assert_eq!(r.blocks[0].receipts[0].tx_id, "fast-later");
    s.scheduler.as_mut().unwrap().emissions[0]
        .transaction
        .program
        .truncate(1);
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert_eq!(r.blocks[0].receipts[0].tx_id, "slow-first");
    let mut s = fixture("allowed");
    s.agents
        .get_mut("trader")
        .unwrap()
        .controlled_accounts
        .insert("builder".into());
    s.accounts
        .get_mut("builder")
        .unwrap()
        .balances
        .insert("Q".into(), 1000.into());
    let c = s.scheduler.as_mut().unwrap();
    c.bundles.as_mut().unwrap().emissions.clear();
    let mut t = transfer("builder-spend", 0, 1, "bob");
    t.sender = "builder".into();
    c.emissions.push(chainarium_core::scheduler::Emission {
        at: 1.into(),
        producer_id: "script".into(),
        producer_sequence: 0.into(),
        transaction: t,
        delivery: Delivery {
            channel: Channel::Public,
            builder_delay: 0.into(),
        },
    });
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert!(r.blocks[0].receipts.is_empty());
    assert_eq!(r.state.accounts, r.initial.accounts);
    assert_eq!(
        r.blocks[0].scheduler.as_ref().unwrap().probes[0].outcome,
        "NegativeBuilderDelta"
    );
}

#[test]
fn member_deadline_is_checked_at_actual_builder_delivery_and_cutoff_equality_is_valid() {
    for deadline in [9, 10] {
        let mut s = fixture("allowed");
        own(&mut s, 2).deadline = deadline.into();
        s.scheduler
            .as_mut()
            .unwrap()
            .bundles
            .as_mut()
            .unwrap()
            .emissions[0]
            .delivery
            .builder_delay = 9.into();
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert_eq!(
            r.blocks[0].receipts.len(),
            if deadline == 10 { 3 } else { 0 }
        );
        if deadline == 9 {
            let report = r.blocks[0].scheduler.as_ref().unwrap();
            assert!(!report.trace.iter().any(|t| t.action == "BundleAdmitted"));
            assert!(
                matches!(&r.scheduler.as_ref().unwrap().bundles["group"].status,Status::Rejected{reason} if reason=="ExpiredMemberDeadline")
            );
        }
    }
}

#[test]
fn full_64_probe_budget_does_not_inspect_later_higher_fee_candidate() {
    let mut s = fixture("allowed");
    s.chain.interval = 100.into();
    let c = s.scheduler.as_mut().unwrap();
    c.rounds = 1;
    c.bundles.as_mut().unwrap().emissions.clear();
    c.emissions = (0..65)
        .map(|i| {
            let mut t = transfer(&format!("candidate-{i}"), 0, 0, "bob");
            if i == 64 {
                t.price = 3.into();
            }
            chainarium_core::scheduler::Emission {
                at: (i + 1).into(),
                producer_id: "script".into(),
                producer_sequence: i.into(),
                transaction: t,
                delivery: Delivery {
                    channel: Channel::Public,
                    builder_delay: 0.into(),
                },
            }
        })
        .collect();
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let report = r.blocks[0].scheduler.as_ref().unwrap();
    assert_eq!(report.metrics.probes, 64);
    assert_eq!(report.metrics.probe_members, 64);
    assert!(report.metrics.truncated);
    assert_eq!(r.blocks[0].receipts.len(), 1);
    assert_eq!(r.blocks[0].receipts[0].tx_id, "candidate-0");
    assert_eq!(r.state.balance("builder", "Q"), 115.into());
    assert!(report.probes.iter().any(|p| p.candidate
        == CandidateId::Transaction {
            id: "candidate-64".into()
        }
        && p.outcome == "ProbeLimit"));
}
#[test]
fn full_256_member_attempt_budget_keeps_probe_fees_out_of_canonical_balance() {
    let mut s = fixture("allowed");
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), 50000.into());
    s.chain.block_limit = 5000.into();
    let b = bundle(&mut s);
    b.bid = 0.into();
    b.payment_tx_id = None;
    b.allowed_reverts.clear();
    b.members = (0..16)
        .map(|i| Member::Own {
            transaction: Box::new(transfer(&format!("member-{i}"), i, 1, "bob")),
        })
        .collect();
    let c = s.scheduler.as_mut().unwrap();
    c.rounds = 1;
    let cfg = c.bundles.as_mut().unwrap();
    let template = cfg.emissions[0].clone();
    cfg.emissions = (0..17)
        .map(|i| {
            let mut e = template.clone();
            e.producer_sequence = i.into();
            e.bundle.id = format!("group-{i}");
            e
        })
        .collect();
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let report = r.blocks[0].scheduler.as_ref().unwrap();
    assert_eq!(report.metrics.probes, 16);
    assert_eq!(report.metrics.probe_members, 256);
    assert_eq!(report.metrics.probe_used, 29440.into());
    assert!(report.metrics.truncated);
    assert_eq!(r.blocks[0].receipts.len(), 16);
    assert_eq!(r.state.balance("builder", "Q"), 1840.into());
    assert_eq!(r.state.balance("alice", "Q"), 48144.into());
    assert_eq!(r.state.balance("bob", "Q"), 16.into());
    assert_eq!(selected(&r).len(), 1);
}
