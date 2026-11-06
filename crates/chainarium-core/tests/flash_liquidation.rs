use chainarium_core::{engine, *};
use std::sync::Arc;

fn fixture() -> Scenario {
    Scenario::parse(include_bytes!(
        "../../../scenarios/demos/sc-07-flash-liquidation.json"
    ))
    .unwrap()
}
fn funded(s: Scenario) -> Run {
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert!(r.blocks[0].receipts.iter().all(|r| r.status == "Success"));
    r
}
fn successful(s: &Scenario) -> Transaction {
    let mut t = s.blocks[1][2].clone();
    t.nonce = 0.into();
    t
}
fn second(r: &Run, t: Transaction) -> Block {
    engine::build_block(&r.scenario, &r.state, &[r.scenario.blocks[1][0].clone(), t]).unwrap()
}
fn accounting(w: &World) -> (Amount, Amount, Amount, Amount) {
    let c = &w.lending["credit"];
    (w.balance("reserve", "Q"), c.debt, c.claims, c.equity)
}
fn assert_rolled_back(r: &Run, b: &Block, status: &str) {
    let receipt = &b.receipts[1];
    assert_eq!(receipt.status, status);
    assert!(receipt.events.is_empty());
    // Accrual and the independent oracle publication stay committed. Everything
    // in the flash transaction must equal the same block without that candidate,
    // except its native envelope effects.
    let mut control =
        engine::build_block(&r.scenario, &r.state, &[r.scenario.blocks[1][0].clone()])
            .unwrap()
            .state;
    let a = Arc::make_mut(&mut control.accounts);
    let carol = a.get_mut("carol").unwrap();
    carol.nonce = 1.into();
    carol.balances.insert(
        "Q".into(),
        carol.balances["Q"].checked_sub(receipt.fee).unwrap(),
    );
    a.get_mut("builder")
        .unwrap()
        .balances
        .insert("Q".into(), receipt.fee);
    assert_eq!(b.state, control);
    assert_eq!(
        accounting(&b.state),
        (20.into(), 90.into(), 108.into(), 2.into())
    );
    assert_eq!(b.state.balance("escrow-bob", "X"), 6.into());
    b.state.invariant(&r.scenario).unwrap();
}

#[test]
fn vf_flash_liquidation_real_legs_loss_and_surplus() {
    let mut r = funded(fixture());
    assert_eq!(
        r.state.feeds["feed"]
            .record
            .as_ref()
            .unwrap()
            .price
            .numerator(),
        20.into()
    );
    assert_eq!(
        accounting(&r.state),
        (20.into(), 80.into(), 100.into(), 0.into())
    );
    r.step().unwrap();
    let b = &r.blocks[1];
    assert_eq!(b.receipts[1].status, "ProtocolRevert");
    assert!(b.receipts[1].events.is_empty());
    let receipt = &b.receipts[2];
    assert_eq!(receipt.status, "Success");
    assert_eq!(
        accounting(&r.state),
        (80.into(), 0.into(), 80.into(), 0.into())
    );
    let loss = &r.state.lending["credit"].losses;
    assert_eq!(loss.len(), 1);
    assert_eq!(
        (loss[0].amount, loss[0].equity_loss, loss[0].depositor_loss),
        (30.into(), 2.into(), 28.into())
    );
    for (from, to, asset, amount) in [
        ("flash", "carol", "Q", 60),
        ("carol", "reserve", "Q", 60),
        ("escrow-bob", "carol", "X", 6),
        ("carol", "sale-pool", "X", 6),
        ("sale-pool", "carol", "Q", 84),
        ("carol", "stable-account", "Q", 84),
        ("stable-account", "carol", "R", 83),
        ("carol", "exit-pool", "R", 83),
        ("exit-pool", "carol", "Q", 76),
        ("carol", "flash", "Q", 61),
    ] {
        assert!(
            receipt.events.iter().any(|e| e.kind == "Transfer"
                && e.from == from
                && e.to == to
                && e.asset == asset
                && e.amount == Amount::from(amount)),
            "missing {from} -> {to} {amount} {asset}"
        );
    }
    assert_eq!(r.state.balance("carol", "Q"), 15.into());
    assert_eq!(r.state.balance("carol", "X"), 0.into());
    assert_eq!(r.state.balance("carol", "R"), 0.into());
    assert_eq!(r.state.balance("flash", "Q"), 10001.into());
    assert_eq!(r.state.balance("escrow-bob", "X"), 0.into());
    let liquidation = receipt
        .trace
        .iter()
        .find(|t| t.path == "0.callback.0")
        .unwrap();
    assert_eq!(
        liquidation
            .oracle
            .as_ref()
            .unwrap()
            .record
            .as_ref()
            .unwrap()
            .price
            .numerator(),
        11.into()
    );
    assert!(receipt.trace.iter().any(|t| !t.solver.is_empty()));
    assert!(receipt.trace.iter().any(|t| !t.source_updates.is_empty()));
    r.step().unwrap();
    assert_eq!(
        accounting(&r.state),
        (60.into(), 0.into(), 60.into(), 0.into())
    );
    assert_eq!(r.state.lending["credit"].total_shares, 75.into());
    r.state.invariant(&r.scenario).unwrap();
}

#[test]
fn insufficient_sale_revenue_restores_writeoff_and_all_three_venues() {
    let mut s = fixture();
    // All legs execute, but the last market pays 45 Q, below the 61 Q due.
    s.accounts
        .get_mut("exit-pool")
        .unwrap()
        .balances
        .insert("Q".into(), 600.into());
    let r = funded(s);
    let b = second(&r, successful(&r.scenario));
    assert_rolled_back(&r, &b, "ProtocolRevert");
    let receipt = &b.receipts[1];
    assert!(receipt.trace.iter().any(|t| {
        t.path == "0.callback.3"
            && t.events
                .iter()
                .any(|e| e.from == "exit-pool" && e.amount == Amount::from(45))
    }));
    assert!(receipt.trace.iter().any(|t| !t.source_updates.is_empty()));
    assert!(receipt.trace.iter().any(|t| !t.solver.is_empty()));
}

#[test]
fn route_minimum_failure_restores_liquidation() {
    let r = funded(fixture());
    let mut t = successful(&r.scenario);
    let Call::FlashLoan { callback, .. } = &mut t.program[0] else {
        unreachable!()
    };
    let Call::SwapOutput { min_out, .. } = &mut callback[3] else {
        unreachable!()
    };
    *min_out = 1000.into();
    assert_rolled_back(&r, &second(&r, t), "ProtocolRevert");
}

#[test]
fn late_failure_after_flash_settlement_restores_fee_and_history() {
    let r = funded(fixture());
    let mut t = successful(&r.scenario);
    t.program[1] = Call::AssertCondition {
        condition: false,
        message: "after flash settlement".into(),
    };
    let b = second(&r, t);
    assert_rolled_back(&r, &b, "ProtocolRevert");
    assert!(b.receipts[1].trace.iter().any(|t| {
        t.events
            .iter()
            .any(|e| e.kind == "FlashSettled" && e.amount == Amount::from(1))
    }));
}

#[test]
fn each_resource_boundary_is_atomic_and_charges_the_envelope() {
    let mut s = fixture();
    s.accounts
        .get_mut("carol")
        .unwrap()
        .balances
        .insert("Q".into(), 1000.into());
    let r = funded(s);
    let mut template = successful(&r.scenario);
    // Ignore the profit guard for this exhaustion sweep: resource units cost
    // more than the educational 15-atom surplus. The full successful trace
    // defines every boundary, including solver and settlement transfers.
    template.program.pop();
    let complete = second(&r, template.clone());
    assert_eq!(complete.receipts[1].status, "Success");
    let used: u64 = complete.receipts[1].used.to_string().parse().unwrap();
    for limit in 100..used {
        let mut t = template.clone();
        t.limit = limit.into();
        t.price = 1.into();
        let b = second(&r, t);
        assert_rolled_back(&r, &b, "ResourceExhausted");
        assert_eq!(b.receipts[1].fee, limit.into());
        assert_eq!(b.receipts[1].refund, 0.into());
    }
}

#[test]
fn composite_profit_guard_uses_actual_execution_fee() {
    let mut s = fixture();
    s.accounts
        .get_mut("carol")
        .unwrap()
        .balances
        .insert("Q".into(), 1000.into());
    let r = funded(s);
    let mut t = successful(&r.scenario);
    t.price = 1.into();
    t.limit = 500.into();
    t.program[1] = Call::AssertNetBalanceDelta {
        asset: "Q".into(),
        baseline: 1000.into(),
        minimum: 1.into(),
    };
    let b = second(&r, t);
    assert_rolled_back(&r, &b, "ProtocolRevert");
    assert_eq!(
        b.receipts[1].error.as_deref(),
        Some("net balance delta assertion")
    );
    assert!(b.receipts[1].fee > 15.into());
}

#[test]
fn composite_checkpoint_replays_attempts_sources_and_loss() {
    let mut r = Run::new(fixture()).unwrap();
    for _ in 0..3 {
        r.step().unwrap();
        assert_eq!(Run::restore(&r.checkpoint().unwrap()).unwrap(), r);
    }
    let mut partial = funded(fixture());
    partial = Run::restore(&partial.checkpoint().unwrap()).unwrap();
    partial.finish().unwrap();
    assert_eq!(r, partial);
}
