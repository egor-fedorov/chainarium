use chainarium_core::{
    scheduler::{Channel, EventKind, backrun as b},
    *,
};
fn fixture(name: &str) -> Scenario {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../scenarios/demos/sc-backrun-{name}.json"));
    Scenario::parse(&std::fs::read(p).unwrap()).unwrap()
}
fn policy(s: &mut Scenario) -> &mut b::Policy {
    s.scheduler
        .as_mut()
        .unwrap()
        .backrun
        .as_mut()
        .unwrap()
        .policies
        .get_mut("trader")
        .unwrap()
}
fn limits(s: &mut Scenario) -> &mut b::Limits {
    &mut s
        .scheduler
        .as_mut()
        .unwrap()
        .backrun
        .as_mut()
        .unwrap()
        .limits
}
fn finish(s: Scenario) -> Run {
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    r
}
fn search(r: &Run) -> &b::Report {
    r.blocks[0]
        .scheduler
        .as_ref()
        .unwrap()
        .search
        .as_ref()
        .unwrap()
}

#[test]
fn exact_grid_selects_best_input_and_pays_real_bid_once() {
    let r = finish(fixture("public"));
    let report = search(&r);
    let d = &report.decisions[0];
    assert_eq!(d.candidates.len(), 10);
    assert_eq!(report.metrics.prefixes, 1);
    let best = &d.candidates[d.selected.unwrap()];
    assert_eq!(best.input, 100.into());
    assert_eq!(best.net, Some(11.into()));
    assert_eq!(
        best.hops.iter().map(|h| h.venue()).collect::<Vec<_>>(),
        vec!["second", "cpmm"]
    );
    assert_eq!(r.state.balance("alice", "Q"), 10011.into());
    assert_eq!(r.state.balance("builder", "Q"), 5.into());
    assert_eq!(r.state.balance("pool", "X"), 924.into());
    assert_eq!(r.state.balance("pool", "Q"), 1084.into());
    assert_eq!(r.state.accounts["alice"].nonce, 2.into());
    assert_eq!(r.blocks[0].used, 395.into());
    assert_eq!(
        r.blocks[0]
            .receipts
            .iter()
            .map(|x| x.used)
            .collect::<Vec<_>>(),
        vec![120.into(), 160.into(), 115.into()]
    );
    assert_eq!(
        r.blocks
            .iter()
            .flat_map(|x| &x.receipts)
            .filter(|r| r.tx_id == "victim")
            .count(),
        1
    );
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/reviews/backrun-reference.json")).unwrap();
    assert_eq!(
        best.net.unwrap().to_string(),
        expected["best"]["conditional_net_Q"].as_str().unwrap()
    );
}

#[test]
fn own_nonce_capacity_covers_response_and_optional_payment_without_aborting_round() {
    let max: Amount =
        "115792089237316195423570985008687907853269984665640564039457584007913129639935"
            .parse()
            .unwrap();
    for (available, bid, included) in [
        (0, 0, false),
        (0, 5, false),
        (1, 5, false),
        (1, 0, true),
        (2, 5, true),
    ] {
        let mut s = fixture("public");
        s.accounts.get_mut("alice").unwrap().nonce = max.checked_sub(available.into()).unwrap();
        policy(&mut s).bid = bid.into();
        // Deliver the standalone victim after the bundle so its zero-score tie cannot win.
        s.scheduler.as_mut().unwrap().emissions[0]
            .delivery
            .builder_delay = 4.into();
        let r = finish(s);
        let d = &search(&r).decisions[0];
        assert_eq!(d.bundle.is_some(), included);
        if !included {
            assert_eq!(d.reason, "NonceExhausted");
            assert!(d.candidates.is_empty());
        }
        assert_eq!(
            r.blocks
                .iter()
                .flat_map(|b| &b.receipts)
                .filter(|t| t.tx_id == "victim")
                .count(),
            1
        );
        assert_eq!(
            r.state.balance("alice", "Q"),
            if included {
                if bid == 0 { 10016.into() } else { 10011.into() }
            } else {
                10000.into()
            }
        );
    }
}

#[test]
fn unrepresentable_search_deadline_skips_without_saturating_or_aborting_victim() {
    let max: Amount =
        "115792089237316195423570985008687907853269984665640564039457584007913129639935"
            .parse()
            .unwrap();
    for (headroom, selected) in [(0, false), (8, false), (9, true)] {
        let mut s = fixture("public");
        policy(&mut s).lifetime = max.checked_sub(headroom.into()).unwrap();
        let r = finish(s);
        let d = &search(&r).decisions[0];
        assert_eq!(d.bundle.is_some(), selected);
        if selected {
            assert_eq!(d.bundle.as_ref().unwrap().deadline, max);
        } else {
            assert_eq!(d.reason, "DeadlineOverflow");
        }
        assert_eq!(r.blocks[0].receipts[0].tx_id, "victim");
        assert_eq!(
            r.state.balance("alice", "Q"),
            if selected { 10011.into() } else { 10000.into() }
        );
    }
}

#[test]
fn unrepresentable_reaction_time_drops_trigger_without_aborting_delivery() {
    let max: Amount =
        "115792089237316195423570985008687907853269984665640564039457584007913129639935"
            .parse()
            .unwrap();
    for (headroom, queued) in [(0, false), (7, false), (8, true)] {
        let mut s = fixture("public");
        policy(&mut s).reaction_delay = max.checked_sub(headroom.into()).unwrap();
        let r = finish(s);
        assert!(search(&r).decisions.is_empty());
        assert_eq!(r.blocks[0].receipts.len(), 1);
        assert_eq!(r.blocks[0].receipts[0].tx_id, "victim");
        let state = r.scheduler.as_ref().unwrap();
        assert_eq!(
            state
                .queue
                .iter()
                .any(|e| matches!(e.kind, EventKind::BackrunReaction { .. }) && e.due_at == max),
            queued
        );
        assert_eq!(
            r.blocks[0]
                .scheduler
                .as_ref()
                .unwrap()
                .trace
                .iter()
                .any(|t| t.action == "BackrunReactionTimeOverflow"),
            !queued
        );
        assert_eq!(Run::restore(&r.checkpoint().unwrap()).unwrap(), r);
    }
}
#[test]
fn private_without_recipient_cannot_start_search() {
    let r = finish(fixture("private"));
    assert!(search(&r).decisions.is_empty());
    assert_eq!(search(&r).metrics, b::Metrics::default());
    assert_eq!(r.state.balance("alice", "Q"), 10000.into());
    assert!(r.scheduler.unwrap().bundles.is_empty());
}
#[test]
fn private_explicit_recipient_can_search() {
    let mut s = fixture("private");
    s.scheduler.as_mut().unwrap().emissions[0].delivery.channel = Channel::Private {
        recipients: ["trader".into()].into(),
    };
    assert_eq!(finish(s).state.balance("alice", "Q"), 10011.into());
}
#[test]
fn delay_and_fifo_lose_without_canonical_response_fees() {
    for name in ["delayed", "fifo"] {
        let r = finish(fixture(name));
        assert!(search(&r).decisions[0].bundle.is_some());
        assert_eq!(r.state.balance("alice", "Q"), 10000.into());
        assert_eq!(r.state.accounts["alice"].nonce, Amount::ZERO);
        assert_eq!(r.blocks.iter().map(|b| b.receipts.len()).sum::<usize>(), 1);
    }
}
#[test]
fn competing_searchers_share_victim_and_greedy_picks_larger_paid_bid() {
    let r = finish(fixture("competition"));
    assert_eq!(search(&r).metrics.candidates, 20);
    assert_eq!(search(&r).decisions.len(), 2);
    assert_eq!(r.state.balance("carol", "Q"), 10009.into());
    assert_eq!(r.state.balance("alice", "Q"), 10000.into());
    assert_eq!(r.state.balance("builder", "Q"), 7.into());
    assert_eq!(r.blocks[0].receipts.len(), 3);
}
#[test]
fn fees_and_guard_cover_response_payment_and_bid() {
    let r = finish(fixture("fees"));
    let d = &search(&r).decisions[0];
    let c = &d.candidates[d.selected.unwrap()];
    assert_eq!(c.net, Some(17752.into()));
    assert_eq!(c.fees, 275.into());
    assert_eq!(r.state.balance("alice", "Q"), 1017752.into());
    assert_eq!(r.state.balance("builder", "Q"), 280.into());
    let scheduler::bundles::Member::Own { transaction } = &d.bundle.as_ref().unwrap().members[1]
    else {
        panic!()
    };
    assert!(
        matches!(transaction.program.last(),Some(Call::AssertTransactionNetGain{minimum,..}) if *minimum==121.into())
    );
}
#[test]
fn unprofitable_search_does_not_charge_canonical_fees() {
    let mut s = fixture("public");
    policy(&mut s).resource_price = 1.into();
    policy(&mut s).payment_price = 1.into();
    let r = finish(s);
    assert!(search(&r).decisions[0].bundle.is_none());
    assert_eq!(r.state.balance("alice", "Q"), 10000.into());
    assert_eq!(r.state.balance("builder", "Q"), Amount::ZERO);
    assert!(search(&r).metrics.used > Amount::ZERO);
}
#[test]
fn zero_bid_has_one_own_member_and_no_payment_fee() {
    let mut s = fixture("public");
    policy(&mut s).bid = Amount::ZERO;
    // Tie with standalone victim at score0; use FixedItems would require knowing generated ID.
    s.scheduler.as_mut().unwrap().emissions[0]
        .delivery
        .builder_delay = 4.into();
    let r = finish(s);
    let d = &search(&r).decisions[0];
    assert_eq!(d.bundle.as_ref().unwrap().members.len(), 2);
    assert!(d.bundle.as_ref().unwrap().payment_tx_id.is_none());
    assert_eq!(d.candidates[d.selected.unwrap()].net, Some(16.into()));
    assert_eq!(r.state.balance("alice", "Q"), 10016.into());
}
#[test]
fn full_declared_limit_boundary_uses_actual_prior_work() {
    for (limit, expected) in [(619, 10000), (620, 10011)] {
        let mut s = fixture("public");
        s.chain.block_limit = limit.into();
        let r = finish(s);
        assert_eq!(r.state.balance("alice", "Q"), expected.into());
    }
}
#[test]
fn failed_or_unfunded_prefix_cannot_authorize_profitable_fantasy() {
    for kind in 0..3 {
        let mut s = fixture("public");
        let tx = &mut s.scheduler.as_mut().unwrap().emissions[0].transaction;
        match kind {
            0 => tx.nonce = 1.into(),
            1 => tx.limit = 100.into(),
            _ => tx.price = 100.into(),
        }
        let r = finish(s);
        let d = &search(&r).decisions[0];
        assert!(d.bundle.is_none());
        assert!(d.candidates.is_empty());
        assert_eq!(search(&r).metrics.members, 1);
    }
}
#[test]
fn input_sorting_does_not_depend_on_config_order() {
    let original = finish(fixture("public"));
    let mut s = fixture("public");
    policy(&mut s).inputs.reverse();
    let reversed = finish(s);
    assert_eq!(search(&original), search(&reversed));
    assert_eq!(original.state, reversed.state);
}
#[test]
fn one_probe_limit_marks_truncation_without_claiming_no_opportunity() {
    let mut s = fixture("public");
    limits(&mut s).candidates_per_reaction = 1;
    let r = finish(s);
    let report = search(&r);
    assert_eq!(report.metrics.candidates, 1);
    assert!(report.metrics.truncated);
    assert!(report.decisions[0].truncated);
    assert_eq!(report.decisions[0].reason, "SearchLimit");
}
#[test]
fn best_completed_candidate_can_be_sent_at_budget_limit() {
    let mut s = fixture("public");
    limits(&mut s).candidates_per_reaction = 2;
    let r = finish(s);
    let d = &search(&r).decisions[0];
    assert!(d.truncated);
    assert_eq!(d.candidates[d.selected.unwrap()].input, 25.into());
    assert_eq!(r.state.balance("alice", "Q"), 10003.into());
}
#[test]
fn search_limits_are_shared_between_competing_agents() {
    let mut s = fixture("competition");
    limits(&mut s).candidates_per_round = 10;
    let r = finish(s);
    let report = search(&r);
    assert_eq!(report.metrics.candidates, 10);
    assert_eq!(report.metrics.prefixes, 1);
    assert_eq!(report.decisions[1].reason, "SearchLimit");
    assert!(report.decisions[1].candidates.is_empty());
}
#[test]
fn member_budget_reserves_entire_tail_and_releases_unused_attempts() {
    for (cap, count) in [(1, 0), (2, 0), (3, 1), (4, 2)] {
        let mut s = fixture("public");
        limits(&mut s).member_attempts = cap;
        let r = finish(s);
        assert_eq!(search(&r).metrics.candidates, count);
        assert!(search(&r).metrics.members <= cap);
    }
}
#[test]
fn venue_checks_bound_unproductive_graph_search() {
    let mut s = fixture("public");
    limits(&mut s).venue_checks = 1;
    let r = finish(s);
    assert_eq!(search(&r).metrics.venue_checks, 1);
    assert_eq!(search(&r).metrics.candidates, 0);
    assert!(search(&r).metrics.truncated);
}
#[test]
fn own_sender_and_own_agent_messages_are_not_triggers() {
    for controlled in [false, true] {
        let mut s = fixture("public");
        if controlled {
            s.agents
                .get_mut("trader")
                .unwrap()
                .controlled_accounts
                .insert("bob".into());
        } else {
            let tx = &mut s.scheduler.as_mut().unwrap().emissions[0].transaction;
            tx.agent = "trader".into();
            tx.sender = "alice".into();
        }
        let r = finish(s);
        assert!(search(&r).decisions.is_empty());
    }
}
#[test]
fn checkpoint_preserves_delayed_reaction_and_rejects_tampering() {
    let mut s = fixture("public");
    policy(&mut s).reaction_delay = 5.into();
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert!(
        r.scheduler
            .as_ref()
            .unwrap()
            .queue
            .iter()
            .any(|e| matches!(e.kind, EventKind::BackrunReaction { .. }))
    );
    let text = r.checkpoint().unwrap();
    let mut corrupted = r.clone();
    corrupted.scheduler.as_mut().unwrap().queue[0].due_at = 999.into();
    assert!(Run::restore(&corrupted.checkpoint().unwrap()).is_err());
    let mut resumed = Run::restore(&text).unwrap();
    r.finish().unwrap();
    resumed.finish().unwrap();
    assert_eq!(r, resumed);
}
#[test]
fn invalid_profile_parameters_and_reserved_group_ids_are_rejected() {
    for kind in 0..8 {
        let mut s = fixture("public");
        match kind {
            0 => {
                s.model_versions.retain(|m| m != b::MODEL);
            }
            1 => policy(&mut s).inputs.push(25.into()),
            2 => policy(&mut s).minimum_net_profit = Amount::ZERO,
            3 => limits(&mut s).venue_checks = 4097,
            4 => policy(&mut s).account = "builder".into(),
            5 => s.scheduler.as_mut().unwrap().bundles = None,
            6 => policy(&mut s).inputs = vec![Amount::ZERO],
            _ => policy(&mut s).lifetime = Amount::ZERO,
        }
        assert!(Run::new(s).is_err(), "case {kind}");
    }
}

#[test]
fn changed_builder_prefix_rechecks_guard_and_rejects_group_without_own_fees() {
    let mut s = fixture("public");
    s.accounts
        .insert("carol".into(), s.accounts["alice"].clone());
    s.agents.insert(
        "interferer".into(),
        Agent {
            controlled_accounts: ["carol".into()].into(),
        },
    );
    let c = s.scheduler.as_mut().unwrap();
    c.agents
        .insert("interferer".into(), c.agents["recipient"].clone());
    let mut e = c.emissions[0].clone();
    e.at = 1.into();
    e.producer_sequence = 1.into();
    e.transaction.id = "change-b".into();
    e.transaction.agent = "interferer".into();
    e.transaction.sender = "carol".into();
    e.transaction.price = 1.into();
    e.transaction.program = vec![Call::SwapExactIn {
        pool: "second".into(),
        asset_in: "Q".into(),
        amount: 200.into(),
        min_out: 1.into(),
    }];
    e.delivery.channel = Channel::Private {
        recipients: Default::default(),
    };
    c.emissions.push(e);
    let r = finish(s);
    let d = &search(&r).decisions[0];
    assert!(d.bundle.is_some());
    assert_eq!(d.candidates[d.selected.unwrap()].net, Some(11.into()));
    assert_eq!(r.state.balance("alice", "Q"), 10000.into());
    assert_eq!(r.state.accounts["alice"].nonce, Amount::ZERO);
    assert_eq!(r.state.balance("builder", "Q"), 120.into());
    assert!(
        r.blocks[0]
            .scheduler
            .as_ref()
            .unwrap()
            .probes
            .iter()
            .any(|p| p
                .receipts
                .iter()
                .any(|t| t.sender == "alice" && t.status == "ProtocolRevert"))
    );
}
#[test]
fn incoming_victim_transfer_is_excluded_from_conditional_net() {
    let mut s = fixture("public");
    s.scheduler.as_mut().unwrap().emissions[0]
        .transaction
        .program
        .push(Call::Transfer {
            to: "alice".into(),
            asset: "Q".into(),
            amount: 100.into(),
        });
    let r = finish(s);
    let d = &search(&r).decisions[0];
    assert_eq!(d.candidates[d.selected.unwrap()].net, Some(11.into()));
    assert_eq!(r.state.balance("alice", "Q"), 10111.into());
}
#[test]
fn own_pending_nonce_blocks_search_and_repeated_delivery_does_not_retry() {
    let mut s = fixture("public");
    let c = s.scheduler.as_mut().unwrap();
    let mut e = c.emissions[0].clone();
    e.at = Amount::ZERO;
    e.producer_sequence = 1.into();
    e.transaction.id = "own-inflight".into();
    e.transaction.agent = "trader".into();
    e.transaction.sender = "alice".into();
    e.transaction.program = vec![];
    e.delivery.builder_delay = 30.into();
    c.emissions.push(e);
    let mut duplicate = c.emissions[0].clone();
    duplicate.at = 9.into();
    duplicate.producer_sequence = 2.into();
    c.emissions.push(duplicate);
    let r = finish(s);
    let report = search(&r);
    assert_eq!(report.decisions.len(), 1);
    assert_eq!(report.decisions[0].reason, "BusyNonce");
    assert_eq!(report.metrics.prefixes, 0);
}
#[test]
fn evicted_visible_trigger_is_not_resolved_from_global_registry() {
    let mut s = fixture("public");
    policy(&mut s).reaction_delay = 2.into();
    let c = s.scheduler.as_mut().unwrap();
    c.limits.visible = 1;
    let mut e = c.emissions[0].clone();
    e.at = 8.into();
    e.producer_sequence = 1.into();
    e.transaction.id = "later-visible".into();
    e.transaction.nonce = 1.into();
    e.transaction.program = vec![];
    c.emissions.push(e);
    let r = finish(s);
    assert_eq!(search(&r).decisions[0].reason, "TriggerUnavailable");
    assert_eq!(search(&r).metrics.prefixes, 0);
    assert!(r.scheduler.as_ref().unwrap().known.contains_key("victim"));
}
#[test]
fn insufficient_capital_payment_resource_or_fee_reserve_do_not_create_bundle() {
    for payment in [false, true] {
        let mut s = fixture("public");
        policy(&mut s).inputs = vec![100.into()];
        if payment {
            policy(&mut s).payment_limit = 100.into();
        } else {
            s.accounts
                .get_mut("alice")
                .unwrap()
                .balances
                .insert("Q".into(), 1.into());
        }
        let r = finish(s);
        assert!(search(&r).decisions[0].bundle.is_none());
        assert_eq!(r.state.accounts["alice"].nonce, Amount::ZERO);
    }
    let mut s = fixture("fees");
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), 100500.into());
    policy(&mut s).payment_limit = 2000.into();
    policy(&mut s).payment_price = 100.into();
    let r = finish(s);
    let d = &search(&r).decisions[0];
    assert!(d.bundle.is_none());
    // Actual profit can pay 11500, but the declared 200000 fee reserve is unavailable.
    assert!(
        d.candidates
            .iter()
            .any(|c| c.outcome.contains("insufficient fee budget")
                && c.receipts.len() == 1
                && c.receipts[0].status == "Success")
    );
    assert_eq!(r.state.balance("alice", "Q"), 100500.into());
}
#[test]
fn reserved_policy_bundle_cannot_be_occupied_by_external_emission() {
    let mut s = fixture("public");
    let b = finish(s.clone());
    let candidate = search(&b).decisions[0].bundle.clone().unwrap();
    let c = s.scheduler.as_mut().unwrap();
    c.bundles
        .as_mut()
        .unwrap()
        .emissions
        .push(scheduler::bundles::Emission {
            at: Amount::ZERO,
            producer_id: "external".into(),
            producer_sequence: Amount::ZERO,
            bundle: candidate,
            delivery: c.agents["trader"].outgoing.clone(),
        });
    assert!(Run::new(s).is_err());
}

fn scheduled_protocol(
    mut s: Scenario,
    setup: Vec<Transaction>,
    victim: Transaction,
    agent: &str,
    account: &str,
) -> Scenario {
    let base = fixture("public");
    s.model_versions.retain(|m| m != "scheduler-m1-v1");
    for m in base.model_versions {
        if !s.model_versions.contains(&m) {
            s.model_versions.push(m);
        }
    }
    s.blocks.clear();
    s.policies.clear();
    s.chain.interval = 10.into();
    let mut c = base.scheduler.unwrap();
    let network = c.agents["trader"].clone();
    c.agents.clear();
    for a in s.agents.keys() {
        let mut n = network.clone();
        n.observe_public = a == agent;
        c.agents.insert(a.clone(), n);
    }
    let mut p = c
        .backrun
        .as_mut()
        .unwrap()
        .policies
        .remove("trader")
        .unwrap();
    p.account = account.into();
    c.backrun.as_mut().unwrap().policies.insert(agent.into(), p);
    let template = c.emissions[0].clone();
    c.emissions.clear();
    for (i, tx) in setup.into_iter().enumerate() {
        let mut e = template.clone();
        e.at = Amount::ZERO;
        e.producer_id = "setup".into();
        e.producer_sequence = (i as u64).into();
        e.transaction = tx;
        e.delivery = scheduler::Delivery {
            channel: Channel::Private {
                recipients: Default::default(),
            },
            builder_delay: Amount::ZERO,
        };
        c.emissions.push(e);
    }
    let mut e = template;
    e.at = if c.emissions.is_empty() {
        7.into()
    } else {
        17.into()
    };
    e.transaction = victim;
    c.emissions.push(e);
    s.scheduler = Some(c);
    s
}
#[test]
fn stable_solver_fees_use_actual_work_and_final_guard() {
    let mut s = Scenario::parse(include_bytes!("../../../scenarios/demos/sc-stable.json")).unwrap();
    s.accounts
        .get_mut("lp")
        .unwrap()
        .balances
        .insert("Q".into(), 1000000000u64.into());
    let victim = s.blocks[0][0].clone();
    let mut s = scheduled_protocol(s, vec![], victim, "lp", "lp");
    let p = s
        .scheduler
        .as_mut()
        .unwrap()
        .backrun
        .as_mut()
        .unwrap()
        .policies
        .get_mut("lp")
        .unwrap();
    p.inputs = vec![100000000u64.into()];
    p.resource_limit = 1000.into();
    p.resource_price = 1.into();
    p.payment_price = 1.into();
    let r = finish(s);
    let d = &search(&r).decisions[0];
    let candidate = &d.candidates[d.selected.unwrap()];
    assert!(candidate.receipts[0].used > 160.into());
    assert_eq!(candidate.receipts[0].fee, candidate.receipts[0].used);
    assert_eq!(
        r.state
            .balance("lp", "Q")
            .checked_sub(1000000000u64.into())
            .unwrap(),
        candidate.net.unwrap()
    );
    assert_eq!(
        r.state.balance("builder", "Q"),
        candidate.fees.checked_add(5.into()).unwrap()
    );
}
#[test]
fn clmm_and_book_partial_results_cannot_be_scored_as_closed_cycle_profit() {
    use routing::heterogeneous::{DirectionalLimit, VenueHop};
    let s = Scenario::parse(include_bytes!(
        "../../../scenarios/demos/sc-03-heterogeneous.json"
    ))
    .unwrap();
    let setup = s.blocks[0].clone();
    let mut victim = setup[1].clone();
    victim.id = "visible-empty".into();
    victim.nonce = 1.into();
    victim.program.clear();
    let mut s = scheduled_protocol(s, setup, victim, "trader", "trader");
    let p = policy(&mut s);
    p.inputs = vec![25.into(), 10000.into()];
    p.resource_limit = 1000.into();
    p.route_limits = vec![
        DirectionalLimit {
            asset_in: "Q".into(),
            hop: VenueHop::Clmm {
                venue: "clmm".into(),
                limit_tick: 100,
            },
        },
        DirectionalLimit {
            asset_in: "X".into(),
            hop: VenueHop::Book {
                venue: "book".into(),
                limit_price_ticks: 50.into(),
            },
        },
    ];
    let r = finish(s);
    let d = &r.blocks[1]
        .scheduler
        .as_ref()
        .unwrap()
        .search
        .as_ref()
        .unwrap()
        .decisions[0];
    let attempts: Vec<_> = d
        .candidates
        .iter()
        .filter(|c| c.hops.first().is_some_and(|h| h.venue() == "clmm"))
        .collect();
    assert_eq!(attempts.len(), 2);
    assert!(
        attempts
            .iter()
            .all(|c| c.net.is_none() && c.outcome == "ProtocolRevert")
    );
    assert!(d.bundle.is_none());
    assert_eq!(r.state.balance("trader", "Q"), 100000.into());
}
