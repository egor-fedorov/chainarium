// Independent DEC-20 review cases, authored against commit 1bc17ee.
// Expected values use direct integer arithmetic, not protocol pricing helpers.
use chainarium_core::{
    book::{Side, TimeInForce},
    engine,
    routing::heterogeneous::{ResidualPolicy, VenueHop},
    *,
};
use std::collections::BTreeMap;
fn scenario() -> Scenario {
    Scenario::parse(include_bytes!(
        "../../../scenarios/demos/sc-03-heterogeneous.json"
    ))
    .unwrap()
}
fn tx(w: &World, sender: &str, program: Vec<Call>) -> Transaction {
    Transaction {
        id: format!("review:{sender}:{}", w.block),
        agent: sender.into(),
        sender: sender.into(),
        chain: "lab-1".into(),
        nonce: w.accounts[sender].nonce,
        limit: 1000.into(),
        price: 1.into(),
        deadline: Amount::MAX,
        program,
    }
}
fn order(book: &str, side: Side, price: u64, lots: u64) -> Call {
    Call::PlaceOrder {
        book: book.into(),
        side,
        price_ticks: price.into(),
        lots: lots.into(),
        tif: TimeInForce::GTC,
    }
}
fn ceilfee(gross: u64, bps: u64) -> u64 {
    (gross * bps).div_ceil(10000)
}
#[test]
fn independent_sell_route_analytical_sweep() {
    for bps in [0, 1, 99, 100, 3333, 9999] {
        for self_fee in [false, true] {
            let mut s = scenario();
            s.blocks.clear();
            s.books.get_mut("book").unwrap().taker_fee_bps = bps as u16;
            if self_fee {
                s.books.get_mut("book").unwrap().fee_account = "trader".into();
            }
            let w = World::genesis(&s).unwrap();
            let w = engine::build_block(
                &s,
                &w,
                &[
                    tx(&w, "alice", vec![order("book", Side::Buy, 50, 3)]),
                    tx(&w, "bob", vec![order("book", Side::Buy, 45, 7)]),
                ],
            )
            .unwrap()
            .state;
            for budget in 1u64..=141 {
                for mode in [ResidualPolicy::KeepResiduals, ResidualPolicy::RequireFull] {
                    let a = (budget / 10).min(3);
                    let b = (budget / 10 - a).min(7);
                    let spent = (a + b) * 10;
                    let gross = a * 100 + b * 90;
                    let fee = ceilfee(a * 100, bps) + ceilfee(b * 90, bps);
                    let q = if self_fee { gross } else { gross - fee };
                    let out = 1000 * q / (1000 + q);
                    let call = Call::ExecuteRoute {
                        asset_in: "X".into(),
                        amount: budget.into(),
                        hops: vec![
                            VenueHop::Book {
                                venue: "book".into(),
                                limit_price_ticks: 40.into(),
                            },
                            VenueHop::Amm {
                                venue: "exit".into(),
                            },
                        ],
                        min_out: 1.into(),
                        residual_policy: mode,
                    };
                    let result =
                        engine::build_block(&s, &w, &[tx(&w, "trader", vec![call])]).unwrap();
                    let r = &result.receipts[0];
                    if out == 0 || (mode == ResidualPolicy::RequireFull && spent < budget) {
                        assert_eq!(r.status, "ProtocolRevert");
                        assert_eq!(result.state.books, w.books);
                        assert_eq!(
                            result.state.balance("trader", "X"),
                            w.balance("trader", "X")
                        );
                        assert!(r.route_results.is_empty());
                    } else {
                        assert_eq!(
                            r.status, "Success",
                            "bps={bps}, budget={budget}, self={self_fee}"
                        );
                        let rr = &r.route_results["0"];
                        assert_eq!(
                            (
                                rr.input_spent,
                                rr.input_unspent,
                                rr.hops[0].output,
                                rr.output
                            ),
                            (spent.into(), (budget - spent).into(), q.into(), out.into())
                        );
                        assert_eq!(
                            result.state.balance("trader", "X"),
                            w.balance("trader", "X").checked_sub(spent.into()).unwrap()
                        );
                        assert_eq!(
                            result.state.balance("trader", "Q"),
                            w.balance("trader", "Q").checked_sub(r.fee).unwrap()
                        );
                        assert_eq!(result.state.balance("trader", "R"), out.into());
                    }
                }
            }
        }
    }
}
#[test]
fn independent_buy_route_fee_rounding_and_budget_sweep() {
    for bps in [0, 1, 100, 3333, 9999] {
        for self_fee in [false, true] {
            let mut s = scenario();
            s.blocks.clear();
            s.books.get_mut("book").unwrap().taker_fee_bps = bps as u16;
            if self_fee {
                s.books.get_mut("book").unwrap().fee_account = "trader".into();
            }
            let w = World::genesis(&s).unwrap();
            let w = engine::build_block(
                &s,
                &w,
                &[
                    tx(&w, "alice", vec![order("book", Side::Sell, 50, 2)]),
                    tx(&w, "bob", vec![order("book", Side::Sell, 55, 3)]),
                ],
            )
            .unwrap()
            .state;
            for budget in (1u64..=780).step_by(7) {
                let lots = budget / (110 + ceilfee(110, bps));
                let a = lots.min(2);
                let b = (lots - a).min(3);
                let gross = a * 100 + b * 110;
                let fee = ceilfee(a * 100, bps) + ceilfee(b * 110, bps);
                let spent = gross + if self_fee { 0 } else { fee };
                let call = Call::ExecuteRoute {
                    asset_in: "Q".into(),
                    amount: budget.into(),
                    hops: vec![VenueHop::Book {
                        venue: "book".into(),
                        limit_price_ticks: 55.into(),
                    }],
                    min_out: 1.into(),
                    residual_policy: ResidualPolicy::KeepResiduals,
                };
                let result = engine::build_block(&s, &w, &[tx(&w, "trader", vec![call])]).unwrap();
                let r = &result.receipts[0];
                if lots == 0 {
                    assert_eq!(r.status, "ProtocolRevert");
                    continue;
                }
                assert_eq!(r.status, "Success");
                let rr = &r.route_results["0"];
                assert_eq!(
                    (rr.output, rr.input_spent, rr.input_unspent),
                    (((a + b) * 10).into(), spent.into(), (budget - spent).into())
                );
                assert_eq!(
                    result.state.balance("trader", "Q"),
                    w.balance("trader", "Q")
                        .checked_sub(spent.into())
                        .unwrap()
                        .checked_sub(r.fee)
                        .unwrap()
                );
            }
        }
    }
}
#[test]
fn intermediate_residuals_and_old_inventory_are_distinct() {
    let mut s = scenario();
    s.blocks.clear();
    let mut cfg = s.books["book"].clone();
    cfg.account = "other-escrow".into();
    cfg.base_asset = "R".into();
    cfg.quote_tick_atoms = 1.into();
    s.books.insert("other".into(), cfg);
    s.accounts.insert(
        "other-escrow".into(),
        Account {
            nonce: Amount::ZERO,
            balances: BTreeMap::new(),
        },
    );
    s.accounts
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("R".into(), 100.into());
    let w = World::genesis(&s).unwrap();
    let w = engine::build_block(
        &s,
        &w,
        &[
            tx(&w, "alice", vec![order("book", Side::Buy, 50, 2)]),
            tx(
                &w,
                "bob",
                vec![
                    order("other", Side::Sell, 40, 1),
                    order("other", Side::Sell, 45, 1),
                ],
            ),
        ],
    )
    .unwrap()
    .state;
    let call = Call::ExecuteRoute {
        asset_in: "X".into(),
        amount: 35.into(),
        hops: vec![
            VenueHop::Book {
                venue: "book".into(),
                limit_price_ticks: 50.into(),
            },
            VenueHop::Book {
                venue: "other".into(),
                limit_price_ticks: 50.into(),
            },
        ],
        min_out: 1.into(),
        residual_policy: ResidualPolicy::KeepResiduals,
    };
    let result = engine::build_block(&s, &w, &[tx(&w, "trader", vec![call])]).unwrap();
    let r = &result.receipts[0];
    assert_eq!(r.status, "Success");
    let rr = &r.route_results["0"];
    assert_eq!(
        rr.residuals,
        BTreeMap::from([("X".into(), 15.into()), ("Q".into(), 111.into())])
    );
    assert_eq!(rr.output, 20.into());
    assert_eq!(rr.hops[1].budget, 198.into());
    assert_eq!(
        result.state.balance("trader", "Q"),
        w.balance("trader", "Q")
            .checked_add(111.into())
            .unwrap()
            .checked_sub(r.fee)
            .unwrap()
    );
}
#[test]
fn earlier_book_fill_and_fee_to_self_roll_back_on_self_trade() {
    let mut s = scenario();
    s.blocks.clear();
    s.books.get_mut("book").unwrap().fee_account = "trader".into();
    let w = World::genesis(&s).unwrap();
    let w = engine::build_block(
        &s,
        &w,
        &[
            tx(&w, "alice", vec![order("book", Side::Buy, 50, 1)]),
            tx(&w, "trader", vec![order("book", Side::Buy, 45, 1)]),
        ],
    )
    .unwrap()
    .state;
    let call = Call::ExecuteRoute {
        asset_in: "X".into(),
        amount: 25.into(),
        hops: vec![
            VenueHop::Book {
                venue: "book".into(),
                limit_price_ticks: 40.into(),
            },
            VenueHop::Amm {
                venue: "exit".into(),
            },
        ],
        min_out: 1.into(),
        residual_policy: ResidualPolicy::KeepResiduals,
    };
    let result = engine::build_block(&s, &w, &[tx(&w, "trader", vec![call])]).unwrap();
    let r = &result.receipts[0];
    assert_eq!(r.error.as_deref(), Some("SelfTrade"));
    assert_eq!(result.state.books, w.books);
    assert_eq!(
        result.state.balance("trader", "X"),
        w.balance("trader", "X")
    );
    assert_eq!(
        result.state.balance("trader", "Q"),
        w.balance("trader", "Q").checked_sub(r.fee).unwrap()
    );
    assert!(r.events.is_empty() && r.route_results.is_empty());
}
#[test]
fn lp_exit_at_sixteen_position_boundary_collects_and_preserves_other_owner() {
    use chainarium_core::policy::{
        self, DecisionAction,
        baselines::{LpEvent, LpTarget},
    };
    let mut s = Scenario::parse(include_bytes!(
        "../../../scenarios/demos/sc-lp-policies.json"
    ))
    .unwrap();
    s.lp_policies.remove("bob");
    s.blocks.clear();
    s.lp_policies.get_mut("alice").unwrap().schedule = vec![LpEvent {
        block: 3.into(),
        target: LpTarget::Exit,
    }];
    let mint = Call::MintPosition {
        pool: "clmm".into(),
        lower: -100,
        upper: 100,
        liquidity: 100000.into(),
        max_x: Amount::MAX,
        max_q: Amount::MAX,
    };
    let w = World::genesis(&s).unwrap();
    let w = engine::build_block(
        &s,
        &w,
        &[
            tx(&w, "alice", vec![mint.clone(); 16]),
            tx(&w, "bob", vec![mint]),
        ],
    )
    .unwrap()
    .state;
    assert_eq!(w.clmm["clmm"].positions.len(), 17);
    let swap = Call::ClmmSwap {
        pool: "clmm".into(),
        asset_in: "X".into(),
        amount: 10000.into(),
        sqrt_price_limit: chainarium_core::clmm::math::tick_sqrt(-100).unwrap(),
        min_out: 1.into(),
        allow_partial: true,
    };
    let w = engine::build_block(&s, &w, &[tx(&w, "trader", vec![swap])])
        .unwrap()
        .state;
    let d = policy::decisions(&s, &w).unwrap().remove(0);
    assert_eq!(d.action, DecisionAction::Submit);
    let intent = d.transaction.unwrap();
    assert_eq!(intent.program.len(), 32);
    let b = engine::build_block(&s, &w, &[intent]).unwrap();
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(b.state.clmm["clmm"].positions.len(), 1);
    assert_eq!(
        b.state.clmm["clmm"].positions[&17.into()],
        w.clmm["clmm"].positions[&17.into()]
    );
    assert_eq!(b.state.accounts["bob"], w.accounts["bob"]);
    assert_eq!(b.state.clmm["clmm"].total_liquidity, 100000.into());
}
#[test]
fn mm_stale_cancellation_at_sixteen_order_boundary_preserves_foreign_order() {
    use chainarium_core::policy::{self, DecisionAction, baselines::Anchor};
    let mut s =
        Scenario::parse(include_bytes!("../../../scenarios/demos/sc-mm-oracle.json")).unwrap();
    s.policies.clear();
    s.blocks.clear();
    s.mm_policies.get_mut("alice").unwrap().anchor = Anchor::PublicOracle {
        feed: "public-price".into(),
        max_age: Amount::ZERO,
    };
    let w = World::genesis(&s).unwrap();
    let orders = (0..16)
        .map(|i| {
            order(
                "book",
                if i % 2 == 0 { Side::Buy } else { Side::Sell },
                if i % 2 == 0 { 40 } else { 60 },
                1,
            )
        })
        .collect();
    let mut request = tx(&w, "alice", orders);
    request.price = Amount::ZERO;
    let mut foreign = tx(&w, "bob", vec![order("book", Side::Sell, 90, 1)]);
    foreign.price = Amount::ZERO;
    let w = engine::build_block(&s, &w, &[request, foreign])
        .unwrap()
        .state;
    assert_eq!(w.books["book"].orders.len(), 17);
    let d = policy::decisions(&s, &w).unwrap().remove(0);
    assert_eq!(d.action, DecisionAction::Submit);
    let intent = d.transaction.unwrap();
    assert_eq!(intent.program.len(), 16);
    let b = engine::build_block(&s, &w, &[intent]).unwrap();
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(b.state.books["book"].orders.len(), 1);
    assert_eq!(b.state.books["book"].orders[&17.into()].owner, "bob");
    assert_eq!(b.state.accounts["bob"], w.accounts["bob"]);
    assert_eq!(b.state.balance("alice", "X"), 100.into());
    assert_eq!(b.state.balance("alice", "Q"), 2000.into());
}
