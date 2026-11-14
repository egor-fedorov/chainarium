use chainarium_core::{explorer as e, valuation::Mark, *};
use serde_json::{Value, json};
use std::path::PathBuf;

fn fixture(path: &str) -> Value {
    serde_json::from_slice(
        &std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../{path}.json")),
        )
        .unwrap(),
    )
    .unwrap()
}
fn run(s: &Value) -> Run {
    let mut r = Run::new(Scenario::parse(&serde_json::to_vec(s).unwrap()).unwrap()).unwrap();
    r.finish().unwrap();
    r
}
fn history(s: &Value) -> e::History {
    e::History::capture(run(s)).unwrap()
}
fn pair(name: &str) -> (e::History, e::History) {
    (
        history(&fixture(&format!("scenarios/explorer/{name}-a"))),
        history(&fixture(&format!("scenarios/explorer/{name}-b"))),
    )
}
fn value(m: &Mark) -> Value {
    serde_json::to_value(&m.value).unwrap()
}
fn metric<'a>(c: &'a e::Comparison, name: &str) -> &'a e::MetricPair {
    c.metrics.iter().find(|m| m.metric == name).unwrap()
}
fn reference(name: &str) -> Value {
    fixture("docs/reviews/explorer-reference")["cases"][name].clone()
}
fn balance(a: &e::History, b: &e::History, t: Amount) -> e::BalancePair {
    e::balances(a, Some(b), t, "alice", 0)
        .unwrap()
        .items
        .into_iter()
        .find(|b| b.asset == "Q")
        .unwrap()
}

#[test]
fn synchronous_matches_preimplementation_reference_and_exact_hodl() {
    let (a, b) = pair("synchronous");
    let expected = reference("synchronous");
    assert_eq!(
        serde_json::to_value(e::index(&a, Some(&b)).unwrap().times).unwrap(),
        expected["times"]
    );
    for (i, t) in [0, 12, 24].into_iter().enumerate() {
        let c = e::compare(&a, Some(&b), t.into()).unwrap();
        let wallet = balance(&a, &b, t.into());
        assert_eq!(json!([wallet.a, wallet.b]), expected["balances"][i]);
        assert_eq!(value(&wallet.difference), expected["delta"][i]);
        assert_eq!(value(&metric(&c, "pnl").difference), expected["delta"][i]);
        assert_eq!(
            value(&metric(&c, "excess_pnl").difference),
            expected["delta"][i]
        );
        assert!(c.reasons.is_empty());
    }
    let c = e::compare(&a, Some(&b), 24.into()).unwrap();
    assert_eq!(
        value(metric(&c, "pnl").a.as_ref().unwrap()),
        expected["pnl_a"]
    );
    assert_eq!(
        value(metric(&c, "pnl").b.as_ref().unwrap()),
        expected["pnl_b"]
    );
}

#[test]
fn union_timeline_uses_past_snapshots_and_disables_unsynchronized_metrics() {
    let (a, b) = pair("as_of");
    let ex = reference("as_of");
    assert_eq!(json!(e::index(&a, Some(&b)).unwrap().times), ex["times"]);
    for (i, t) in [0, 10, 12, 20].into_iter().enumerate() {
        let c = e::compare(&a, Some(&b), t.into()).unwrap();
        let pb = c.b.as_ref().unwrap();
        assert_eq!(
            json!([c.a.block.to_string(), pb.block.to_string()]),
            ex["blocks"][i]
        );
        assert_eq!(json!([c.a.age, pb.age]), ex["ages"][i]);
        assert_eq!(
            value(&balance(&a, &b, t.into()).difference),
            ex["wallet_delta"][i]
        );
        assert_eq!(value(&metric(&c, "pnl").difference), ex["pnl_delta"][i]);
        assert_eq!(c.reasons.contains("BoundaryTimeMismatch"), i != 0);
    }
    assert!(e::compare(&a, Some(&b), 24.into()).is_err());
    assert!(e::compare(&a, Some(&b), 11.into()).is_err());
    assert!(e::balances(&a, Some(&b), 24.into(), "alice", 0).is_err());
}

#[test]
fn unknown_history_does_not_cancel_unknown_pnl_terms() {
    let (a, b) = pair("unknown_history");
    let ex = reference("unknown_history");
    let c = e::compare(&a, Some(&b), 2.into()).unwrap();
    for (name, ka, kb, kd) in [
        ("value", "actual_a", "actual_b", "value_delta"),
        ("pnl", "pnl_a", "pnl_b", "pnl_delta"),
    ] {
        let m = metric(&c, name);
        assert_eq!(value(m.a.as_ref().unwrap()), ex[ka]);
        assert_eq!(value(m.b.as_ref().unwrap()), ex[kb]);
        assert_eq!(value(&m.difference), ex[kd]);
    }
    assert!(
        metric(&c, "pnl")
            .difference
            .reasons
            .contains("UnpricedLeft")
    );
    assert!(
        metric(&c, "pnl")
            .difference
            .reasons
            .contains("UnpricedRight")
    );
}

#[test]
fn wide_atoms_and_negative_fraction_never_round_through_f64() {
    let (a, b) = pair("wide_fraction");
    let ex = reference("wide_fraction");
    let c = e::compare(&a, Some(&b), 12.into()).unwrap();
    let v = metric(&c, "value");
    assert_eq!(value(v.a.as_ref().unwrap()), ex["actual_a"]);
    assert_eq!(value(v.b.as_ref().unwrap()), ex["actual_b"]);
    assert_eq!(value(&metric(&c, "pnl").difference), ex["pnl_delta"]);
    assert_eq!(json!(balance(&a, &b, 12.into()).a), ex["wallet_q"]);
}

#[test]
fn decimals_mismatch_is_not_a_zero_difference() {
    let (a, b) = pair("different_decimals");
    let ex = reference("different_decimals");
    let c = e::compare(&a, Some(&b), 12.into()).unwrap();
    let wallet = balance(&a, &b, 12.into());
    assert_eq!(json!(wallet.a), ex["left_atoms"]);
    assert_eq!(json!(wallet.b), ex["right_atoms"]);
    assert!(wallet.difference.value.is_none());
    assert!(wallet.difference.reasons.contains("AssetUnitsMismatch"));
    assert!(metric(&c, "value").difference.value.is_none());
}

#[test]
fn shorter_prefix_and_single_history_have_no_future_extrapolation() {
    let (a, b) = pair("short_prefix");
    let ex = reference("short_prefix");
    assert_eq!(json!(e::index(&a, Some(&b)).unwrap().times), ex["common"]);
    assert!(e::compare(&a, Some(&b), 20.into()).is_err());
    assert_eq!(json!(e::index(&a, None).unwrap().times), ex["times_a"]);
    let c = e::compare(&b, None, 10.into()).unwrap();
    assert!(c.b.is_none());
    assert!(c.reasons.contains("NoRightHistory"));
    assert!(metric(&c, "pnl").a.as_ref().unwrap().value.is_some());
    assert!(metric(&c, "pnl").difference.value.is_none());
    assert!(a.world(3).is_err());
}

#[test]
fn chain_and_policies_gate_only_the_applicable_differences() {
    let s = fixture("scenarios/explorer/synchronous-a");
    let a = history(&s);
    let mut t = s.clone();
    t["valuation_policy"]["max_age_seconds"] = json!("61");
    let b = history(&t);
    let c = e::compare(&a, Some(&b), 12.into()).unwrap();
    assert!(
        metric(&c, "pnl")
            .difference
            .reasons
            .contains("ValuationPolicyMismatch")
    );
    assert!(balance(&a, &b, 12.into()).difference.value.is_some());
    let mut t = s.clone();
    t["hodl_policy"]["limits"]["rational_bits"] = json!(1024);
    let b = history(&t);
    let c = e::compare(&a, Some(&b), 12.into()).unwrap();
    assert!(metric(&c, "pnl").difference.value.is_some());
    assert!(
        metric(&c, "excess_pnl")
            .difference
            .reasons
            .contains("HodlPolicyMismatch")
    );
    let mut t = s.clone();
    t["chain"]["id"] = json!("another");
    t["blocks"][0][0]["chain"] = json!("another");
    let b = history(&t);
    assert!(
        balance(&a, &b, 12.into())
            .difference
            .reasons
            .contains("ChainMismatch")
    );
    assert!(
        e::compare(&a, Some(&b), 12.into())
            .unwrap()
            .reasons
            .contains("ChainMismatch")
    );
}

#[test]
fn missing_account_is_not_an_empty_existing_balance() {
    let mut s = fixture("scenarios/explorer/synchronous-a");
    s["accounts"]["orphan"] = json!({"nonce":"0","balances":{"Q":"7"}});
    let a = history(&s);
    let mut t = s.clone();
    t["accounts"]["orphan"]["balances"] = json!({});
    let b = history(&t);
    let v = e::balances(&a, Some(&b), 12.into(), "orphan", 0)
        .unwrap()
        .items
        .remove(0);
    assert_eq!(v.b, Some(0.into()));
    assert_eq!(value(&v.difference)["numerator"], "-7");
    t["accounts"].as_object_mut().unwrap().remove("orphan");
    let b = history(&t);
    let v = e::balances(&a, Some(&b), 12.into(), "orphan", 0)
        .unwrap()
        .items
        .remove(0);
    assert!(v.b.is_none());
    assert!(v.difference.reasons.contains("MissingAccount"));
    assert!(e::balances(&a, Some(&b), 12.into(), "unknown", 0).is_err());
}

#[test]
fn immutable_capture_keys_include_prefix_and_restoration_replays() {
    let s = fixture("scenarios/explorer/synchronous-a");
    let mut live = Run::new(serde_json::from_value(s).unwrap()).unwrap();
    let genesis = live.checkpoint().unwrap();
    let a = e::History::capture(live.clone()).unwrap();
    live.step().unwrap();
    let b = e::History::capture(live.clone()).unwrap();
    assert_eq!(a.run.run_id, b.run.run_id);
    assert_ne!(a.key, b.key);
    assert_eq!(a.run.checkpoint().unwrap(), genesis);
    assert!(a.ensure_key(&b.key).is_err());
    let bytes = live.checkpoint().unwrap();
    let imported = e::History::restore(&bytes).unwrap();
    assert_eq!(imported.key, b.key);
    let mut raw: Value = serde_json::from_slice(&bytes).unwrap();
    raw["run"]["state"]["timestamp"] = json!("999");
    assert!(e::History::restore(&serde_json::to_vec(&raw).unwrap()).is_err());
    assert!(e::History::restore(b"{}").is_err());
    assert!(e::History::restore(&vec![b' '; e::MAX_CHECKPOINT + 1]).is_err());
    let pretty =
        serde_json::to_vec_pretty(&serde_json::from_slice::<Value>(&bytes).unwrap()).unwrap();
    assert_eq!(e::History::restore(&pretty).unwrap().key, b.key);
}

#[test]
fn historical_observations_use_saved_delivery_state_and_own_accounts() {
    for mode in ["public", "private", "delayed"] {
        let s = fixture(&format!("scenarios/demos/sc-scheduler-{mode}"));
        let h = history(&s);
        let before = h.run.checkpoint().unwrap();
        let genesis = h.observation(0, "trader", "alice").unwrap();
        assert!(genesis.messages.is_empty());
        assert!(genesis.own_outbox.is_empty());
        let early = h.observation(1, "trader", "alice").unwrap();
        let later = h.observation(3, "trader", "alice").unwrap();
        assert_eq!(early.market.block, 1.into());
        assert_eq!(later.market.block, 3.into());
        assert_eq!(
            early.messages.iter().any(|m| m.transaction.id == "victim"),
            mode != "private"
        );
        assert!(!early.messages.iter().any(|m| m.delivered_at > 10.into()));
        assert!(h.observation(1, "trader", "bob").is_err());
        assert!(h.observation(4, "trader", "alice").is_err());
        let raw = serde_json::to_value(&early).unwrap();
        assert!(raw["market"].get("accounts").is_none());
        assert!(raw.to_string().find("reference_feeds").is_none());
        assert_eq!(h.run.checkpoint().unwrap(), before);
    }
}

#[test]
fn paid_revert_distinguishes_canonical_events_from_attempt_transfers() {
    let mut s = fixture("scenarios/explorer/synchronous-a");
    s["accounts"]["alice"]["balances"]["Q"] = json!("1000");
    s["blocks"][0][0]["price"] = json!("1");
    s["blocks"][0][0]["program"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"Transfer","to":"bob","asset":"X","amount":"1"}));
    let h = history(&s);
    let native = &h.run.blocks[0].receipts[0];
    assert_eq!(native.status, "ProtocolRevert");
    let r = e::receipt_view(&h, 1, 0, 0, 0).unwrap();
    assert_eq!(r.canonical_events.items, native.events);
    assert!(r.receipt.fee > 0.into());
    assert_eq!(h.world(1).unwrap().accounts["alice"].nonce, 1.into());
    assert_eq!(h.world(1).unwrap().balance("bob", "Q"), 0.into());
    let mut attempted = false;
    for call in &r.calls.items {
        let t = e::call_view(&h, 1, 0, call.index, 0).unwrap();
        assert!(!t.committed);
        assert_eq!(t.diagnostic["path"], call.path);
        assert!(t.diagnostic.get("events").is_none());
        attempted |= t
            .attempt_events
            .items
            .iter()
            .any(|e| e.to == "bob" && e.amount == 10.into());
    }
    assert!(attempted);
    assert!(e::receipt_view(&h, 0, 0, 0, 0).is_err());
    assert!(e::call_view(&h, 1, 0, 999, 0).is_err());
}

#[test]
fn receipts_keep_execution_order_and_actual_call_indices() {
    let h = history(&fixture("scenarios/demos/sc-hodl-ordered"));
    let rows = e::receipts(&h, 1, 0).unwrap();
    assert_eq!(
        rows.items
            .iter()
            .map(|r| r.tx_id.as_str())
            .collect::<Vec<_>>(),
        ["z-in", "a-out"]
    );
    for row in rows.items {
        let v = e::receipt_view(&h, 1, row.index, 0, 0).unwrap();
        assert_eq!(v.receipt, row);
    }
    assert!(e::receipts(&h, 0, 0).unwrap().items.is_empty());
    assert!(e::receipts(&h, 3, 0).is_err());
}

#[test]
fn entity_catalog_and_graph_point_to_real_configured_objects() {
    for name in [
        "sc-twap",
        "sc-02-clmm",
        "sc-08-book",
        "sc-07-flash-liquidation",
        "sc-hook-lp",
        "sc-hodl-credit",
    ] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../scenarios/demos/{name}.json"));
        // Names are explicit fixture inputs; missing inputs must fail, not silently skip.
        let s: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let h = history(&s);
        let g = e::graph(&h);
        assert!(!g.truncated);
        assert_eq!(g.nodes.len(), g.total_nodes);
        assert_eq!(g.edges.len(), g.total_edges);
        for edge in g.edges {
            assert!(g.nodes.contains(&edge.from));
            assert!(g.nodes.contains(&edge.to));
        }
        for ent in e::entities(&h, 0).unwrap().items {
            let v = e::entity_view(&h, h.run.blocks.len(), &ent, 0).unwrap();
            assert_eq!(v["entity"], json!(ent));
        }
        assert!(
            e::entity_view(
                &h,
                0,
                &e::Entity {
                    kind: e::Kind::Account,
                    id: "missing".into()
                },
                0
            )
            .is_err()
        );
    }
}

#[test]
fn bounded_pages_graph_and_report_serialization_are_explicit() {
    let p = e::page(0..129, 64).unwrap();
    assert_eq!(p.items, (64..128).collect::<Vec<_>>());
    assert_eq!(p.next, Some(128));
    assert_eq!(p.total, 129);
    assert!(e::page(0..129, 129).unwrap().items.is_empty());
    assert!(e::page(0..129, 130).is_err());
    assert!(e::page(0..129, usize::MAX).is_err());
    let mut s = fixture("scenarios/explorer/synchronous-a");
    for i in 0..300 {
        s["assets"][format!("unused-{i:03}")] = json!({"decimals":0});
    }
    let h = history(&s);
    let g = e::graph(&h);
    assert!(g.truncated);
    assert_eq!(g.nodes.len(), 256);
    assert_eq!(g.total_nodes, 302);
    assert_eq!(e::entities(&h, 0).unwrap().items.len(), 64);
    assert!(e::entities(&h, 64).unwrap().next.is_some());
    assert!(
        e::report_bytes(&"x".repeat(e::MAX_REPORT))
            .unwrap_err()
            .contains("8 MiB")
    );
    assert!(e::report_bytes(&"x".repeat(e::MAX_REPORT - 2)).is_ok());
}
