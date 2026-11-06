use chainarium_core::{book::*, engine, *};
use std::sync::Arc;

fn fixture() -> Scenario {
    Scenario::parse(include_bytes!("../../../scenarios/demos/sc-08-book.json")).unwrap()
}
fn order(side: Side, lots: u64, price: u64, tif: TimeInForce) -> Call {
    Call::PlaceOrder {
        book: "book".into(),
        side,
        lots: lots.into(),
        price_ticks: price.into(),
        tif,
    }
}
fn tx(w: &World, who: &str, calls: Vec<Call>) -> Transaction {
    Transaction {
        id: format!("{}-{who}", w.block),
        agent: who.into(),
        sender: who.into(),
        chain: "lab-1".into(),
        nonce: w.accounts[who].nonce,
        limit: 5000.into(),
        price: Amount::ZERO,
        deadline: Amount::MAX,
        program: calls,
    }
}
fn seeded() -> Run {
    let mut r = Run::new(fixture()).unwrap();
    r.step().unwrap();
    r
}
fn execute(r: &Run, who: &str, calls: Vec<Call>) -> Block {
    engine::build_block(&r.scenario, &r.state, &[tx(&r.state, who, calls)]).unwrap()
}
fn cancel(id: u64) -> Call {
    Call::CancelOrder {
        book: "book".into(),
        order_id: id.into(),
    }
}
fn trace(b: &Block) -> &OrderTrace {
    b.receipts[0].trace[0].book.as_ref().unwrap()
}
fn rollback(r: &Run, b: &Block) {
    let mut expected = engine::build_block(&r.scenario, &r.state, &[])
        .unwrap()
        .state;
    let receipt = &b.receipts[0];
    let a = Arc::make_mut(&mut expected.accounts);
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
    assert_eq!(b.state, expected);
    assert!(receipt.events.is_empty());
}
#[test]
fn exact_four_lots_fee_per_match_and_refund() {
    let r = seeded();
    let b = execute(&r, "carol", vec![order(Side::Buy, 4, 55, TimeInForce::IOC)]);
    assert_eq!(b.receipts[0].status, "Success");
    let t = trace(&b);
    assert_eq!(
        (t.reserved, t.gross_quote, t.fee, t.refund),
        (448.into(), 420.into(), 5.into(), 23.into())
    );
    assert_eq!(
        (t.filled_lots, t.remaining_lots, t.resting_lots),
        (4.into(), 0.into(), 0.into())
    );
    assert_eq!(
        t.fills
            .iter()
            .map(|f| (f.maker_id, f.lots, f.fee))
            .collect::<Vec<_>>(),
        vec![
            (1.into(), 2.into(), 2.into()),
            (2.into(), 2.into(), 3.into())
        ]
    );
    for (account, asset, amount) in [
        ("carol", "X", 40),
        ("carol", "Q", 4575),
        ("alice", "Q", 2200),
        ("bob", "Q", 220),
        ("treasury", "Q", 5),
        ("book-escrow", "X", 10),
        ("book-escrow", "Q", 0),
    ] {
        assert_eq!(b.state.balance(account, asset), amount.into());
    }
    assert_eq!(b.state.books["book"].orders.len(), 1);
    assert_eq!(
        b.state.books["book"].orders[&2.into()].remaining_lots,
        1.into()
    );
    b.state.invariant(&r.scenario).unwrap();
}
#[test]
fn ioc_gtc_fok_share_prefix_but_different_remainders() {
    let r = seeded();
    for tif in [TimeInForce::IOC, TimeInForce::GTC, TimeInForce::FOK] {
        let b = execute(&r, "carol", vec![order(Side::Buy, 6, 55, tif)]);
        let t = trace(&b);
        assert_eq!(
            (t.gross_quote, t.fee, t.filled_lots, t.remaining_lots),
            (530.into(), 6.into(), 5.into(), 1.into())
        );
        if tif == TimeInForce::FOK {
            assert_eq!(
                b.receipts[0].error.as_deref(),
                Some("FOK not fully fillable")
            );
            rollback(&r, &b);
        } else {
            assert_eq!(b.receipts[0].status, "Success");
            assert_eq!(
                t.refund,
                if tif == TimeInForce::IOC {
                    136.into()
                } else {
                    26.into()
                }
            );
            assert_eq!(
                b.state.balance("book-escrow", "Q"),
                if tif == TimeInForce::IOC {
                    0.into()
                } else {
                    110.into()
                }
            );
            if tif == TimeInForce::GTC {
                let mut after = r.clone();
                after.state = b.state;
                let cancelled = execute(&after, "carol", vec![cancel(3)]);
                assert_eq!(trace(&cancelled).refund, 110.into());
                assert_eq!(cancelled.state.balance("carol", "Q"), 4464.into());
            }
        }
    }
}
#[test]
fn price_time_priority_uses_acceptance_not_owner_or_id_text() {
    let mut r = Run::new(fixture()).unwrap();
    // Bob is accepted first at 50; Alice's better 49 must execute before him.
    let block = engine::build_block(
        &r.scenario,
        &r.state,
        &[
            tx(
                &r.state,
                "bob",
                vec![order(Side::Sell, 2, 50, TimeInForce::GTC)],
            ),
            tx(
                &r.state,
                "alice",
                vec![
                    order(Side::Sell, 1, 49, TimeInForce::GTC),
                    order(Side::Sell, 2, 50, TimeInForce::GTC),
                ],
            ),
        ],
    )
    .unwrap();
    r.state = block.state;
    let b = execute(&r, "carol", vec![order(Side::Buy, 4, 55, TimeInForce::IOC)]);
    assert_eq!(
        trace(&b)
            .fills
            .iter()
            .map(|f| f.maker_id)
            .collect::<Vec<_>>(),
        vec![2.into(), 1.into(), 3.into()]
    );
    assert_eq!(
        b.state.books["book"].orders[&3.into()].remaining_lots,
        1.into()
    );
}
#[test]
fn sell_taker_pays_quote_fee_maker_price_and_zero_net_is_explicit() {
    let mut r = Run::new(fixture()).unwrap();
    r.state = execute(&r, "carol", vec![order(Side::Buy, 3, 55, TimeInForce::GTC)]).state;
    let b = execute(&r, "bob", vec![order(Side::Sell, 2, 50, TimeInForce::IOC)]);
    assert_eq!(
        (trace(&b).gross_quote, trace(&b).fee),
        (220.into(), 3.into())
    );
    assert_eq!(b.state.balance("bob", "Q"), 217.into());
    assert_eq!(b.state.balance("carol", "X"), 20.into());
    assert_eq!(b.state.balance("book-escrow", "Q"), 110.into());
    let mut s = fixture();
    s.blocks.clear();
    let cfg = s.books.get_mut("book").unwrap();
    cfg.quote_tick_atoms = 1.into();
    cfg.taker_fee_bps = 9999;
    let mut r = Run::new(s).unwrap();
    r.state = execute(&r, "carol", vec![order(Side::Buy, 1, 1, TimeInForce::GTC)]).state;
    let b = execute(&r, "bob", vec![order(Side::Sell, 1, 1, TimeInForce::FOK)]);
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(trace(&b).fills[0].taker_quote, 0.into());
    assert_eq!(b.state.balance("treasury", "Q"), 1.into());
    assert_eq!(b.state.balance("bob", "Q"), 0.into());
}
#[test]
fn self_trade_and_unauthorized_cancel_do_not_skip_or_unlock() {
    let r = seeded();
    for (who, calls, error) in [
        (
            "alice",
            vec![order(Side::Buy, 4, 55, TimeInForce::IOC)],
            "SelfTrade",
        ),
        ("carol", vec![cancel(1)], "order owner required"),
        ("bob", vec![cancel(999)], "unknown order id"),
    ] {
        let b = execute(&r, who, calls);
        assert_eq!(b.receipts[0].error.as_deref(), Some(error));
        rollback(&r, &b);
    }
    let b = execute(&r, "alice", vec![cancel(1)]);
    assert_eq!(trace(&b).refund, 20.into());
    assert_eq!(b.state.balance("alice", "X"), 100.into());
}
#[test]
fn late_failure_and_every_resource_boundary_restore_escrow_ids_and_fees() {
    let r = seeded();
    let call = order(Side::Buy, 4, 55, TimeInForce::IOC);
    let b = execute(
        &r,
        "carol",
        vec![
            call.clone(),
            Call::AssertCondition {
                condition: false,
                message: "late book revert".into(),
            },
        ],
    );
    rollback(&r, &b);
    let used: u64 = execute(&r, "carol", vec![call.clone()]).receipts[0]
        .used
        .to_string()
        .parse()
        .unwrap();
    for limit in 100..used {
        let mut t = tx(&r.state, "carol", vec![call.clone()]);
        t.limit = limit.into();
        t.price = 1.into();
        let b = engine::build_block(&r.scenario, &r.state, &[t]).unwrap();
        assert_eq!(b.receipts[0].status, "ResourceExhausted");
        rollback(&r, &b);
        assert_eq!(b.receipts[0].fee, limit.into());
    }
}
#[test]
fn empty_ioc_refunds_full_reserve_fok_reverts_and_cancel_is_single_use() {
    let r = Run::new(fixture()).unwrap();
    let b = execute(&r, "carol", vec![order(Side::Buy, 1, 55, TimeInForce::IOC)]);
    assert_eq!(trace(&b).status, "Unfilled");
    assert_eq!(trace(&b).refund, 112.into());
    assert_eq!(b.state.books["book"].next_id, 2.into());
    let b = execute(&r, "carol", vec![order(Side::Buy, 1, 55, TimeInForce::FOK)]);
    rollback(&r, &b);
    let mut r = seeded();
    r.state = execute(&r, "alice", vec![cancel(1)]).state;
    let b = execute(&r, "alice", vec![cancel(1)]);
    rollback(&r, &b);
}
#[test]
fn exact_reservation_is_required_and_protected_escrow_rejects_donations() {
    let mut r = seeded();
    // Spend is 425 but the declared upper reserve is 448.
    let mut s = fixture();
    s.accounts
        .get_mut("carol")
        .unwrap()
        .balances
        .insert("Q".into(), 447.into());
    let mut limited = Run::new(s).unwrap();
    limited.step().unwrap();
    let b = execute(
        &limited,
        "carol",
        vec![order(Side::Buy, 4, 55, TimeInForce::IOC)],
    );
    rollback(&limited, &b);
    let b = execute(
        &r,
        "alice",
        vec![Call::Transfer {
            to: "book-escrow".into(),
            asset: "X".into(),
            amount: 1.into(),
        }],
    );
    rollback(&r, &b);
    r.state = execute(
        &r,
        "alice",
        vec![Call::Approve {
            spender: "bob".into(),
            asset: "X".into(),
            amount: 1.into(),
        }],
    )
    .state;
    let b = execute(
        &r,
        "bob",
        vec![Call::TransferFrom {
            from: "alice".into(),
            to: "book-escrow".into(),
            asset: "X".into(),
            amount: 1.into(),
        }],
    );
    rollback(&r, &b);
}
#[test]
fn read_only_quote_checks_funding_nonce_and_stale_book_reexecutes() {
    let mut r = seeded();
    let before = r.state.clone();
    let t = tx(
        &r.state,
        "carol",
        vec![order(Side::Buy, 4, 55, TimeInForce::IOC)],
    );
    let q = quote(&r.scenario, &r.state, &t).unwrap();
    assert_eq!(q.receipt.status, "Success");
    assert_eq!(r.state, before);
    assert_eq!(
        q.receipt,
        execute(&r, "carol", t.program.clone()).receipts[0]
    );
    r.state = execute(&r, "alice", vec![cancel(1)]).state;
    let q2 = quote(&r.scenario, &r.state, &t).unwrap();
    assert_ne!(q2.state_ref, q.state_ref);
    assert_eq!(
        q2.receipt.trace[0].book.as_ref().unwrap().filled_lots,
        3.into()
    );
    let mut invalid = t.clone();
    invalid.nonce = 2.into();
    assert!(
        quote(&r.scenario, &r.state, &invalid)
            .unwrap_err()
            .contains("nonce")
    );
    invalid = t.clone();
    invalid.deadline = 0.into();
    assert!(
        quote(&r.scenario, &r.state, &invalid)
            .unwrap_err()
            .contains("deadline")
    );
    invalid = t;
    invalid.price = 100.into();
    assert!(
        quote(&r.scenario, &r.state, &invalid)
            .unwrap_err()
            .contains("fee budget")
    );
}
#[test]
fn config_zero_overflow_and_sequence_limits_fail_explicitly() {
    for field in [
        "lot_atoms",
        "quote_tick_atoms",
        "taker_fee_bps",
        "account",
        "fee_account",
    ] {
        let mut s = fixture();
        let c = s.books.get_mut("book").unwrap();
        match field {
            "lot_atoms" => c.lot_atoms = 0.into(),
            "quote_tick_atoms" => c.quote_tick_atoms = 0.into(),
            "taker_fee_bps" => c.taker_fee_bps = 10000,
            "account" => c.account = "alice".into(),
            _ => c.fee_account = "book-escrow".into(),
        };
        assert!(s.validate().is_err());
    }
    let r = seeded();
    for call in [
        order(Side::Buy, 0, 55, TimeInForce::IOC),
        order(Side::Buy, 1, 0, TimeInForce::IOC),
        Call::PlaceOrder {
            book: "book".into(),
            side: Side::Buy,
            price_ticks: Amount::MAX,
            lots: 1.into(),
            tif: TimeInForce::IOC,
        },
        Call::PlaceOrder {
            book: "book".into(),
            side: Side::Sell,
            price_ticks: 1.into(),
            lots: Amount::MAX,
            tif: TimeInForce::IOC,
        },
    ] {
        let b = execute(&r, "carol", vec![call]);
        assert_eq!(b.receipts[0].status, "ProtocolRevert");
        rollback(&r, &b);
    }
    let mut r = r;
    Arc::make_mut(&mut r.state.books)
        .get_mut("book")
        .unwrap()
        .next_id = Amount::MAX;
    let b = execute(&r, "carol", vec![order(Side::Buy, 1, 55, TimeInForce::IOC)]);
    rollback(&r, &b);
}
#[test]
fn capacity_allows_matching_and_refuses_only_new_remainder() {
    let mut s = fixture();
    s.blocks.clear();
    s.chain.block_limit = 1000000.into();
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("X".into(), 20000.into());
    s.accounts
        .get_mut("carol")
        .unwrap()
        .balances
        .insert("Q".into(), 1000000.into());
    let mut r = Run::new(s).unwrap();
    for _ in 0..4 {
        r.state = execute(
            &r,
            "alice",
            (0..256)
                .map(|_| order(Side::Sell, 1, 50, TimeInForce::GTC))
                .collect(),
        )
        .state;
    }
    assert_eq!(r.state.books["book"].orders.len(), MAX_ORDERS);
    let b = execute(&r, "bob", vec![order(Side::Sell, 1, 55, TimeInForce::GTC)]);
    assert_eq!(b.receipts[0].error.as_deref(), Some("book capacity"));
    rollback(&r, &b);
    let b = execute(&r, "carol", vec![order(Side::Buy, 1, 50, TimeInForce::FOK)]);
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(b.state.books["book"].orders.len(), 1023);
    let mut t = tx(
        &r.state,
        "carol",
        vec![order(Side::Buy, 1024, 50, TimeInForce::FOK)],
    );
    t.limit = 30000.into();
    let b = engine::build_block(&r.scenario, &r.state, &[t]).unwrap();
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(trace(&b).fills.len(), 1024);
    assert!(b.state.books["book"].orders.is_empty());
}
#[test]
fn book_checkpoint_preserves_priority_attempts_and_ids() {
    let mut r = Run::new(fixture()).unwrap();
    for _ in 0..6 {
        r.step().unwrap();
        assert_eq!(Run::restore(&r.checkpoint().unwrap()).unwrap(), r);
    }
    let mut resumed = Run::restore(&seeded().checkpoint().unwrap()).unwrap();
    resumed.finish().unwrap();
    assert_eq!(r, resumed);
    assert!(r.state.books["book"].orders.is_empty());
    assert_eq!(r.state.books["book"].next_id, 10.into());
    assert_eq!(r.state.balance("treasury", "Q"), 16.into());
    let statuses: Vec<_> = r
        .blocks
        .iter()
        .map(|b| {
            b.receipts
                .iter()
                .map(|r| r.status.as_str())
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(
        statuses,
        vec![
            vec!["Success", "Success"],
            vec!["ProtocolRevert", "ProtocolRevert", "Success"],
            vec!["ProtocolRevert", "Success", "Success", "Success"],
            vec!["Success", "Success"],
            vec!["ProtocolRevert", "Success", "Success", "ProtocolRevert"],
            vec!["Success", "ProtocolRevert", "ResourceExhausted"]
        ]
    );
}

#[test]
fn matching_agrees_with_independent_sorted_list_for_both_sides_and_tifs() {
    let mut rng = 7u64;
    for case in 0..96 {
        let mut s = fixture();
        s.blocks.clear();
        for who in ["alice", "bob", "carol"] {
            for asset in ["X", "Q"] {
                s.accounts
                    .get_mut(who)
                    .unwrap()
                    .balances
                    .insert(asset.into(), 1000000.into());
            }
        }
        let fee = [0u64, 1, 100, 9999][case % 4];
        s.books.get_mut("book").unwrap().taker_fee_bps = fee as u16;
        let side = if case % 2 == 0 { Side::Buy } else { Side::Sell };
        let maker_side = if side == Side::Buy {
            Side::Sell
        } else {
            Side::Buy
        };
        let mut r = Run::new(s).unwrap();
        let mut list = vec![];
        // Reference data is a flat list, independently sorted for each request.
        for id in 1..=12u64 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let price = 45 + (rng >> 32) % 21;
            let lots = 1 + (rng >> 16) % 5;
            let who = if id % 2 == 0 { "alice" } else { "bob" };
            r.state = execute(
                &r,
                who,
                vec![order(maker_side, lots, price, TimeInForce::GTC)],
            )
            .state;
            list.push((id, price, lots));
        }
        list.sort_by_key(|&(id, price, _)| {
            (
                if side == Side::Buy {
                    price
                } else {
                    100 - price
                },
                id,
            )
        });
        let requested = 1 + (case as u64 * 7) % 55;
        let limit = 49 + (case as u64 % 12);
        let mut left = requested;
        let mut expected = vec![];
        let mut gross = 0u64;
        let mut fees = 0u64;
        for &(id, price, lots) in &list {
            if left == 0
                || (side == Side::Buy && price > limit)
                || (side == Side::Sell && price < limit)
            {
                break;
            }
            let filled = left.min(lots);
            left -= filled;
            let q = filled * price * 2;
            let f = (q * fee).div_ceil(10000);
            gross += q;
            fees += f;
            expected.push((
                Amount::from(id),
                Amount::from(filled),
                Amount::from(f),
                Amount::from(lots - filled),
            ));
        }
        let tif = [TimeInForce::GTC, TimeInForce::IOC, TimeInForce::FOK][case % 3];
        let b = execute(&r, "carol", vec![order(side, requested, limit, tif)]);
        let t = trace(&b);
        assert_eq!(
            t.fills
                .iter()
                .map(|f| (f.maker_id, f.lots, f.fee, f.maker_remaining))
                .collect::<Vec<_>>(),
            expected,
            "case {case}"
        );
        assert_eq!(
            (t.gross_quote, t.fee, t.remaining_lots),
            (gross.into(), fees.into(), left.into())
        );
        if tif == TimeInForce::FOK && left > 0 {
            rollback(&r, &b);
            continue;
        }
        assert_eq!(b.receipts[0].status, "Success");
        let filled = requested - left;
        let locked = if tif == TimeInForce::GTC { left } else { 0 };
        let (x, q) = if side == Side::Buy {
            (
                1000000 + filled * 10,
                1000000 - gross - fees - locked * limit * 2,
            )
        } else {
            (1000000 - filled * 10 - locked * 10, 1000000 + gross - fees)
        };
        assert_eq!(b.state.balance("carol", "X"), x.into());
        assert_eq!(b.state.balance("carol", "Q"), q.into());
        assert_eq!(b.state.balance("treasury", "Q"), fees.into());
        b.state.invariant(&r.scenario).unwrap();
        views(&r.scenario, &b.state).unwrap();
    }
}

#[test]
fn fee_recipient_can_be_taker_without_duplicating_or_losing_tokens() {
    let mut s = fixture();
    s.books.get_mut("book").unwrap().fee_account = "carol".into();
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let b = execute(&r, "carol", vec![order(Side::Buy, 4, 55, TimeInForce::IOC)]);
    assert_eq!(trace(&b).fee, 5.into());
    assert_eq!(b.state.balance("carol", "Q"), 4580.into());
    b.state.invariant(&r.scenario).unwrap();
}
