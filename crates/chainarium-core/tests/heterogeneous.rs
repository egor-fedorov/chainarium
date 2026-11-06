use chainarium_core::{
    book::{Side, TimeInForce},
    engine,
    routing::heterogeneous::*,
    *,
};
use std::{collections::BTreeMap, sync::Arc};

fn scenario() -> Scenario {
    Scenario::parse(include_bytes!(
        "../../../scenarios/demos/sc-03-heterogeneous.json"
    ))
    .unwrap()
}
fn seeded() -> Run {
    let mut r = Run::new(scenario()).unwrap();
    r.step().unwrap();
    r
}
fn tx(w: &World, who: &str, program: Vec<Call>) -> Transaction {
    Transaction {
        id: format!("route-{}-{who}", w.block),
        agent: who.into(),
        sender: who.into(),
        chain: "lab-1".into(),
        nonce: w.accounts[who].nonce,
        limit: 1000.into(),
        price: Amount::ZERO,
        deadline: Amount::MAX,
        program,
    }
}
fn amm(venue: &str) -> VenueHop {
    VenueHop::Amm {
        venue: venue.into(),
    }
}
fn book() -> VenueHop {
    VenueHop::Book {
        venue: "book".into(),
        limit_price_ticks: 50.into(),
    }
}
fn route(asset: &str, amount: u64, hops: Vec<VenueHop>, policy: ResidualPolicy) -> Call {
    Call::ExecuteRoute {
        asset_in: asset.into(),
        amount: amount.into(),
        hops,
        min_out: 1.into(),
        residual_policy: policy,
    }
}
fn order(side: Side, price: u64, lots: u64) -> Call {
    Call::PlaceOrder {
        book: "book".into(),
        side,
        price_ticks: price.into(),
        lots: lots.into(),
        tif: TimeInForce::GTC,
    }
}
fn apply(r: &Run, calls: Vec<Call>) -> Block {
    engine::build_block(&r.scenario, &r.state, &[tx(&r.state, "trader", calls)]).unwrap()
}
fn result(b: &Block) -> &RouteResult {
    &b.receipts[0].route_results["0"]
}
fn rollback(r: &Run, b: &Block) {
    let mut expected = engine::build_block(&r.scenario, &r.state, &[])
        .unwrap()
        .state;
    let receipt = &b.receipts[0];
    let accounts = Arc::make_mut(&mut expected.accounts);
    let sender = accounts.get_mut(&receipt.sender).unwrap();
    sender.nonce = sender.nonce.checked_add(1.into()).unwrap();
    sender.balances.insert(
        "Q".into(),
        sender.balances["Q"].checked_sub(receipt.fee).unwrap(),
    );
    let builder = accounts.get_mut("builder").unwrap();
    builder.balances.insert(
        "Q".into(),
        builder.balances["Q"].checked_add(receipt.fee).unwrap(),
    );
    assert_eq!(b.state, expected);
    assert!(receipt.events.is_empty() && receipt.route_results.is_empty());
}
fn query(asset_in: &str, asset_out: &str, amount: u64) -> Query {
    Query {
        agent: "trader".into(),
        sender: "trader".into(),
        asset_in: asset_in.into(),
        asset_out: asset_out.into(),
        amount: amount.into(),
        min_out: 1.into(),
        residual_policy: ResidualPolicy::KeepResiduals,
        resource_limit: 1000.into(),
        resource_price: Amount::ZERO,
        limits: vec![DirectionalLimit {
            asset_in: "X".into(),
            hop: book(),
        }],
    }
}

#[test]
fn pinned_book_sell_clmm_partial_and_three_family_demo_replay() {
    let mut r = seeded();
    r.step().unwrap();
    let b = &r.blocks[1];
    assert_eq!(
        b.receipts
            .iter()
            .map(|r| r.status.as_str())
            .collect::<Vec<_>>(),
        ["ProtocolRevert", "ProtocolRevert", "Success"]
    );
    assert!(b.receipts[0].route_results.is_empty() && b.receipts[1].route_results.is_empty());
    let out = &b.receipts[2].route_results["0"];
    assert_eq!(
        (out.input_spent, out.input_unspent, out.output),
        (20.into(), 15.into(), 165.into())
    );
    assert_eq!(out.residuals, BTreeMap::from([("X".into(), 15.into())]));
    assert_eq!(out.hops[0].output, 198.into());
    assert_eq!(out.hops[1].budget, 198.into());
    r.step().unwrap();
    let b = &r.blocks[2];
    assert_eq!(b.receipts[0].status, "ProtocolRevert");
    let out = &b.receipts[1].route_results["0"];
    assert_eq!(
        (out.input_spent, out.input_unspent, out.output),
        (5029.into(), 4971.into(), 3327.into())
    );
    assert_eq!(out.hops[0].output, 4987.into());
    r.step().unwrap();
    let receipt = &r.blocks[3].receipts[1];
    assert_eq!(receipt.status, "Success");
    assert_eq!(receipt.route_results["0"].hops.len(), 3);
    assert!(receipt.trace.iter().any(|t| t.book.is_some()));
    assert!(receipt.trace.iter().any(|t| !t.solver.is_empty()));
    let finished = r.clone();
    let mut r = Run::new(scenario()).unwrap();
    while r.step().unwrap().is_some() {
        let mut restored = Run::restore(&r.checkpoint().unwrap()).unwrap();
        restored.finish().unwrap();
        assert_eq!(restored, finished);
    }
}

#[test]
fn buy_price_improvement_and_fee_to_self_are_real_residuals() {
    for fee_to_self in [false, true] {
        let mut s = scenario();
        if fee_to_self {
            s.books.get_mut("book").unwrap().fee_account = "trader".into();
        }
        let mut r = Run::new(s).unwrap();
        r.state = engine::build_block(
            &r.scenario,
            &r.state,
            &[
                tx(&r.state, "alice", vec![order(Side::Sell, 50, 2)]),
                tx(&r.state, "bob", vec![order(Side::Sell, 55, 3)]),
            ],
        )
        .unwrap()
        .state;
        let hop = VenueHop::Book {
            venue: "book".into(),
            limit_price_ticks: 55.into(),
        };
        let b = apply(
            &r,
            vec![route(
                "Q",
                448,
                vec![hop.clone()],
                ResidualPolicy::KeepResiduals,
            )],
        );
        assert_eq!(b.receipts[0].status, "Success");
        let out = result(&b);
        assert_eq!(out.output, 40.into());
        assert_eq!(
            out.input_spent,
            if fee_to_self { 420.into() } else { 425.into() }
        );
        assert_eq!(
            out.input_unspent,
            if fee_to_self { 28.into() } else { 23.into() }
        );
        let strict = apply(
            &r,
            vec![route("Q", 448, vec![hop], ResidualPolicy::RequireFull)],
        );
        assert_eq!(
            strict.receipts[0].error.as_deref(),
            Some("route requires full input consumption")
        );
        rollback(&r, &strict);
    }
}

#[test]
fn fee_to_self_is_output_on_book_sell_and_envelope_fee_is_separate() {
    let mut s = scenario();
    s.books.get_mut("book").unwrap().fee_account = "trader".into();
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let mut request = tx(
        &r.state,
        "trader",
        vec![route(
            "X",
            35,
            vec![book(), amm("exit")],
            ResidualPolicy::KeepResiduals,
        )],
    );
    request.price = 1.into();
    let b = engine::build_block(&r.scenario, &r.state, &[request]).unwrap();
    assert_eq!(b.receipts[0].status, "Success");
    let out = result(&b);
    assert_eq!(out.hops[0].output, 200.into());
    assert_eq!(out.output, 166.into());
    assert_eq!(out.input_spent, 20.into());
    assert_eq!(
        b.state.balance("trader", "Q"),
        r.state
            .balance("trader", "Q")
            .checked_sub(b.receipts[0].fee)
            .unwrap()
    );
}

#[test]
fn late_failure_minimum_and_every_resource_boundary_roll_back_all_venues() {
    let r = seeded();
    let call = route(
        "X",
        35,
        vec![book(), amm("exit"), amm("stable")],
        ResidualPolicy::KeepResiduals,
    );
    let good = apply(&r, vec![call.clone()]);
    assert_eq!(good.receipts[0].status, "Success");
    let cost = good.receipts[0].used.to_string().parse::<u64>().unwrap();
    for limit in 100..cost {
        let mut request = tx(&r.state, "trader", vec![call.clone()]);
        request.limit = limit.into();
        request.price = 1.into();
        let b = engine::build_block(&r.scenario, &r.state, &[request]).unwrap();
        assert_eq!(b.receipts[0].status, "ResourceExhausted", "limit={limit}");
        assert_eq!(b.receipts[0].used, limit.into());
        rollback(&r, &b);
    }
    let b = apply(
        &r,
        vec![
            call.clone(),
            Call::AssertCondition {
                condition: false,
                message: "late".into(),
            },
        ],
    );
    rollback(&r, &b);
    assert!(
        b.receipts[0]
            .trace
            .iter()
            .filter(|t| t.operation != "AssertCondition")
            .all(|t| t.outcome == "RolledBack")
    );
    let mut minimum = call;
    if let Call::ExecuteRoute { min_out, .. } = &mut minimum {
        *min_out = Amount::MAX;
    }
    let b = apply(&r, vec![minimum]);
    rollback(&r, &b);
    assert_eq!(b.receipts[0].error.as_deref(), Some("route min_out"));
}

#[test]
fn no_fill_no_liquidity_and_lot_dust_are_explicit_failures() {
    let r = Run::new(scenario()).unwrap();
    for (call, error) in [
        (
            route("X", 35, vec![book()], ResidualPolicy::KeepResiduals),
            "route hop has no output",
        ),
        (
            route("X", 5, vec![book()], ResidualPolicy::KeepResiduals),
            "route budget below one book lot",
        ),
        (
            route(
                "X",
                1000,
                vec![VenueHop::Clmm {
                    venue: "clmm".into(),
                    limit_tick: -100,
                }],
                ResidualPolicy::KeepResiduals,
            ),
            "NoExecutableOutput",
        ),
    ] {
        let b = apply(&r, vec![call]);
        assert_eq!(b.receipts[0].status, "ProtocolRevert");
        assert_eq!(b.receipts[0].error.as_deref(), Some(error));
        rollback(&r, &b);
    }
}

#[test]
fn typed_root_output_never_spends_old_intermediate_inventory() {
    let mut s = scenario();
    s.accounts
        .get_mut("trader")
        .unwrap()
        .balances
        .insert("R".into(), 500.into());
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let b = apply(
        &r,
        vec![
            route(
                "X",
                35,
                vec![book(), amm("exit")],
                ResidualPolicy::KeepResiduals,
            ),
            Call::SwapOutput {
                pool: "exit".into(),
                asset_in: "R".into(),
                output_of: "0".into(),
                min_out: 1.into(),
            },
        ],
    );
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(b.state.balance("trader", "R"), 500.into());
    assert_eq!(result(&b).hops[1].budget, 198.into());
    for reference in ["0.0", "1", "nonexistent"] {
        let request = tx(
            &r.state,
            "trader",
            vec![
                route(
                    "X",
                    35,
                    vec![book(), amm("exit")],
                    ResidualPolicy::KeepResiduals,
                ),
                Call::SwapOutput {
                    pool: "exit".into(),
                    asset_in: "R".into(),
                    output_of: reference.into(),
                    min_out: 1.into(),
                },
            ],
        );
        assert!(r.scenario.validate_tx(&request).is_err());
    }
}

#[test]
fn structural_limits_count_generated_children_and_check_types() {
    let r = seeded();
    for hops in [
        vec![],
        vec![book(), book()],
        vec![amm("exit"), amm("deep"), book(), amm("stable")],
        vec![VenueHop::Clmm {
            venue: "clmm".into(),
            limit_tick: -32769,
        }],
        vec![amm("book")],
    ] {
        assert!(
            r.scenario
                .validate_tx(&tx(
                    &r.state,
                    "trader",
                    vec![route("X", 35, hops, ResidualPolicy::KeepResiduals)]
                ))
                .is_err()
        );
    }
    let call = route(
        "X",
        35,
        vec![book(), amm("exit"), amm("stable")],
        ResidualPolicy::KeepResiduals,
    );
    // 256 roots * (1 route + 3 children) is precisely the admitted maximum.
    let request = tx(&r.state, "trader", vec![call.clone(); 256]);
    r.scenario.validate_tx(&request).unwrap();
    let too_large = tx(
        &r.state,
        "trader",
        vec![Call::Sequence {
            calls: request.program,
        }],
    );
    assert_eq!(
        r.scenario.validate_tx(&too_large).unwrap_err(),
        "route child node/depth limit"
    );
    let mut nested = call;
    for _ in 0..30 {
        nested = Call::Sequence {
            calls: vec![nested],
        };
    }
    r.scenario
        .validate_tx(&tx(&r.state, "trader", vec![nested.clone()]))
        .unwrap();
    nested = Call::Sequence {
        calls: vec![nested],
    };
    assert_eq!(
        r.scenario
            .validate_tx(&tx(&r.state, "trader", vec![nested]))
            .unwrap_err(),
        "route child node/depth limit"
    );
}

#[test]
fn search_is_readonly_all_families_require_limits_and_stale_execution_rechecks() {
    let r = seeded();
    let before = r.checkpoint().unwrap();
    let mut q = query("X", "Q", 35);
    let quoted = quote(&r.scenario, &r.state, &q).unwrap();
    assert_eq!(quoted.search.examined, 1.into());
    assert_eq!(
        quoted.search.candidates[0].result().unwrap().output,
        198.into()
    );
    q.limits.clear();
    assert_eq!(
        quote(&r.scenario, &r.state, &q).unwrap().search.examined,
        Amount::ZERO
    );
    q.limits.push(DirectionalLimit {
        asset_in: "X".into(),
        hop: VenueHop::Clmm {
            venue: "clmm".into(),
            limit_tick: -100,
        },
    });
    assert!(
        quote(&r.scenario, &r.state, &q).unwrap().search.candidates[0]
            .result()
            .is_some()
    );
    assert_eq!(before, r.checkpoint().unwrap());
    let fill = tx(
        &r.state,
        "alice",
        vec![Call::PlaceOrder {
            book: "book".into(),
            side: Side::Sell,
            price_ticks: 50.into(),
            lots: 2.into(),
            tif: TimeInForce::IOC,
        }],
    );
    let b = engine::build_block(
        &r.scenario,
        &r.state,
        &[fill, quoted.search.candidates[0].transaction.clone()],
    )
    .unwrap();
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(
        b.receipts[1].error.as_deref(),
        Some("route hop has no output")
    );
    assert!(b.receipts[1].route_results.is_empty());
    assert_ne!(
        quoted.state_ref,
        quote(&r.scenario, &b.state, &q).unwrap().state_ref
    );
}

#[test]
fn search_caps_exact_executions_preserves_failures_and_breaks_ties_deterministically() {
    let mut s = scenario();
    for i in 0..70 {
        let id = format!("p{i:03}");
        s.accounts.insert(
            id.clone(),
            Account {
                nonce: Amount::ZERO,
                balances: BTreeMap::from([("X".into(), 10000.into()), ("Q".into(), 10000.into())]),
            },
        );
        s.protocols.insert(
            id.clone(),
            Pool {
                account: id,
                x: "X".into(),
                y: "Q".into(),
                fee_bps: 0,
                owner: "alice".into(),
            },
        );
    }
    let r = Run::new(s).unwrap();
    let mut q = query("X", "Q", 100);
    q.limits.clear();
    let result = quote(&r.scenario, &r.state, &q).unwrap().search;
    assert!(result.truncated);
    assert_eq!(result.examined, 64.into());
    assert_eq!(result.candidates[0].hops[0].venue(), "p000");
    assert_eq!(result.candidates.last().unwrap().hops[0].venue(), "p063");
    assert!(
        result
            .candidates
            .iter()
            .all(|c| c.result().unwrap().output == 99.into())
    );
    q.resource_limit = 110.into();
    let failed = quote(&r.scenario, &r.state, &q).unwrap().search;
    assert_eq!(failed.examined, 64.into());
    assert!(
        failed
            .candidates
            .iter()
            .all(|c| c.result().is_none()
                && c.receipt.as_ref().unwrap().status == "ResourceExhausted")
    );
    q.resource_price = Amount::MAX;
    assert!(quote(&r.scenario, &r.state, &q).is_err());
    q.resource_limit = 1000.into();
    q.resource_price = 1000.into();
    let failed = quote(&r.scenario, &r.state, &q).unwrap().search;
    assert!(
        failed.candidates.iter().all(|c| c.receipt.is_none()
            && c.admission_error.as_deref() == Some("insufficient fee budget"))
    );
}

#[test]
fn search_output_tie_prefers_lower_actual_resource_before_venue_name() {
    let mut s = scenario();
    for id in ["aaa-stable", "zzz-cpmm"] {
        s.accounts.insert(
            id.into(),
            Account {
                nonce: Amount::ZERO,
                balances: BTreeMap::from([("X".into(), 10000.into()), ("Q".into(), 10000.into())]),
            },
        );
        let p = Pool {
            account: id.into(),
            x: "X".into(),
            y: "Q".into(),
            fee_bps: 0,
            owner: "alice".into(),
        };
        if id == "zzz-cpmm" {
            s.protocols.insert(id.into(), p);
        } else {
            s.stable_pools.insert(
                id.into(),
                chainarium_core::stable::StablePool {
                    pool: p,
                    amplification: 100,
                    iteration_limit: 255,
                },
            );
        }
    }
    let r = Run::new(s).unwrap();
    let mut q = query("X", "Q", 100);
    q.limits.clear();
    let result = quote(&r.scenario, &r.state, &q).unwrap().search;
    assert_eq!(result.examined, 2.into());
    assert!(
        result
            .candidates
            .iter()
            .all(|c| c.result().unwrap().output == 99.into())
    );
    assert_eq!(result.candidates[0].hops[0].venue(), "zzz-cpmm");
    assert!(
        result.candidates[0].receipt.as_ref().unwrap().used
            < result.candidates[1].receipt.as_ref().unwrap().used
    );
}

#[test]
fn flash_callback_shortfall_rolls_back_route_and_provider() {
    let mut s = scenario();
    s.model_versions.push("strict-flash-provider-v1".into());
    s.accounts.insert(
        "flash-account".into(),
        Account {
            nonce: Amount::ZERO,
            balances: BTreeMap::from([("X".into(), 10000.into())]),
        },
    );
    s.flash_providers.insert(
        "flash".into(),
        FlashProvider {
            account: "flash-account".into(),
            asset: "X".into(),
            fee_bps: 5,
        },
    );
    s.accounts
        .get_mut("trader")
        .unwrap()
        .balances
        .insert("X".into(), Amount::ZERO);
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let b = apply(
        &r,
        vec![Call::FlashLoan {
            provider: "flash".into(),
            receiver: "trader".into(),
            amount: 35.into(),
            callback: vec![route(
                "X",
                35,
                vec![book(), amm("exit")],
                ResidualPolicy::KeepResiduals,
            )],
        }],
    );
    assert_eq!(b.receipts[0].status, "ProtocolRevert");
    assert!(
        b.receipts[0].trace.iter().any(|t| t
            .route
            .as_ref()
            .is_some_and(|r| r.output == 165.into())
            && t.outcome == "RolledBack")
    );
    rollback(&r, &b);
}
