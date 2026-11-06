use chainarium_core::{
    engine,
    policy::ArbitragePolicy,
    routing::{Observation, search},
    stable::*,
    *,
};
use std::collections::{BTreeMap, BTreeSet};
fn a(n: u64) -> Amount {
    n.into()
}
fn account(q: u64, r: u64) -> Account {
    Account {
        nonce: Amount::ZERO,
        balances: BTreeMap::from([("Q".into(), q.into()), ("R".into(), r.into())]),
    }
}
fn scenario() -> Scenario {
    let mut s = Scenario::parse(include_bytes!("../../../scenarios/demos/sc-01.json")).unwrap();
    s.blocks.clear();
    s.protocols.clear();
    s.accounts.clear();
    s.agents.clear();
    s.model_versions.extend(M2_MODELS.map(String::from));
    s.model_versions.push(MODEL.into());
    s.assets = BTreeMap::from([
        ("Q".into(), Asset { decimals: 6 }),
        ("R".into(), Asset { decimals: 6 }),
    ]);
    s.accounts = BTreeMap::from([
        ("alice".into(), account(2_000_000_000, 2_000_000_000)),
        ("lp".into(), account(0, 0)),
        ("builder".into(), account(0, 0)),
        (
            "stable-account".into(),
            account(1_000_000_000, 1_000_000_000),
        ),
    ]);
    for id in ["alice", "lp"] {
        s.agents.insert(
            id.into(),
            Agent {
                controlled_accounts: BTreeSet::from([id.into()]),
            },
        );
    }
    s.stable_pools.insert(
        "stable".into(),
        StablePool {
            pool: Pool {
                account: "stable-account".into(),
                x: "Q".into(),
                y: "R".into(),
                fee_bps: 4,
                owner: "lp".into(),
            },
            amplification: 100,
            iteration_limit: 255,
        },
    );
    s
}
fn tx(calls: Vec<Call>) -> Transaction {
    Transaction {
        id: "stable-tx".into(),
        agent: "alice".into(),
        sender: "alice".into(),
        chain: "lab-1".into(),
        nonce: Amount::ZERO,
        limit: 1000.into(),
        price: Amount::ZERO,
        deadline: Amount::MAX,
        program: calls,
    }
}
fn swap(input: u64, minimum: u64) -> Call {
    Call::SwapExactIn {
        pool: "stable".into(),
        asset_in: "Q".into(),
        amount: input.into(),
        min_out: minimum.into(),
    }
}
fn fail() -> Call {
    Call::AssertCondition {
        condition: false,
        message: "after StableSwap".into(),
    }
}
fn run(mut s: Scenario, tx: Transaction) -> Run {
    s.blocks = vec![vec![tx]];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    r
}
fn rec(r: &Run) -> &Receipt {
    &r.blocks[0].receipts[0]
}
fn d(s: &Scenario, w: &World) -> Amount {
    let p = s.stable_pools["stable"].parameters(s).unwrap();
    invariant(
        p.normalized([
            w.balance("stable-account", "Q"),
            w.balance("stable-account", "R"),
        ])
        .unwrap(),
        &p,
        &mut || Ok::<(), String>(()),
        &mut vec![],
    )
    .unwrap()
}
#[test]
fn vf_stable_pinned_analytical_balanced_depeg_and_unequal_decimals() {
    let values: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../docs/reviews/stableswap-reference.json"
    ))
    .unwrap();
    for case in values.as_array().unwrap() {
        let dx = case["decimals"][0].as_u64().unwrap();
        let dy = case["decimals"][1].as_u64().unwrap();
        let raw = [
            case["balances_atoms"][0].as_str().unwrap().parse().unwrap(),
            case["balances_atoms"][1].as_str().unwrap().parse().unwrap(),
        ];
        let p = Parameters {
            amplification: 100,
            iteration_limit: 255,
            fee_bps: 4,
            scale_x: 10u64.pow(18 - dx as u32).into(),
            scale_y: 10u64.pow(18 - dy as u32).into(),
        };
        let mut steps = vec![];
        let mut charged = 0;
        let out = output(
            raw,
            true,
            case["input_atoms"].as_str().unwrap().parse().unwrap(),
            &p,
            &mut || {
                charged += 1;
                Ok::<(), String>(())
            },
            &mut steps,
        )
        .unwrap();
        assert_eq!(
            out.to_string(),
            case["expected_output_atoms"].as_str().unwrap()
        );
        assert_eq!(charged, steps.len());
        assert!(steps.iter().filter(|x| x.converged).count() == 2);
        assert!(steps.iter().all(|x| x.iteration <= 255));
    }
}
#[test]
fn vf_stable_execution_metering_fee_minimum_and_trace() {
    let s = scenario();
    let w = World::genesis(&s).unwrap();
    assert_eq!(d(&s, &w), "2000000000000000000000".parse().unwrap());
    let mut t = tx(vec![swap(100_000_000, 99_860_150)]);
    t.price = 2.into();
    let r = run(s.clone(), t);
    assert_eq!(rec(&r).status, "Success");
    let iterations = rec(&r).trace[0].solver.len();
    assert_eq!(rec(&r).used, a(120 + iterations as u64));
    assert_eq!(rec(&r).fee, a(2 * (120 + iterations as u64)));
    assert_eq!(
        rec(&r)
            .charges
            .iter()
            .filter(|c| c.phase == "Iteration")
            .count(),
        iterations
    );
    assert_eq!(r.state.balance("stable-account", "Q"), a(1_100_000_000));
    assert_eq!(r.state.balance("stable-account", "R"), a(900_139_850));
    assert!(d(&s, &r.state) >= d(&s, &w));
    let r = run(s, tx(vec![swap(100_000_000, 99_860_151)]));
    assert_eq!(rec(&r).error.as_deref(), Some("min_out"));
    assert_eq!(r.state.balance("stable-account", "Q"), a(1_000_000_000));
    assert!(!rec(&r).trace[0].solver.is_empty());
    assert!(rec(&r).events.is_empty());
}
#[test]
fn vf_stable_nonconvergence_exhaustion_and_genesis_limits() {
    let mut s = scenario();
    s.stable_pools.get_mut("stable").unwrap().iteration_limit = 1;
    let r = run(s.clone(), tx(vec![swap(100_000_000, 1)]));
    assert_eq!(rec(&r).error.as_deref(), Some("SolverDidNotConverge: y"));
    assert_eq!(rec(&r).used, a(112));
    assert_eq!(rec(&r).trace[0].solver.len(), 2);
    assert_eq!(
        r.state.accounts["stable-account"],
        r.initial.accounts["stable-account"]
    );
    s.accounts
        .get_mut("stable-account")
        .unwrap()
        .balances
        .insert("Q".into(), 100_000_000.into());
    assert!(s.validate().unwrap_err().contains("SolverDidNotConverge"));
    let mut t = tx(vec![swap(100_000_000, 1)]);
    t.limit = 111.into();
    t.price = 3.into();
    let r = run(scenario(), t);
    assert_eq!(rec(&r).status, "ResourceExhausted");
    assert_eq!(rec(&r).fee, a(333));
    assert_eq!(rec(&r).refund, Amount::ZERO);
    assert_eq!(rec(&r).trace[0].solver.len(), 1);
    assert_eq!(
        r.state.accounts["stable-account"],
        r.initial.accounts["stable-account"]
    );
}
#[test]
fn vf_stable_proportional_lp_minima_burn_and_last_exit() {
    let s = scenario();
    let initial = World::genesis(&s).unwrap();
    let mint: Amount = "200000000000000000000".parse().unwrap();
    let half: Amount = "100000000000000000000".parse().unwrap();
    let add = Call::AddLiquidity {
        pool: "stable".into(),
        max_x: 100_000_000.into(),
        max_y: 120_000_000.into(),
        min_shares: mint,
    };
    let r = run(
        s.clone(),
        tx(vec![
            add.clone(),
            Call::RemoveLiquidity {
                pool: "stable".into(),
                shares: half,
                min_x: 50_000_000.into(),
                min_y: 50_000_000.into(),
            },
        ]),
    );
    assert_eq!(rec(&r).status, "Success");
    assert_eq!(r.state.lp_balances["stable"]["alice"], half);
    assert_eq!(r.state.balance("stable-account", "Q"), a(1_050_000_000));
    assert_eq!(r.state.balance("stable-account", "R"), a(1_050_000_000));
    assert_eq!(r.state.balance("alice", "R"), a(1_950_000_000));
    let bad = Call::AddLiquidity {
        pool: "stable".into(),
        max_x: 100_000_000.into(),
        max_y: 120_000_000.into(),
        min_shares: mint.checked_add(1.into()).unwrap(),
    };
    let r = run(s.clone(), tx(vec![bad]));
    assert_eq!(rec(&r).status, "ProtocolRevert");
    assert_eq!(r.state.shares, initial.shares);
    let bad = Call::RemoveLiquidity {
        pool: "stable".into(),
        shares: half,
        min_x: 50_000_001.into(),
        min_y: 0.into(),
    };
    let r = run(s.clone(), tx(vec![add, bad]));
    assert_eq!(rec(&r).status, "ProtocolRevert");
    assert_eq!(r.state.lp_balances, initial.lp_balances);
    let mut t = tx(vec![Call::RemoveLiquidity {
        pool: "stable".into(),
        shares: initial.shares["stable"],
        min_x: 1.into(),
        min_y: 1.into(),
    }]);
    t.agent = "lp".into();
    t.sender = "lp".into();
    let r = run(s, t);
    assert_eq!(rec(&r).status, "Success");
    assert_eq!(r.state.shares["stable"], Amount::ZERO);
    assert_eq!(r.state.balance("stable-account", "Q"), Amount::ZERO);
    assert!(engine::quote(&r.scenario, &r.state, "stable", "Q", 1.into()).is_err());
    let bootstrap = engine::build_block(
        &r.scenario,
        &r.state,
        &[tx(vec![Call::AddLiquidity {
            pool: "stable".into(),
            max_x: 100_000_000.into(),
            max_y: 100_000_000.into(),
            min_shares: mint,
        }])],
    )
    .unwrap();
    assert_eq!(bootstrap.receipts[0].status, "Success");
    assert_eq!(bootstrap.state.shares["stable"], mint);
    assert_eq!(bootstrap.receipts[0].trace[0].solver.len(), 1);
}
#[test]
fn vf_stable_late_rollback_restores_swaps_shares_and_fee_retention() {
    let mut t = tx(vec![
        Call::AddLiquidity {
            pool: "stable".into(),
            max_x: 100_000_000.into(),
            max_y: 100_000_000.into(),
            min_shares: 1.into(),
        },
        swap(100_000_000, 1),
        fail(),
    ]);
    t.price = 1.into();
    let r = run(scenario(), t);
    assert_eq!(rec(&r).status, "ProtocolRevert");
    assert!(rec(&r).events.is_empty());
    assert_eq!(r.state.shares, r.initial.shares);
    assert_eq!(r.state.lp_balances, r.initial.lp_balances);
    assert_eq!(
        r.state.accounts["stable-account"],
        r.initial.accounts["stable-account"]
    );
    assert_eq!(
        r.state.balance("alice", "Q"),
        a(2_000_000_000).checked_sub(rec(&r).fee).unwrap()
    );
    assert!(rec(&r).trace[1].outcome.contains("RolledBack"));
}
fn two_venues() -> Scenario {
    let mut s = scenario();
    s.accounts
        .insert("cpmm-account".into(), account(1_500_000_000, 1_000_000_000));
    s.protocols.insert(
        "cpmm".into(),
        Pool {
            account: "cpmm-account".into(),
            x: "Q".into(),
            y: "R".into(),
            fee_bps: 30,
            owner: "lp".into(),
        },
    );
    s
}
#[test]
fn vf_stable_route_actual_outputs_simulation_and_net_fee_guard() {
    let s = two_venues();
    let w = World::genesis(&s).unwrap();
    let obs = Observation::capture(&s, &w, "alice").unwrap();
    let found = search(&obs, "Q", "Q", 100_000_000.into());
    let route = &found.routes[0];
    assert_eq!(route.hops.len(), 2);
    assert_eq!(route.hops[0].venue, "stable");
    assert_eq!(route.hops[0].output, a(99_860_150));
    assert!(route.output > a(130_000_000));
    let r = run(
        s.clone(),
        tx(route.calls(100_000_000.into(), 130_000_000.into(), "")),
    );
    assert_eq!(rec(&r).status, "Success");
    assert_eq!(r.state.balance("alice", "R"), w.balance("alice", "R"));
    assert_eq!(
        r.state.balance("alice", "Q"),
        w.balance("alice", "Q")
            .checked_sub(100_000_000.into())
            .unwrap()
            .checked_add(route.output)
            .unwrap()
    );
    let mut s = s;
    s.blocks = vec![vec![]; 2];
    s.policies.insert(
        "alice".into(),
        ArbitragePolicy {
            account: "alice".into(),
            asset: "Q".into(),
            input: 100_000_000.into(),
            min_profit: 1.into(),
            resource_limit: 1000.into(),
            resource_price: 1000.into(),
        },
    );
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert_eq!(r.blocks[0].receipts[0].status, "Success");
    let decision = &r.blocks[0].decisions[0];
    assert!(decision.expected_net_gain.unwrap() > a(30_000_000));
    assert_eq!(
        r.state
            .balance("alice", "Q")
            .checked_sub(w.balance("alice", "Q"))
            .unwrap(),
        decision.expected_net_gain.unwrap()
    );
    assert!(matches!(
        decision
            .transaction
            .as_ref()
            .unwrap()
            .program
            .last()
            .unwrap(),
        Call::AssertNetBalanceDelta { .. }
    ));
    let mut resumed = Run::restore(&r.checkpoint().unwrap()).unwrap();
    r.finish().unwrap();
    resumed.finish().unwrap();
    assert_eq!(r, resumed);
}
#[test]
fn vf_stable_route_second_leg_failure_rolls_back_first_and_rejects_stale_minimum() {
    let s = two_venues();
    let w = World::genesis(&s).unwrap();
    let route = search(
        &Observation::capture(&s, &w, "alice").unwrap(),
        "Q",
        "Q",
        100_000_000.into(),
    )
    .routes
    .remove(0);
    let r = run(
        s.clone(),
        tx(route.calls(
            100_000_000.into(),
            route.output.checked_add(1.into()).unwrap(),
            "",
        )),
    );
    assert_eq!(rec(&r).status, "ProtocolRevert");
    assert_eq!(
        r.state.accounts["stable-account"],
        w.accounts["stable-account"]
    );
    assert_eq!(r.state.accounts["cpmm-account"], w.accounts["cpmm-account"]);
    let prior = engine::build_block(&s, &w, &[tx(vec![swap(500_000_000, 1)])]).unwrap();
    let mut stale = tx(vec![swap(100_000_000, 99_860_150)]);
    stale.nonce = 1.into();
    let b = engine::build_block(&s, &prior.state, &[stale]).unwrap();
    assert_eq!(b.receipts[0].error.as_deref(), Some("min_out"));
    assert_eq!(
        b.state.accounts["stable-account"],
        prior.state.accounts["stable-account"]
    );
}
#[test]
fn vf_stable_domain_configuration_protection_zero_mint_and_tiny_output() {
    for decimals in [19, 255] {
        let mut s = scenario();
        s.assets.get_mut("R").unwrap().decimals = decimals;
        assert!(s.validate().is_err());
    }
    for amp in [0, 1_000_001] {
        let mut s = scenario();
        s.stable_pools.get_mut("stable").unwrap().amplification = amp;
        assert!(s.validate().is_err());
    }
    for limit in [0, 256] {
        let mut s = scenario();
        s.stable_pools.get_mut("stable").unwrap().iteration_limit = limit;
        assert!(s.validate().is_err());
    }
    let mut s = scenario();
    s.chain.fee_account = "stable-account".into();
    assert!(s.validate().is_err());
    let mut s = scenario();
    s.protocols
        .insert("stable".into(), s.stable_pools["stable"].pool.clone());
    assert!(s.validate().is_err());
    let r = run(
        scenario(),
        tx(vec![Call::Transfer {
            to: "stable-account".into(),
            asset: "Q".into(),
            amount: 1.into(),
        }]),
    );
    assert_eq!(rec(&r).error.as_deref(), Some("protected protocol account"));
    let r = run(scenario(), tx(vec![swap(1, 0)]));
    assert_eq!(rec(&r).status, "ProtocolRevert");
    let r = run(
        scenario(),
        tx(vec![Call::RemoveLiquidity {
            pool: "stable".into(),
            shares: 1.into(),
            min_x: 0.into(),
            min_y: 0.into(),
        }]),
    );
    assert_eq!(rec(&r).error.as_deref(), Some("insufficient LP shares"));
    let mut s = scenario();
    for asset in s.assets.values_mut() {
        asset.decimals = 18;
    }
    let half = Amount::from_big(Amount::MAX.big() / 2u8).unwrap();
    s.accounts.get_mut("stable-account").unwrap().balances =
        BTreeMap::from([("Q".into(), half), ("R".into(), half)]);
    s.accounts.get_mut("alice").unwrap().balances =
        BTreeMap::from([("Q".into(), a(1)), ("R".into(), a(1))]);
    let r = run(
        s,
        tx(vec![Call::AddLiquidity {
            pool: "stable".into(),
            max_x: 1.into(),
            max_y: 1.into(),
            min_shares: 1.into(),
        }]),
    );
    assert_eq!(
        rec(&r).error.as_deref(),
        Some("StableSwap normalized domain")
    );
    assert_eq!(r.state.shares, r.initial.shares);
    let mut s = scenario();
    for asset in s.assets.values_mut() {
        asset.decimals = 18;
    }
    s.accounts.get_mut("stable-account").unwrap().balances =
        BTreeMap::from([("Q".into(), a(1_000_000)), ("R".into(), a(1))]);
    // At tiny normalized balances the integer recurrence can oscillate;
    // the accepted profile must stop at its limit rather than hang or invent D.
    assert!(s.validate().unwrap_err().contains("SolverDidNotConverge"));
    s.accounts
        .get_mut("stable-account")
        .unwrap()
        .balances
        .insert("Q".into(), a(1000));
    let r = run(
        s,
        tx(vec![Call::AddLiquidity {
            pool: "stable".into(),
            max_x: 1.into(),
            max_y: 1.into(),
            min_shares: 0.into(),
        }]),
    );
    assert_eq!(rec(&r).error.as_deref(), Some("min_shares or zero mint"));
}
#[test]
fn vf_stable_dynamic_fee_guard_is_final_and_covers_solver_work() {
    let s = two_venues();
    let w = World::genesis(&s).unwrap();
    let route = search(
        &Observation::capture(&s, &w, "alice").unwrap(),
        "Q",
        "Q",
        100_000_000.into(),
    )
    .routes
    .remove(0);
    let gross = route.output.checked_sub(100_000_000.into()).unwrap();
    // A flat two-CPMM estimate of 150 units omits the StableSwap iterations.
    let guard = Call::AssertNetBalanceDelta {
        asset: "Q".into(),
        baseline: w.balance("alice", "Q"),
        minimum: gross.checked_sub(150.into()).unwrap(),
    };
    let mut calls = route.calls(100_000_000.into(), 1.into(), "");
    calls.push(guard.clone());
    let mut t = tx(calls);
    t.price = 1.into();
    let r = run(s.clone(), t);
    assert_eq!(
        rec(&r).error.as_deref(),
        Some("net balance delta assertion")
    );
    assert!(rec(&r).used > 150.into());
    assert_eq!(
        r.state.accounts["stable-account"],
        w.accounts["stable-account"]
    );
    assert!(s.validate_tx(&tx(vec![guard.clone(), swap(1, 0)])).is_err());
    assert!(
        s.validate_tx(&tx(vec![Call::Sequence { calls: vec![guard] }]))
            .is_err()
    );
}
#[test]
fn vf_stable_boundary_sweep_conserves_tokens_and_invariant_in_both_directions() {
    let mut random = 271u64;
    for i in 0..180 {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut s = scenario();
        s.accounts.get_mut("stable-account").unwrap().balances = BTreeMap::from([
            ("Q".into(), a((1 + random % 10_000) * 1_000_000)),
            ("R".into(), a((1 + (random >> 16) % 10_000) * 1_000_000)),
        ]);
        s.stable_pools.get_mut("stable").unwrap().amplification = [1, 100, 1_000_000][i % 3];
        s.stable_pools.get_mut("stable").unwrap().pool.fee_bps = (random % 31) as u16;
        let raw_input = 1 + (random >> 32) % 100_000_000;
        let input_asset = if i % 2 == 0 { "Q" } else { "R" };
        let call = Call::SwapExactIn {
            pool: "stable".into(),
            asset_in: input_asset.into(),
            amount: raw_input.into(),
            min_out: 1.into(),
        };
        let initial = World::genesis(&s).unwrap();
        let initial_d = d(&s, &initial);
        let r = run(s.clone(), tx(vec![call]));
        assert_eq!(rec(&r).status, "Success", "case {i}");
        r.state.invariant(&s).unwrap();
        assert!(
            d(&s, &r.state).checked_add(1.into()).unwrap() >= initial_d,
            "D tolerance, case {i}"
        );
        assert_eq!(
            r.state.balance("stable-account", input_asset),
            initial
                .balance("stable-account", input_asset)
                .checked_add(raw_input.into())
                .unwrap()
        );
    }
}
