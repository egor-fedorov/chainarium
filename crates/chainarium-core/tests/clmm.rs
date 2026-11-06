use chainarium_core::{
    clmm::{self, math::*},
    engine, *,
};
use std::sync::Arc;

fn scenario() -> Scenario {
    Scenario::parse(include_bytes!("../../../scenarios/demos/sc-02-clmm.json")).unwrap()
}
fn cross_scenario() -> Scenario {
    Scenario::parse(include_bytes!(
        "../../../scenarios/demos/sc-clmm-crossing.json"
    ))
    .unwrap()
}
fn tx(w: &World, who: &str, calls: Vec<Call>) -> Transaction {
    Transaction {
        id: format!("{}-{who}", w.block),
        agent: who.into(),
        sender: who.into(),
        chain: "lab-1".into(),
        nonce: w.accounts[who].nonce,
        limit: 1000.into(),
        price: Amount::ZERO,
        deadline: Amount::MAX,
        program: calls,
    }
}
fn apply(r: &Run, who: &str, calls: Vec<Call>) -> Block {
    engine::build_block(&r.scenario, &r.state, &[tx(&r.state, who, calls)]).unwrap()
}
fn funded(s: Scenario) -> Run {
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert!(r.blocks[0].receipts.iter().all(|r| r.status == "Success"));
    r
}
fn mint(lower: i32, upper: i32, l: u64) -> Call {
    Call::MintPosition {
        pool: "clmm".into(),
        lower,
        upper,
        liquidity: l.into(),
        max_x: Amount::MAX,
        max_q: Amount::MAX,
    }
}
fn burn(id: u64, l: u64) -> Call {
    Call::BurnPosition {
        pool: "clmm".into(),
        position_id: id.into(),
        liquidity: l.into(),
        min_x: Amount::ZERO,
        min_q: Amount::ZERO,
    }
}
fn collect(id: u64) -> Call {
    Call::CollectPosition {
        pool: "clmm".into(),
        position_id: id.into(),
    }
}
fn swap(asset: &str, n: u64, limit: i32, partial: bool) -> Call {
    Call::ClmmSwap {
        pool: "clmm".into(),
        asset_in: asset.into(),
        amount: n.into(),
        sqrt_price_limit: tick_sqrt(limit).unwrap(),
        min_out: 1.into(),
        allow_partial: partial,
    }
}
fn trace(b: &Block) -> &clmm::Trace {
    b.receipts[0].trace[0].clmm.as_ref().unwrap()
}
fn rollback(r: &Run, b: &Block) {
    let mut control = engine::build_block(&r.scenario, &r.state, &[])
        .unwrap()
        .state;
    let receipt = &b.receipts[0];
    let a = Arc::make_mut(&mut control.accounts);
    let sender = a.get_mut(&receipt.sender).unwrap();
    sender.nonce = sender.nonce.checked_add(1.into()).unwrap();
    sender.balances.insert(
        "Q".into(),
        sender.balances["Q"].checked_sub(receipt.fee).unwrap(),
    );
    let builder = a.get_mut("builder").unwrap();
    builder.balances.insert(
        "Q".into(),
        builder.balances["Q"].checked_add(receipt.fee).unwrap(),
    );
    assert_eq!(b.state, control);
    assert!(receipt.events.is_empty());
}

#[test]
fn new_range_with_mixed_tick_history_has_signed_checkpoint_and_no_old_fees() {
    let mut r = Run::new(scenario()).unwrap();
    for (who, calls) in [
        ("alice", vec![mint(0, 100, 1_000_000)]),
        ("trader", vec![swap("Q", 6000, 200, true)]),
        ("alice", vec![mint(-100, 0, 1_000_000)]),
    ] {
        let b = apply(&r, who, calls);
        assert_eq!(b.receipts[0].status, "Success");
        r.state = b.state;
    }
    let state = &r.state.clmm["clmm"];
    let checkpoint = state.positions[&2.into()].last_inside[1];
    assert!(checkpoint.negative);
    assert_eq!(checkpoint.magnitude, state.fee_growth[1]);
    let b = apply(&r, "alice", vec![collect(2)]);
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(trace(&b).amounts, [Amount::ZERO; 2]);
    r.state = b.state;
    // Cross both ranges in the other direction, then collect and burn. The
    // new position earns only X fees accumulated while it was active.
    let b = apply(&r, "trader", vec![swap("X", 11000, -200, true)]);
    assert_eq!(b.receipts[0].status, "Success");
    r.state = b.state;
    let b = apply(&r, "alice", vec![collect(2), burn(2, 1_000_000)]);
    assert_eq!(b.receipts[0].status, "Success");
    assert!(trace(&b).amounts[0] > Amount::ZERO);
    assert_eq!(trace(&b).amounts[1], Amount::ZERO);
    b.state.invariant(&r.scenario).unwrap();
}

#[test]
fn mixed_mints_crossings_collects_and_burns_preserve_backing() {
    let mut r = Run::new(scenario()).unwrap();
    let mut seed = 721398u64;
    let mut successes = 0;
    for _ in 0..600 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let choice = seed % 8;
        let mut who = "alice";
        let call = if choice < 3 {
            let lower = ((seed >> 10) % 41) as i32 * 10 - 200;
            let upper = lower + 10 + ((seed >> 25) % 25) as i32 * 10;
            mint(lower, upper, (seed >> 40) % 1_000_000 + 1)
        } else if choice < 5 && !r.state.clmm["clmm"].positions.is_empty() {
            let positions = &r.state.clmm["clmm"].positions;
            let (id, p) = positions
                .iter()
                .nth((seed >> 30) as usize % positions.len())
                .unwrap();
            if choice == 3 && !p.liquidity.is_zero() {
                Call::BurnPosition {
                    pool: "clmm".into(),
                    position_id: *id,
                    liquidity: p.liquidity,
                    min_x: Amount::ZERO,
                    min_q: Amount::ZERO,
                }
            } else {
                Call::CollectPosition {
                    pool: "clmm".into(),
                    position_id: *id,
                }
            }
        } else {
            who = "trader";
            let down = seed.is_multiple_of(2);
            swap(
                if down { "X" } else { "Q" },
                (seed >> 30) % 10000 + 1,
                if down { -300 } else { 500 },
                true,
            )
        };
        let b = apply(&r, who, vec![call]);
        successes += usize::from(b.receipts[0].status == "Success");
        b.state.invariant(&r.scenario).unwrap();
        r.state = b.state;
    }
    assert!(successes > 300);
}

#[test]
fn pinned_single_range_swap_burn_collect_and_surplus() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/reviews/clmm-reference.json")).unwrap();
    let expected = |key: &str| fixture[key].as_str().unwrap().parse::<Amount>().unwrap();
    let mut r = funded(scenario());
    assert_eq!(r.state.balance("clmm-account", "X"), expected("mint_x"));
    assert_eq!(r.state.balance("clmm-account", "Q"), expected("mint_y"));
    let b = apply(&r, "trader", vec![swap("X", 1000, -100, false)]);
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(trace(&b).output, expected("output"));
    assert_eq!(trace(&b).fee, expected("input_fee"));
    assert_eq!(b.state.clmm["clmm"].sqrt_price, expected("sqrt_after"));
    assert_eq!(
        b.state.clmm["clmm"].fee_growth[0],
        expected("fee_growth_x128")
    );
    r.state = b.state;
    let b = apply(&r, "alice", vec![burn(1, 1000000), collect(1)]);
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(trace(&b).amounts, [expected("burn_x"), expected("burn_y")]);
    assert_eq!(
        b.receipts[0].trace[1].clmm.as_ref().unwrap().amounts,
        [expected("collect_x"), Amount::ZERO]
    );
    assert_eq!(b.state.balance("clmm-account", "X"), expected("surplus_x"));
    assert_eq!(b.state.balance("clmm-account", "Q"), expected("surplus_y"));
    assert!(b.state.clmm["clmm"].ticks.is_empty());
    assert!(b.state.clmm["clmm"].positions.is_empty());
    b.state.invariant(&r.scenario).unwrap();
}
#[test]
fn pinned_two_crossings_allocate_fees_before_cross_and_leave_third_active() {
    let mut r = funded(cross_scenario());
    r.step().unwrap();
    let receipt = &r.blocks[1].receipts[0];
    assert_eq!(receipt.status, "Success");
    let t = receipt.trace[0].clmm.as_ref().unwrap();
    assert_eq!(t.output, 15755.into());
    assert_eq!(
        t.steps
            .iter()
            .map(|s| (s.gross, s.output, s.fee))
            .collect::<Vec<_>>(),
        vec![
            (5029.into(), 4987.into(), 16.into()),
            (10106.into(), 9924.into(), 31.into()),
            (865.into(), 844.into(), 3.into())
        ]
    );
    assert_eq!(
        t.steps
            .iter()
            .filter_map(|s| s.crossed_tick)
            .collect::<Vec<_>>(),
        vec![-100, -200]
    );
    assert_eq!(r.state.clmm["clmm"].liquidity, 3000000.into());
    assert_eq!(
        r.state.clmm["clmm"].sqrt_price,
        "78417560549529315000367649697".parse().unwrap()
    );
    for (who, id, n) in [("alice", 1, 15), ("bob", 2, 30), ("carol", 3, 2)] {
        let b = apply(&r, who, vec![collect(id)]);
        assert_eq!(trace(&b).amounts, [n.into(), 0.into()]);
        r.state = b.state;
    }
}
#[test]
fn exact_tick_math_bounds_and_inverse_near_boundaries() {
    assert_eq!(
        tick_sqrt(-100).unwrap(),
        "78833030112140176575862854578".parse().unwrap()
    );
    assert_eq!(
        tick_sqrt(100).unwrap(),
        "79625275426524748796330556127".parse().unwrap()
    );
    for t in [
        MIN_TICK, -30000, -200, -100, -1, 0, 1, 100, 200, 30000, MAX_TICK,
    ] {
        let p = tick_sqrt(t).unwrap();
        assert_eq!(tick_at(p).unwrap(), t);
        if t > MIN_TICK {
            assert_eq!(tick_at(p.checked_sub(1.into()).unwrap()).unwrap(), t - 1);
        }
        if t < MAX_TICK {
            assert_eq!(tick_at(p.checked_add(1.into()).unwrap()).unwrap(), t);
        }
    }
    assert!(tick_sqrt(MIN_TICK - 1).is_err());
    assert!(tick_sqrt(MAX_TICK + 1).is_err());
    assert!(tick_at(Amount::ZERO).is_err());
    assert!(growth(Amount::MAX, 1.into()).is_err());
}
#[test]
fn exact_boundary_crosses_even_when_input_is_exhausted() {
    let r = funded(scenario());
    let b = apply(&r, "trader", vec![swap("X", 5029, -100, false)]);
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(trace(&b).remainder, 0.into());
    assert_eq!(b.state.clmm["clmm"].current_tick, -101);
    assert_eq!(b.state.clmm["clmm"].liquidity, 0.into());
    let mut after = r.clone();
    after.state = b.state;
    let reverse = apply(&after, "trader", vec![swap("Q", 100, 100, false)]);
    assert_eq!(reverse.receipts[0].status, "Success");
    assert_eq!(trace(&reverse).steps[0].crossed_tick, Some(-100));
    assert_eq!(trace(&reverse).steps[0].gross, 0.into());
    assert_eq!(reverse.state.clmm["clmm"].liquidity, 1000000.into());
}
#[test]
fn explicit_partial_policy_and_no_liquidity_do_not_spend_unfilled_input() {
    let r = funded(scenario());
    let b = apply(&r, "trader", vec![swap("X", 10000, -100, false)]);
    assert_eq!(
        b.receipts[0].error.as_deref(),
        Some("CLMM partial fill disabled")
    );
    rollback(&r, &b);
    let b = apply(&r, "trader", vec![swap("X", 10000, -100, true)]);
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(
        (trace(&b).spent, trace(&b).remainder, trace(&b).output),
        (5029.into(), 4971.into(), 4987.into())
    );
    assert_eq!(b.state.balance("trader", "X"), 94971.into());
    let mut empty = r.clone();
    empty.state = b.state;
    let b = apply(&empty, "trader", vec![swap("X", 100, -200, true)]);
    assert_eq!(b.receipts[0].error.as_deref(), Some("NoExecutableOutput"));
    rollback(&empty, &b);
}
#[test]
fn gap_crossing_has_zero_input_output_and_no_fee_without_active_liquidity() {
    let mut r = funded(scenario());
    r.state = apply(&r, "bob", vec![mint(-400, -300, 2000000)]).state;
    let b = apply(&r, "trader", vec![swap("X", 20000, -400, true)]);
    assert_eq!(b.receipts[0].status, "Success");
    let gap = trace(&b)
        .steps
        .iter()
        .find(|s| s.liquidity_before.is_zero())
        .unwrap();
    assert_eq!(gap.crossed_tick, Some(-300));
    assert_eq!(
        (gap.gross, gap.fee, gap.output),
        (0.into(), 0.into(), 0.into())
    );
    assert_eq!(gap.liquidity_after, 2000000.into());
    b.state.invariant(&r.scenario).unwrap();
}
#[test]
fn late_failure_and_every_resource_boundary_restore_price_positions_and_growth() {
    let r = funded(cross_scenario());
    let call = swap("X", 16000, -300, false);
    let b = apply(
        &r,
        "trader",
        vec![
            call.clone(),
            Call::AssertCondition {
                condition: false,
                message: "after CLMM crossing".into(),
            },
        ],
    );
    rollback(&r, &b);
    let used: u64 = apply(&r, "trader", vec![call.clone()]).receipts[0]
        .used
        .to_string()
        .parse()
        .unwrap();
    for limit in 100..used {
        let mut t = tx(&r.state, "trader", vec![call.clone()]);
        t.limit = limit.into();
        t.price = 1.into();
        let b = engine::build_block(&r.scenario, &r.state, &[t]).unwrap();
        assert_eq!(b.receipts[0].status, "ResourceExhausted");
        rollback(&r, &b);
        assert_eq!(b.receipts[0].fee, limit.into());
    }
}
#[test]
fn new_position_cannot_collect_historical_fees_and_partial_burn_crystallizes_old_fees() {
    let mut r = funded(scenario());
    r.state = apply(&r, "trader", vec![swap("X", 1000, -100, false)]).state;
    r.state = apply(&r, "bob", vec![mint(-100, 100, 1000000)]).state;
    let b = apply(&r, "bob", vec![collect(2)]);
    assert_eq!(trace(&b).amounts, [0.into(), 0.into()]);
    r.state = b.state;
    let b = apply(&r, "alice", vec![burn(1, 500000)]);
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(
        b.state.clmm["clmm"].positions[&1.into()].owed,
        [2.into(), 0.into()]
    );
    assert_eq!(b.state.clmm["clmm"].liquidity, 1500000.into());
}
#[test]
fn collect_frequency_exposes_declared_fraction_loss_without_losing_tokens() {
    let mut often = funded(scenario());
    let mut once = often.clone();
    for _ in 0..4 {
        often.state = apply(&often, "trader", vec![swap("X", 500, -100, false)]).state;
        once.state = apply(&once, "trader", vec![swap("X", 500, -100, false)]).state;
        often.state = apply(&often, "alice", vec![collect(1)]).state;
    }
    let b = apply(&once, "alice", vec![collect(1)]);
    assert_eq!(trace(&b).amounts, [7.into(), 0.into()]);
    once.state = b.state;
    assert_eq!(often.state.balance("alice", "X"), 95016.into());
    assert_eq!(once.state.balance("alice", "X"), 95019.into());
    assert_eq!(
        often.state.clmm["clmm"].sqrt_price,
        once.state.clmm["clmm"].sqrt_price
    );
    often.state.invariant(&often.scenario).unwrap();
    once.state.invariant(&once.scenario).unwrap();
}
#[test]
fn ownership_minima_zero_outputs_and_protected_account() {
    let r = funded(scenario());
    for (who, call) in [
        ("bob", burn(1, 1)),
        ("bob", collect(1)),
        ("alice", burn(1, 0)),
        ("alice", burn(1, 1000001)),
        (
            "alice",
            Call::MintPosition {
                pool: "clmm".into(),
                lower: -100,
                upper: 100,
                liquidity: 1000000.into(),
                max_x: 1.into(),
                max_q: Amount::MAX,
            },
        ),
        (
            "trader",
            Call::ClmmSwap {
                pool: "clmm".into(),
                asset_in: "X".into(),
                amount: 1000.into(),
                sqrt_price_limit: tick_sqrt(-100).unwrap(),
                min_out: 997.into(),
                allow_partial: false,
            },
        ),
        (
            "alice",
            Call::Transfer {
                to: "clmm-account".into(),
                asset: "X".into(),
                amount: 1.into(),
            },
        ),
    ] {
        let b = apply(&r, who, vec![call]);
        assert_eq!(b.receipts[0].status, "ProtocolRevert");
        rollback(&r, &b);
    }
    let b = apply(&r, "alice", vec![burn(1, 1)]);
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(trace(&b).amounts, [0.into(), 0.into()]);
}
#[test]
fn both_demos_replay_every_committed_boundary() {
    for s in [scenario(), cross_scenario()] {
        let mut r = Run::new(s).unwrap();
        while r.step().unwrap().is_some() {
            assert_eq!(Run::restore(&r.checkpoint().unwrap()).unwrap(), r);
        }
        assert!(r.blocks.iter().all(|b| b.rejected.is_empty()));
    }
}

#[test]
fn read_only_views_and_quotes_keep_exact_state_and_surplus() {
    let mut r = funded(scenario());
    let before = r.state.clone();
    let t = tx(&r.state, "trader", vec![swap("X", 1000, -100, false)]);
    let q = clmm::quote(&r.scenario, &r.state, &t).unwrap();
    assert_eq!(
        q.receipt,
        apply(&r, "trader", t.program.clone()).receipts[0]
    );
    let v = clmm::views(&r.scenario, &r.state).unwrap();
    assert_eq!(v[0].rounding_surplus, [1.into(), 1.into()]);
    assert_eq!(r.state, before);
    r.state = apply(&r, "trader", t.program).state;
    let v = clmm::views(&r.scenario, &r.state).unwrap();
    assert_eq!(v[0].positions[0].collectable, [2.into(), 0.into()]);
    assert_eq!(v[0].rounding_surplus, [2.into(), 1.into()]);
    let old = q.state_ref;
    r.state = apply(&r, "alice", vec![burn(1, 1000000)]).state;
    let q = clmm::quote(
        &r.scenario,
        &r.state,
        &tx(&r.state, "trader", vec![swap("X", 1000, -100, false)]),
    )
    .unwrap();
    assert_ne!(q.state_ref, old);
    assert_eq!(q.receipt.error.as_deref(), Some("NoExecutableOutput"));
}

#[test]
fn declared_config_and_user_domains_are_checked() {
    for variant in 0..5 {
        let mut s = scenario();
        let c = s.clmm_pools.get_mut("clmm").unwrap();
        match variant {
            0 => c.initial_sqrt = Amount::ZERO,
            1 => c.initial_sqrt = tick_sqrt(MAX_TICK).unwrap().checked_add(1.into()).unwrap(),
            2 => c.fee_bps = 10000,
            3 => c.account = "alice".into(),
            _ => s.model_versions.retain(|v| v != clmm::MODEL),
        }
        assert!(s.validate().is_err());
    }
    let r = funded(scenario());
    for call in [
        mint(MIN_TICK - 1, 0, 1),
        mint(0, MAX_TICK + 1, 1),
        mint(100, 100, 1),
        mint(-100, 100, 0),
        swap("X", 0, -100, false),
        swap("X", 1, 100, false),
        swap("Q", 1, -100, false),
    ] {
        let b = apply(&r, "alice", vec![call]);
        assert_eq!(b.receipts[0].status, "ProtocolRevert");
        rollback(&r, &b);
    }
    let b = apply(&r, "trader", vec![swap("X", 1, -100, false)]);
    assert_eq!(b.receipts[0].error.as_deref(), Some("NoExecutableOutput"));
    rollback(&r, &b);
    let mut state = r.state.clone();
    Arc::make_mut(&mut state.clmm)
        .get_mut("clmm")
        .unwrap()
        .positions
        .get_mut(&1.into())
        .unwrap()
        .owed[0] = Amount::MAX;
    assert!(state.invariant(&r.scenario).is_err());
}

#[test]
fn capacity_1024_positions_2048_ticks_and_liquidity_overflow() {
    let mut s = scenario();
    s.blocks.clear();
    let mut r = Run::new(s).unwrap();
    for batch in 0..4 {
        let calls = (0..256)
            .map(|i| {
                let lower = (batch * 256 + i) * 2;
                mint(lower, lower + 1, 1)
            })
            .collect();
        let mut t = tx(&r.state, "alice", calls);
        t.limit = 5000.into();
        let b = engine::build_block(&r.scenario, &r.state, &[t]).unwrap();
        assert_eq!(b.receipts[0].status, "Success");
        r.state = b.state;
    }
    assert_eq!(
        (
            r.state.clmm["clmm"].positions.len(),
            r.state.clmm["clmm"].ticks.len()
        ),
        (1024, 2048)
    );
    let b = apply(&r, "alice", vec![mint(-1, 1, 1)]);
    assert_eq!(b.receipts[0].error.as_deref(), Some("CLMM capacity"));
    rollback(&r, &b);
    r.state = apply(&r, "alice", vec![burn(1, 1), mint(6000, 6001, 1)]).state;
    assert_eq!(r.state.clmm["clmm"].next_id, 1026.into());
    let mut s = scenario();
    s.blocks.clear();
    for account in s.accounts.values_mut() {
        for n in account.balances.values_mut() {
            *n = Amount::ZERO;
        }
    }
    let supply = Amount::MAX.mul_div(1.into(), 2.into(), false).unwrap();
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("X".into(), supply);
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), supply);
    let mut r = Run::new(s).unwrap();
    let mut call = mint(-1, 1, 1);
    if let Call::MintPosition { liquidity, .. } = &mut call {
        *liquidity = Amount::MAX;
    }
    let b = apply(&r, "alice", vec![call]);
    assert_eq!(b.receipts[0].status, "Success");
    r.state = b.state;
    let b = apply(&r, "alice", vec![mint(-1, 1, 1)]);
    assert_eq!(b.receipts[0].status, "ProtocolRevert");
    rollback(&r, &b);
}

#[test]
fn deterministic_two_way_swap_sweep_preserves_claim_backing() {
    let mut seed = 19u64;
    let mut successes = 0;
    let mut reverts = 0;
    for fee in [0, 30, 300, 9999] {
        let mut s = cross_scenario();
        s.clmm_pools.get_mut("clmm").unwrap().fee_bps = fee;
        for n in s.accounts.get_mut("trader").unwrap().balances.values_mut() {
            *n = 1000000.into();
        }
        let mut r = funded(s);
        for i in 0..48 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let amount = 1 + (seed >> 32) % 20000;
            let (asset, other, limit) = if i % 2 == 0 {
                ("X", "Q", -300)
            } else {
                ("Q", "X", 100)
            };
            let b = apply(&r, "trader", vec![swap(asset, amount, limit, true)]);
            if b.receipts[0].status == "Success" {
                successes += 1;
                let t = trace(&b);
                assert_eq!(t.spent.checked_add(t.remainder).unwrap(), amount.into());
                assert_eq!(
                    r.state
                        .balance("trader", asset)
                        .checked_sub(b.state.balance("trader", asset))
                        .unwrap(),
                    t.spent
                );
                assert_eq!(
                    b.state
                        .balance("trader", other)
                        .checked_sub(r.state.balance("trader", other))
                        .unwrap(),
                    t.output
                );
                assert!(t.steps.len() <= 2049);
                b.state.invariant(&r.scenario).unwrap();
                clmm::views(&r.scenario, &b.state).unwrap();
            } else {
                reverts += 1;
                assert_eq!(b.receipts[0].status, "ProtocolRevert");
                rollback(&r, &b);
            }
            r.state = b.state;
        }
    }
    assert!(successes > 100 && reverts > 0);
}
