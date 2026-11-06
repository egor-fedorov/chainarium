use chainarium_core::{
    policy::liquidator as l,
    scheduler::{Channel, Status},
    *,
};
fn fixture(name: &str) -> Scenario {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../scenarios/demos/sc-liquidator-{name}.json"));
    Scenario::parse(&std::fs::read(path).unwrap()).unwrap()
}
fn policy(s: &mut Scenario) -> &mut l::Policy {
    s.scheduler
        .as_mut()
        .unwrap()
        .liquidator
        .as_mut()
        .unwrap()
        .policies
        .get_mut("carol")
        .unwrap()
}
fn limits(s: &mut Scenario) -> &mut l::Limits {
    &mut s
        .scheduler
        .as_mut()
        .unwrap()
        .liquidator
        .as_mut()
        .unwrap()
        .limits
}
fn finish(s: Scenario) -> Run {
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    r
}
fn report(r: &Run, block: usize) -> &l::Report {
    r.blocks[block]
        .scheduler
        .as_ref()
        .unwrap()
        .liquidators
        .as_ref()
        .unwrap()
}
#[test]
fn own_policy_waits_for_committed_oracle_and_selects_analytic_repayment() {
    let r = finish(fixture("own"));
    assert!(report(&r, 0).decisions[0].transaction.is_none());
    assert!(report(&r, 1).decisions[0].transaction.is_none());
    let d = &report(&r, 2).decisions[0];
    assert_eq!(d.candidates.len(), 4);
    let c = &d.candidates[d.selected.unwrap()];
    assert_eq!(c.repayment, 60.into());
    assert_eq!(c.net, Some(24.into()));
    assert_eq!(r.state.balance("carol", "Q"), 1024.into());
    assert_eq!(r.blocks[2].receipts[0].used, 170.into());
    assert_eq!(r.state.lending["credit"].claims, 80.into());
    let view = d.observation.as_ref().unwrap();
    assert_eq!(view.parent_block, 2.into());
    assert_eq!(view.market.block, 3.into());
    assert_eq!(view.market.timestamp, 30.into());
    assert_eq!(
        view.targets[0].oracle.record.as_ref().unwrap().observed_at,
        20.into()
    );
}
#[test]
fn flash_principal_fee_and_native_fee_are_not_profit() {
    let r = finish(fixture("flash"));
    let d = &report(&r, 2).decisions[0];
    let c = &d.candidates[d.selected.unwrap()];
    assert_eq!(c.repayment, 60000.into());
    assert_eq!(c.net, Some(24675.into()));
    assert_eq!(c.receipt.as_ref().unwrap().used, 200.into());
    assert_eq!(r.state.balance("carol", "Q"), 25675.into());
    assert_eq!(r.state.balance("builder", "Q"), 200.into());
    assert_eq!(r.state.balance("flash", "Q"), 10000030u64.into());
    assert_eq!(
        d.candidates
            .iter()
            .map(|c| c.net.unwrap())
            .collect::<Vec<_>>(),
        vec![9201.into(), 17472.into(), 24675.into(), 24665.into()]
    );
}
#[test]
fn fifo_and_fee_priority_change_winner_and_charge_included_no_debt_loser() {
    for (name, alice, carol, builder, winner) in [
        ("fifo", 124735, 99760, 410, "alice"),
        ("fee", 99880, 124565, 460, "carol"),
    ] {
        let r = finish(fixture(name));
        assert_eq!(r.state.balance("alice", "Q"), alice.into());
        assert_eq!(r.state.balance("carol", "Q"), carol.into());
        assert_eq!(r.state.balance("builder", "Q"), builder.into());
        let block = &r.blocks[2];
        assert_eq!(block.receipts.len(), 2);
        assert_eq!(block.receipts[0].sender, winner);
        assert_eq!(block.receipts[0].used, 170.into());
        assert_eq!(block.receipts[1].error.as_deref(), Some("NoDebt"));
        assert_eq!(block.receipts[1].used, 120.into());
        assert_eq!(r.state.accounts["alice"].nonce, 1.into());
        assert_eq!(r.state.accounts["carol"].nonce, 1.into());
        assert_eq!(r.state.lending["credit"].losses.len(), 1);
        assert_eq!(r.state.lending["credit"].losses[0].amount, 20000.into());
        assert_eq!(report(&r, 2).metrics.trials, 8);
    }
}
#[test]
fn freshness_is_evaluated_at_preview_time_and_equality_is_allowed() {
    for (max_age, success) in [(9, false), (10, true)] {
        let mut s = fixture("stale");
        s.lending.get_mut("credit").unwrap().max_age = max_age.into();
        let r = finish(s);
        let d = &report(&r, 2).decisions[0];
        assert_eq!(d.transaction.is_some(), success);
        assert_eq!(
            d.observation.as_ref().unwrap().targets[0].status,
            if success {
                "Liquidatable"
            } else {
                "StaleOracle"
            }
        );
    }
}
#[test]
fn delayed_intent_blocks_duplicate_nonce_and_executes_later() {
    let r = finish(fixture("delayed"));
    assert!(r.blocks[2].receipts.is_empty());
    assert_eq!(report(&r, 3).decisions[0].reason, "BusyNonce");
    assert_eq!(r.blocks[3].receipts.len(), 1);
    assert_eq!(r.state.balance("carol", "Q"), 1024.into());
    assert_eq!(r.state.accounts["carol"].nonce, 1.into());
}
#[test]
fn own_capital_checks_actual_repayment_instead_of_requested_maximum() {
    let mut s = fixture("own");
    s.accounts
        .get_mut("carol")
        .unwrap()
        .balances
        .insert("Q".into(), 60.into());
    policy(&mut s).targets[0].repay_sizes = vec![80.into()];
    let r = finish(s);
    let d = &report(&r, 2).decisions[0];
    assert!(d.transaction.is_some());
    assert_eq!(r.state.balance("carol", "Q"), 84.into());
}
#[test]
fn flash_cannot_fund_native_reserve_and_provider_shortage_has_no_canonical_fee() {
    for native in [true, false] {
        let mut s = fixture("flash");
        if native {
            s.accounts
                .get_mut("carol")
                .unwrap()
                .balances
                .insert("Q".into(), 499.into());
        } else {
            s.accounts
                .get_mut("flash")
                .unwrap()
                .balances
                .insert("Q".into(), 1.into());
        }
        let r = finish(s);
        assert!(report(&r, 2).decisions[0].transaction.is_none());
        assert!(r.blocks[2].receipts.is_empty());
        assert_eq!(r.state.accounts["carol"].nonce, Amount::ZERO);
        assert_eq!(r.state.balance("builder", "Q"), Amount::ZERO);
    }
}
#[test]
fn trial_and_venue_limits_mark_unexplored_candidates_and_can_submit_best_completed() {
    let mut s = fixture("own");
    limits(&mut s).trials_per_policy = 1;
    let r = finish(s);
    let d = &report(&r, 2).decisions[0];
    assert!(d.truncated);
    assert_eq!(d.candidates.len(), 1);
    assert_eq!(d.candidates[0].net, Some(9.into()));
    assert!(d.transaction.is_some());
    let mut s = fixture("own");
    limits(&mut s).venue_checks = 1;
    let r = finish(s);
    assert_eq!(report(&r, 2).metrics.venue_checks, 1);
    assert!(report(&r, 2).metrics.truncated);
}
#[test]
fn round_trial_budget_is_shared_in_agent_id_order() {
    let mut s = fixture("fifo");
    limits(&mut s).trials_per_round = 4;
    let r = finish(s);
    let report = report(&r, 2);
    assert_eq!(report.metrics.trials, 4);
    assert_eq!(report.decisions[0].agent, "alice");
    assert!(report.decisions[0].transaction.is_some());
    assert_eq!(report.decisions[1].reason, "SearchLimit");
    assert_eq!(r.state.balance("carol", "Q"), 100000.into());
}
#[test]
fn target_and_repayment_config_order_does_not_change_search() {
    let a = finish(fixture("own"));
    let mut s = fixture("own");
    policy(&mut s).targets[0].repay_sizes.reverse();
    let b = finish(s);
    assert_eq!(report(&a, 2), report(&b, 2));
    assert_eq!(a.state, b.state);
}
#[test]
fn canonical_recheck_can_revert_late_unprofitable_sale_without_committing_loss() {
    let mut s = fixture("fifo");
    s.scheduler
        .as_mut()
        .unwrap()
        .liquidator
        .as_mut()
        .unwrap()
        .policies
        .remove("carol");
    s.accounts
        .get_mut("carol")
        .unwrap()
        .balances
        .insert("X".into(), 150000.into());
    let c = s.scheduler.as_mut().unwrap();
    let mut e = c.emissions[0].clone();
    e.at = 21.into();
    e.producer_sequence = 3.into();
    e.transaction = Transaction {
        id: "dump-before-liquidation".into(),
        agent: "carol".into(),
        sender: "carol".into(),
        chain: "lab-1".into(),
        nonce: Amount::ZERO,
        limit: 500.into(),
        price: Amount::ZERO,
        deadline: 100.into(),
        program: vec![Call::SwapExactIn {
            pool: "sale".into(),
            asset_in: "X".into(),
            amount: 150000.into(),
            min_out: 900000.into(),
        }],
    };
    e.delivery.channel = Channel::Private {
        recipients: Default::default(),
    };
    c.emissions.push(e);
    c.agents.get_mut("alice").unwrap().outgoing.builder_delay = 2.into();
    c.rounds = 3;
    let r = finish(s);
    let d = &report(&r, 2).decisions[0];
    assert_eq!(d.candidates[d.selected.unwrap()].net, Some(24735.into()));
    assert_eq!(r.blocks[2].receipts[1].status, "ProtocolRevert");
    assert_eq!(r.state.balance("alice", "Q"), 99830.into());
    assert_eq!(r.state.lending["credit"].debt, 80000.into());
    assert!(r.state.lending["credit"].losses.is_empty());
    assert_eq!(r.state.balance("escrow-bob", "X"), 6000.into());
}
#[test]
fn checkpoint_with_inflight_liquidation_replays_and_tampered_search_is_rejected() {
    let mut r = Run::new(fixture("delayed")).unwrap();
    for _ in 0..3 {
        r.step().unwrap();
    }
    let mut resumed = Run::restore(&r.checkpoint().unwrap()).unwrap();
    let mut bad = r.clone();
    bad.blocks[2]
        .scheduler
        .as_mut()
        .unwrap()
        .liquidators
        .as_mut()
        .unwrap()
        .metrics
        .trials += 1;
    assert!(Run::restore(&bad.checkpoint().unwrap()).is_err());
    r.finish().unwrap();
    resumed.finish().unwrap();
    assert_eq!(r, resumed);
}
#[test]
fn exhausted_nonce_or_unrepresentable_lifetime_only_skip_own_policy() {
    let max: Amount =
        "115792089237316195423570985008687907853269984665640564039457584007913129639935"
            .parse()
            .unwrap();
    for nonce in [true, false] {
        let mut s = fixture("own");
        if nonce {
            s.accounts.get_mut("carol").unwrap().nonce = max;
        } else {
            policy(&mut s).lifetime = max;
        }
        let r = finish(s);
        assert_eq!(
            report(&r, 2).decisions[0].reason,
            if nonce {
                "NonceExhausted"
            } else {
                "DeadlineOverflow"
            }
        );
        assert_eq!(r.blocks[0].receipts.len(), 2);
        assert_eq!(r.blocks[1].receipts.len(), 1);
    }
}
#[test]
fn expired_native_intent_is_not_refreshed_or_charged() {
    let mut s = fixture("delayed");
    policy(&mut s).lifetime = 10.into();
    let r = finish(s);
    assert!(r.blocks[2].receipts.is_empty());
    assert!(r.blocks[3].receipts.is_empty());
    assert_eq!(r.state.balance("carol", "Q"), 1000.into());
    assert!(
        r.scheduler
            .as_ref()
            .unwrap()
            .known
            .values()
            .any(|k| k.transaction.agent == "carol" && matches!(k.status, Status::Rejected { .. }))
    );
}
#[test]
fn invalid_targets_sizes_models_and_role_combinations_are_rejected() {
    for kind in 0..7 {
        let mut s = fixture("own");
        match kind {
            0 => {
                s.model_versions.retain(|m| m != l::MODEL);
            }
            1 => policy(&mut s).targets[0].repay_sizes.push(20.into()),
            2 => policy(&mut s).targets[0].borrower = "unknown".into(),
            3 => policy(&mut s).account = "builder".into(),
            4 => limits(&mut s).trials_per_round = 65,
            5 => policy(&mut s).minimum_net_profit = Amount::ZERO,
            _ => {
                s.agents
                    .get_mut("carol")
                    .unwrap()
                    .controlled_accounts
                    .insert("bob".into());
            }
        }
        assert!(Run::new(s).is_err(), "case {kind}");
    }
}

#[test]
fn all_trials_share_one_accrual_preview_and_canonical_round_accrues_once() {
    let mut s = fixture("own");
    s.lending.get_mut("credit").unwrap().rate = credit::Rate::Constant {
        rate: "12500000000000000".parse().unwrap(),
    };
    let r = finish(s);
    assert_eq!(r.blocks[1].state.lending["credit"].debt, 90.into());
    let d = &report(&r, 2).decisions[0];
    assert_eq!(d.observation.as_ref().unwrap().targets[0].debt, 101.into());
    assert_eq!(d.candidates.len(), 4);
    for c in &d.candidates {
        assert_eq!(
            c.receipt.as_ref().unwrap().liquidations["0"].paid,
            c.repayment.min(60.into())
        );
    }
    let l = &r.blocks[2].state.lending["credit"];
    assert_eq!(l.losses[0].amount, 41.into());
    assert_eq!(
        (l.debt, l.claims, l.equity),
        (0.into(), 80.into(), 0.into())
    );
    assert_eq!(r.state.balance("reserve", "Q"), 80.into());
}
#[test]
fn current_private_and_future_emissions_cannot_change_the_periodic_decision() {
    let a = finish(fixture("own"));
    let mut s = fixture("own");
    let c = s.scheduler.as_mut().unwrap();
    let mut hidden = c.emissions[2].clone();
    hidden.at = 21.into();
    hidden.producer_sequence = 3.into();
    hidden.transaction.id = "private-price-recovery".into();
    hidden.transaction.nonce = 1.into();
    if let Call::PublishOracle {
        price,
        observed_at,
        sequence,
        update_id,
        ..
    } = &mut hidden.transaction.program[0]
    {
        *price = amount::Price::new("X".into(), "Q".into(), 30.into(), 1.into()).unwrap();
        *observed_at = 21.into();
        *sequence = 2.into();
        *update_id = "private".into();
    }
    hidden.delivery.channel = Channel::Private {
        recipients: Default::default(),
    };
    c.emissions.push(hidden.clone());
    hidden.at = 31.into();
    hidden.producer_sequence = 4.into();
    hidden.transaction.id = "future-recovery".into();
    hidden.transaction.nonce = 2.into();
    if let Call::PublishOracle {
        observed_at,
        sequence,
        update_id,
        ..
    } = &mut hidden.transaction.program[0]
    {
        *observed_at = 31.into();
        *sequence = 3.into();
        *update_id = "future".into();
    }
    c.emissions.push(hidden);
    let b = finish(s);
    assert_eq!(report(&a, 2), report(&b, 2));
    assert_eq!(a.blocks[2].receipts[0], b.blocks[2].receipts[0]);
}
#[test]
fn hard_trial_and_venue_caps_stop_without_claiming_unexplored_profitability() {
    let mut s = fixture("own");
    policy(&mut s).targets[0].repay_sizes = (10..=80).step_by(10).map(Amount::from).collect();
    for i in 0..8 {
        let id = format!("sale-{i}");
        let mut p = s.protocols["sale"].clone();
        p.account = id.clone();
        s.accounts.insert(id.clone(), s.accounts["sale"].clone());
        s.protocols.insert(id, p);
    }
    let r = finish(s);
    let q = report(&r, 2);
    assert_eq!(q.metrics.trials, 64);
    assert!(q.metrics.truncated);
    assert_eq!(q.decisions[0].candidates.len(), 64);
    assert!(q.decisions[0].transaction.is_some());
    let mut s = fixture("own");
    s.protocols.clear();
    s.assets.insert("R".into(), Asset { decimals: 0 });
    for i in 0..20 {
        let id = format!("dead-{i:02}");
        s.accounts.insert(
            id.clone(),
            Account {
                nonce: 0.into(),
                balances: [("X".into(), 1.into()), ("R".into(), 1.into())].into(),
            },
        );
        s.protocols.insert(
            id.clone(),
            Pool {
                account: id,
                x: "X".into(),
                y: "R".into(),
                fee_bps: 0,
                owner: "lender".into(),
            },
        );
    }
    let r = finish(s);
    let q = report(&r, 2);
    assert_eq!(q.metrics.venue_checks, 4096);
    assert_eq!(q.metrics.trials, 0);
    assert!(q.metrics.truncated);
    assert_eq!(q.decisions[0].reason, "SearchLimit");
}
#[test]
fn maximum_target_and_size_sets_are_valid_and_next_item_is_rejected() {
    let mut s = fixture("own");
    for i in 0..15 {
        let id = format!("borrower-{i}");
        let escrow = format!("escrow-{i}");
        s.accounts.insert(id.clone(), s.accounts["alice"].clone());
        s.accounts
            .insert(escrow.clone(), s.accounts["escrow-bob"].clone());
        s.lending
            .get_mut("credit")
            .unwrap()
            .positions
            .insert(id.clone(), escrow);
        policy(&mut s).targets.push(l::Target {
            market: "credit".into(),
            borrower: id,
            repay_sizes: vec![1.into()],
        });
    }
    policy(&mut s).targets[0].repay_sizes = (1..=8).map(Amount::from).collect();
    let a = finish(s.clone());
    policy(&mut s).targets.reverse();
    let b = finish(s.clone());
    assert_eq!(report(&a, 2), report(&b, 2));
    let mut over = s.clone();
    let extra = policy(&mut over).targets[0].clone();
    policy(&mut over).targets.push(extra);
    assert!(Run::new(over).is_err());
    let t = policy(&mut s)
        .targets
        .iter_mut()
        .find(|t| t.borrower == "bob")
        .unwrap();
    t.repay_sizes.push(9.into());
    assert!(Run::new(s).is_err());
}
#[test]
fn close_factor_reprices_after_competing_partial_liquidation() {
    let mut s = fixture("fifo");
    s.lending.get_mut("credit").unwrap().close_bps = 5000;
    let r = finish(s);
    let rs = &r.blocks[2].receipts;
    assert_eq!(rs.len(), 2);
    assert!(rs.iter().all(|r| r.status == "Success"));
    assert_eq!(rs[0].liquidations["0"].paid, 40000.into());
    assert_eq!(rs[1].liquidations["0"].paid, 20000.into());
    assert_eq!(rs[1].liquidations["0"].seized, 2000.into());
    assert_eq!(
        r.blocks[2].state.lending["credit"].losses[0].amount,
        20000.into()
    );
}
