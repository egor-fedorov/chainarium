use chainarium_core::{
    engine,
    run::{Streams, digest},
    *,
};
use std::sync::Arc;
fn scenario() -> Scenario {
    Scenario::parse(include_bytes!("../../../scenarios/demos/sc-01.json")).unwrap()
}
fn transfer() -> Call {
    Call::Transfer {
        to: "bob".into(),
        asset: "X".into(),
        amount: 10.into(),
    }
}
fn fail() -> Call {
    Call::AssertCondition {
        condition: false,
        message: "late failure".into(),
    }
}
fn transaction(program: Vec<Call>) -> Transaction {
    let mut tx = scenario().blocks[0][0].clone();
    tx.program = program;
    tx
}
fn execute(tx: Transaction) -> Run {
    let mut s = scenario();
    s.blocks = vec![vec![tx]];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    r
}
fn receipt(r: &Run) -> &Receipt {
    &r.blocks[0].receipts[0]
}

#[test]
fn vf_wire_exact_u256_and_strict_decimal() {
    for text in [
        "0",
        "9007199254740991",
        "9007199254740993",
        &Amount::MAX.to_string(),
    ] {
        let a: Amount = text.parse().unwrap();
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(json, format!("\"{text}\""));
        assert_eq!(serde_json::from_str::<Amount>(&json).unwrap(), a);
    }
    for text in [
        "",
        "00",
        "01",
        "+1",
        "-1",
        "1.0",
        "1e3",
        " 1",
        "١",
        "115792089237316195423570985008687907853269984665640564039457584007913129639936",
    ] {
        assert!(text.parse::<Amount>().is_err(), "{text}");
    }
    assert!(serde_json::from_str::<Amount>("9007199254740993").is_err());
    assert_eq!(Amount::units("0.000001", 6).unwrap(), 1.into());
    assert_eq!(
        Amount::units("1.001", 18).unwrap().to_string(),
        "1001000000000000000"
    );
    assert!(Amount::units("0.0000001", 6).is_err());
    assert!(Amount::units("01", 0).is_err());
    assert_eq!(
        Amount::MAX
            .mul_div(Amount::MAX, Amount::MAX, false)
            .unwrap(),
        Amount::MAX
    );
    assert_eq!(
        Amount::from(7).mul_div(3.into(), 5.into(), false).unwrap(),
        4.into()
    );
    assert_eq!(
        Amount::from(7).mul_div(3.into(), 5.into(), true).unwrap(),
        5.into()
    );
    assert!(Amount::MAX.checked_add(1.into()).is_err());
    assert!(
        Amount::from(1)
            .mul_div(1.into(), Amount::ZERO, false)
            .is_err()
    );
}
#[test]
fn vf_exec_transfer_late_failure_exact_fee_and_refund() {
    let r = execute(transaction(vec![transfer(), fail()]));
    let receipt = receipt(&r);
    assert_eq!(
        (receipt.used, receipt.reserved, receipt.fee, receipt.refund),
        (125.into(), 400.into(), 250.into(), 150.into())
    );
    assert_eq!(receipt.status, "ProtocolRevert");
    assert!(receipt.events.is_empty());
    assert_eq!(r.state.balance("alice", "X"), 1000.into());
    assert_eq!(r.state.balance("bob", "X"), Amount::ZERO);
    assert_eq!(r.state.balance("alice", "Q"), 9750.into());
    assert_eq!(r.state.balance("builder", "Q"), 250.into());
    assert_eq!(r.state.accounts["alice"].nonce, 1.into());
    assert_eq!(receipt.trace[0].outcome, "RolledBack");
    assert_eq!(receipt.trace[0].events.len(), 1);
    r.state.invariant(&r.scenario).unwrap();
}
#[test]
fn vf_exec_charge_before_step_exhaustion() {
    let mut tx = transaction(vec![transfer(), fail()]);
    tx.limit = 120.into();
    let r = execute(tx);
    let receipt = receipt(&r);
    assert_eq!(receipt.status, "ResourceExhausted");
    assert_eq!(receipt.used, 120.into());
    assert_eq!(receipt.fee, 240.into());
    assert_eq!(receipt.refund, Amount::ZERO);
    assert_eq!(r.state.balance("bob", "X"), Amount::ZERO);
    assert_eq!(receipt.trace[1].outcome, "ResourceExhausted");
    assert!(receipt.trace[1].events.is_empty());
}
#[test]
fn vf_exec_nested_savepoints_rollback_all_prior_calls() {
    let r = execute(transaction(vec![
        transfer(),
        Call::Sequence {
            calls: vec![transfer(), fail()],
        },
    ]));
    assert_eq!(receipt(&r).used, 150.into());
    assert_eq!(r.state.balance("bob", "X"), Amount::ZERO);
    assert!(receipt(&r).events.is_empty());
    assert!(
        receipt(&r)
            .trace
            .iter()
            .any(|t| t.path == "1.0" && t.outcome == "RolledBack")
    );
}
#[test]
fn vf_cpmm_swap_zero_fee_and_minimum() {
    let mut s = scenario();
    s.protocols.get_mut("cpmm").unwrap().fee_bps = 0;
    s.blocks.truncate(1);
    let mut r = Run::new(s.clone()).unwrap();
    r.finish().unwrap();
    assert_eq!(r.state.balance("pool", "X"), 1100.into());
    assert_eq!(r.state.balance("pool", "Q"), 910.into());
    assert_eq!(receipt(&r).used, 120.into());
    if let Call::SwapExactIn { min_out, .. } = &mut s.blocks[0][0].program[0] {
        *min_out = 91.into();
    }
    let mut failed = Run::new(s).unwrap();
    failed.finish().unwrap();
    assert_eq!(receipt(&failed).status, "ProtocolRevert");
    assert_eq!(failed.state.balance("pool", "X"), 1000.into());
}
#[test]
fn vf_cpmm_nonzero_fee_changes_output_analytically() {
    let mut s = scenario();
    s.accounts
        .get_mut("pool")
        .unwrap()
        .balances
        .insert("X".into(), 10000.into());
    s.accounts
        .get_mut("pool")
        .unwrap()
        .balances
        .insert("Q".into(), 10000.into());
    let w = World::genesis(&s).unwrap();
    assert_eq!(
        engine::quote(&s, &w, "cpmm", "X", 1000.into()).unwrap().1,
        906.into()
    );
    s.protocols.get_mut("cpmm").unwrap().fee_bps = 0;
    assert_eq!(
        engine::quote(&s, &w, "cpmm", "X", 1000.into()).unwrap().1,
        909.into()
    );
}
#[test]
fn vf_cpmm_late_failure_reverts_both_legs() {
    let s = scenario();
    let mut tx = s.blocks[0][0].clone();
    tx.program.push(fail());
    let r = execute(tx);
    assert_eq!(receipt(&r).used, 130.into());
    assert_eq!(receipt(&r).fee, 260.into());
    assert_eq!(r.state.balance("pool", "X"), 1000.into());
    assert_eq!(r.state.balance("pool", "Q"), 1000.into());
    assert!(receipt(&r).events.is_empty());
    assert_eq!(receipt(&r).trace[0].events.len(), 2);
}
#[test]
fn vf_cpmm_wide_product_boundary_and_random_invariants() {
    let mut s = scenario();
    let half = Amount::MAX.big() / 2u8;
    s.accounts
        .get_mut("pool")
        .unwrap()
        .balances
        .insert("X".into(), Amount::from_big(half.clone()).unwrap());
    s.accounts
        .get_mut("pool")
        .unwrap()
        .balances
        .insert("Q".into(), Amount::from_big(half).unwrap());
    let w = World::genesis(&s).unwrap();
    let (_, out) = engine::quote(
        &s,
        &w,
        "cpmm",
        "X",
        Amount::MAX.checked_sub(w.balance("pool", "X")).unwrap(),
    )
    .unwrap();
    assert!(
        out > w
            .balance("pool", "Q")
            .mul_div(1.into(), 3.into(), false)
            .unwrap()
    );
    // Independent integer inequality proves floor rounding without asking the implementation for its expected answer.
    let mut stream = Streams::new(42.into());
    for _ in 0..500 {
        let draw = stream
            .draw("fixture")
            .unwrap()
            .to_string()
            .parse::<u64>()
            .unwrap();
        let input = Amount::from(draw % 999 + 1);
        let s = scenario();
        let w = World::genesis(&s).unwrap();
        let (_, out) = engine::quote(&s, &w, "cpmm", "X", input).unwrap();
        let n = input.big() * 9970u32 * 1000u32;
        let d = 10000000u32 + input.big() * 9970u32;
        assert!(out.big() * &d <= n);
        assert!((out.big() + 1u32) * d > n);
        assert!((1000u32 + input.big()) * (1000u32 - out.big()) >= 1000000u32.into());
    }
}
#[test]
fn vf_auth_sender_and_protected_accounts() {
    let s = scenario();
    let w = World::genesis(&s).unwrap();
    let mut tx = transaction(vec![transfer()]);
    tx.sender = "bob".into();
    let block = engine::build_block(&s, &w, &[tx]).unwrap();
    assert!(block.receipts.is_empty());
    assert_eq!(block.state.accounts, w.accounts);
    let mut bad = s.clone();
    bad.chain.fee_account = "pool".into();
    assert!(bad.validate().is_err());
    let r = execute(transaction(vec![Call::Transfer {
        to: "pool".into(),
        asset: "X".into(),
        amount: 1.into(),
    }]));
    assert_eq!(receipt(&r).status, "ProtocolRevert");
    let mut value = serde_json::to_value(transaction(vec![transfer()])).unwrap();
    value["caller"] = serde_json::json!("pool");
    assert!(serde_json::from_value::<Transaction>(value).is_err());
}
#[test]
fn vf_auth_allowance_is_spent_and_rolled_back() {
    let mut s = scenario();
    let approve = transaction(vec![Call::Approve {
        spender: "bob".into(),
        asset: "X".into(),
        amount: 20.into(),
    }]);
    let mut spend = transaction(vec![
        Call::TransferFrom {
            from: "alice".into(),
            to: "bob".into(),
            asset: "X".into(),
            amount: 10.into(),
        },
        fail(),
    ]);
    spend.id = "spend".into();
    spend.sender = "bob".into();
    spend.agent = "recipient".into();
    spend.price = Amount::ZERO;
    s.blocks = vec![vec![approve, spend.clone()]];
    let mut r = Run::new(s.clone()).unwrap();
    r.finish().unwrap();
    assert_eq!(r.state.allowances["alice"]["bob"]["X"], 20.into());
    assert_eq!(r.state.balance("bob", "X"), Amount::ZERO);
    spend.id = "spend-success".into();
    spend.nonce = 1.into();
    spend.program.pop();
    s.blocks.push(vec![spend]);
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(r.state.allowances["alice"]["bob"]["X"], 10.into());
    assert_eq!(r.state.balance("bob", "X"), 10.into());
}
#[test]
fn vf_block_limit_admission_fee_budget_nonce_and_simulation() {
    let mut s = scenario();
    s.chain.block_limit = 175.into();
    let w = World::genesis(&s).unwrap();
    let mut tx = transaction(vec![transfer()]);
    tx.limit = 180.into();
    let block = engine::build_block(&s, &w, &[tx.clone()]).unwrap();
    assert!(block.receipts.is_empty());
    assert_eq!(block.used, Amount::ZERO);
    assert_eq!(block.state.accounts, w.accounts);
    tx.limit = 170.into();
    let first = engine::build_block(&s, &w, &[tx.clone(), tx.clone()]).unwrap();
    assert_eq!(first.receipts.len(), 1);
    assert_eq!(first.rejected.len(), 1);
    assert_eq!(first.used, 115.into());
    assert_eq!(w.accounts["alice"].nonce, Amount::ZERO);
    tx.nonce = 1.into();
    assert!(
        engine::build_block(&s, &w, &[tx.clone()])
            .unwrap()
            .receipts
            .is_empty()
    );
    tx.nonce = Amount::ZERO;
    tx.price = 10000.into();
    assert!(
        engine::build_block(&s, &w, &[tx])
            .unwrap()
            .receipts
            .is_empty()
    );
}
#[test]
fn vf_block_inclusion_rechecks_funding_after_prior_transaction() {
    let mut s = scenario();
    s.accounts
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("Q".into(), Amount::ZERO);
    let a = transaction(vec![Call::Transfer {
        to: "bob".into(),
        asset: "Q".into(),
        amount: 400.into(),
    }]);
    let mut b = transaction(vec![Call::Transfer {
        to: "alice".into(),
        asset: "X".into(),
        amount: Amount::ZERO,
    }]);
    b.id = "bob".into();
    b.sender = "bob".into();
    b.agent = "recipient".into();
    s.blocks = vec![vec![a, b]];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(r.blocks[0].receipts.len(), 2);
    assert_eq!(r.state.accounts["bob"].nonce, 1.into());
}
#[test]
fn vf_input_validation_and_program_limits() {
    assert!(Scenario::parse(b"{broken").is_err());
    let mut s = scenario();
    s.blocks[0][0].limit = 99.into();
    assert!(s.validate().is_err());
    let s = scenario();
    let mut tx = transaction(vec![transfer(); 257]);
    assert!(s.validate_tx(&tx).is_err());
    tx.program = vec![transfer(); 256];
    assert!(s.validate_tx(&tx).is_ok());
    let mut c = transfer();
    for _ in 0..31 {
        c = Call::Sequence { calls: vec![c] };
    }
    tx.program = vec![c.clone()];
    assert!(s.validate_tx(&tx).is_ok());
    tx.program = vec![Call::Sequence { calls: vec![c] }];
    assert!(s.validate_tx(&tx).is_err());
    tx.program = vec![
        Call::Sequence {
            calls: vec![transfer(); 256]
        };
        4
    ];
    assert!(s.validate_tx(&tx).is_err());
    tx.program = vec![Call::AssertCondition {
        condition: true,
        message: "a".repeat(1024 * 1024),
    }];
    assert!(s.validate_tx(&tx).is_err());
}
#[test]
fn vf_checkpoint_replay_resume_fork_and_tamper() {
    let mut full = Run::new(scenario()).unwrap();
    full.finish().unwrap();
    let mut partial = Run::new(scenario()).unwrap();
    partial.step().unwrap();
    let bytes = partial.checkpoint().unwrap();
    let mut resumed = Run::restore(&bytes).unwrap();
    resumed.finish().unwrap();
    assert_eq!(full, resumed);
    let mut fork = partial.fork("alternative").unwrap();
    assert_ne!(partial.run_id, fork.run_id);
    fork.finish().unwrap();
    assert_eq!(full.state, fork.state);
    assert_eq!(full.blocks, fork.blocks);
    let mut bad: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    bad["run"]["state"]["accounts"]["alice"]["balances"]["Q"] = serde_json::json!("1");
    assert!(Run::restore(&serde_json::to_vec(&bad).unwrap()).is_err());
    // A forged matching checksum must still fail semantic replay validation.
    let run: Run = serde_json::from_value(bad["run"].clone()).unwrap();
    bad["hash"] = serde_json::json!(digest(&serde_json::to_vec(&run).unwrap()));
    assert!(Run::restore(&serde_json::to_vec(&bad).unwrap()).is_err());
    let mut invalid = partial.clone();
    Arc::make_mut(&mut invalid.state.accounts)
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), 1.into());
    assert!(invalid.checkpoint().is_err());
}
#[test]
fn vf_rng_independence_and_exact_stream_restore() {
    let mut a = Streams::new(42.into());
    let mut b = a.clone();
    for _ in 0..100 {
        a.draw("agent/trader").unwrap();
    }
    for _ in 0..20 {
        assert_eq!(
            a.draw("environment").unwrap(),
            b.draw("environment").unwrap()
        );
    }
    let mut restored: Streams = serde_json::from_slice(&serde_json::to_vec(&a).unwrap()).unwrap();
    for _ in 0..20 {
        assert_eq!(
            a.draw("agent/trader").unwrap(),
            restored.draw("agent/trader").unwrap()
        );
    }
}
#[test]
fn vf_genesis_rejects_duplicate_protected_accounts_and_invalid_assets() {
    let mut s = scenario();
    let p = s.protocols["cpmm"].clone();
    s.protocols.insert("duplicate".into(), p);
    assert!(s.validate().is_err());
    let mut s = scenario();
    s.protocols.get_mut("cpmm").unwrap().fee_bps = 10000;
    assert!(s.validate().is_err());
    let mut s = scenario();
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("unknown".into(), 1.into());
    assert!(s.validate().is_err());
}

#[test]
fn vf_supply_capacity_and_charge_breakdown() {
    let mut s = scenario();
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), Amount::MAX);
    assert!(s.validate().unwrap_err().contains("initial supply"));
    let mut tx = transaction(vec![transfer(), fail()]);
    tx.limit = 120.into();
    let r = execute(tx);
    let charges = &receipt(&r).charges;
    assert_eq!(
        charges
            .iter()
            .map(|c| c.charged.to_string())
            .collect::<Vec<_>>(),
        ["100", "10", "5", "5"]
    );
    assert_eq!(charges[3].requested, 10.into());
    assert_eq!(charges[3].total, 120.into());
}

#[test]
fn vf_wire_rational_price_is_positive_and_exact() {
    use chainarium_core::amount::Price;
    let p = Price::new(
        "X".into(),
        "Q".into(),
        Amount::MAX,
        "9007199254740993".parse().unwrap(),
    )
    .unwrap();
    let bytes = serde_json::to_vec(&p).unwrap();
    assert_eq!(serde_json::from_slice::<Price>(&bytes).unwrap(), p);
    assert!(Price::new("X".into(), "Q".into(), 1.into(), Amount::ZERO).is_err());
    assert!(Price::new("X".into(), "X".into(), 1.into(), 1.into()).is_err());
}

#[test]
fn vf_export_roundtrip_manifest_events_csv_and_no_overwrite() {
    let mut run = Run::new(scenario()).unwrap();
    run.finish().unwrap();
    let root = std::env::temp_dir().join(format!("chainarium-export-test-{}", std::process::id()));
    // This test owns a unique process directory; fail rather than delete someone else's data.
    run.export(&root).unwrap();
    let restored_scenario =
        Scenario::parse(&std::fs::read(root.join("scenario.json")).unwrap()).unwrap();
    let mut replay = Run::new(restored_scenario).unwrap();
    replay.finish().unwrap();
    assert_eq!(run, replay);
    let resumed = Run::restore(&std::fs::read(root.join("checkpoint.json")).unwrap()).unwrap();
    assert_eq!(run, resumed);
    let events = std::fs::read_to_string(root.join("events.jsonl")).unwrap();
    assert_eq!(events.lines().count(), 2);
    let csv = std::fs::read_to_string(root.join("transactions.csv")).unwrap();
    assert_eq!(csv.lines().count(), 4);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["build"], chainarium_core::BUILD_ID);
    assert!(run.export(&root).is_err());
    let cp = root.join("saved.json");
    chainarium_core::run::save_checkpoint(&cp, &run).unwrap();
    assert!(chainarium_core::run::save_checkpoint(&cp, &run).is_err());
    assert_eq!(Run::restore(&std::fs::read(&cp).unwrap()).unwrap(), run);
    std::fs::remove_dir_all(root).unwrap();
}
