use chainarium_core::{
    hodl::{self as h, Ledger},
    valuation as v, *,
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

fn fixture(name: &str) -> Value {
    serde_json::from_slice(
        &std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../scenarios/demos/{name}.json")),
        )
        .unwrap(),
    )
    .unwrap()
}
fn parse(s: &Value) -> Scenario {
    Scenario::parse(&serde_json::to_vec(s).unwrap()).unwrap()
}
fn finish(s: &Value) -> Run {
    let mut r = Run::new(parse(s)).unwrap();
    r.finish().unwrap();
    r
}
fn view(r: &Run) -> h::View {
    h::at(r, r.blocks.len()).unwrap().portfolios.remove(0)
}
fn value(m: &v::Mark) -> Value {
    serde_json::to_value(&m.value).unwrap()
}
fn n(m: &v::Mark, n: &str) {
    assert_eq!(
        value(m),
        json!({"numerator":n,"denominator":"1","floor_atoms":n})
    );
}
fn temp() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "chainarium-hodl-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}
fn enable(s: &mut Value, accounts: &[&str]) {
    s["valuation_policy"] = fixture("sc-mtm-capital")["valuation_policy"].clone();
    s["valuation_policy"]["portfolios"] = json!({"owner":{"accounts":accounts}});
    s["valuation_policy"]["capital_transfers"] = json!([]);
    if s["assets"].get("X").is_none() {
        s["valuation_policy"]["reference_feeds"] = json!({});
    }
    s["hodl_policy"] = fixture("sc-hodl-capital")["hodl_policy"].clone();
    s["model_versions"]
        .as_array_mut()
        .unwrap()
        .extend([json!(v::MODEL), json!(h::MODEL)]);
}
fn label(s: &mut Value, id: &str) {
    s["valuation_policy"]["capital_transfers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"portfolio":"owner","tx_id":id}));
}
fn simple() -> Value {
    let mut s = fixture("sc-hodl-ordered");
    s["blocks"] = json!([]);
    s["valuation_policy"]["capital_transfers"] = json!([]);
    s["accounts"]["alice"]["balances"] = json!({"X":"0","Q":"0"});
    s
}
fn transfer(id: &str, owner: &str, to: &str, asset: &str, amount: u64, nonce: u64) -> Value {
    json!({"id":id,"agent":if owner=="alice" {"trader"} else {"recipient"},"sender":owner,"chain":"lab-1","nonce":nonce.to_string(),"limit":"200","price":"0","deadline":"100","program":[{"type":"Transfer","to":to,"asset":asset,"amount":amount.to_string()}]})
}

#[test]
fn native_demos_match_preimplementation_marks_and_report_canonical_order() {
    let reference: Value =
        serde_json::from_slice(include_bytes!("../../../docs/reviews/hodl-reference.json"))
            .unwrap();
    for (name, case) in [
        ("lp", "cpmm_price_shock"),
        ("capital", "dynamic_inflow"),
        ("ordered", "canonical_order_in_then_out"),
        ("unpriced", "historical_unpriced_flow"),
    ] {
        let r = finish(&fixture(&format!("sc-hodl-{name}")));
        let p = view(&r);
        let expected = &reference["cases"][case];
        for (key, m) in [
            ("current", &p.hodl.mark),
            ("initial", &p.initial),
            ("actual_pnl", &p.actual_pnl),
            ("hodl_pnl", &p.hodl_pnl),
            ("excess_pnl", &p.excess_pnl),
            ("value_gap", &p.value_gap),
        ] {
            assert_eq!(value(m), expected[key]["value"], "{name} {key}");
        }
        if name == "ordered" {
            assert_eq!(
                p.flows
                    .iter()
                    .map(|f| f.cursor.tx_id.as_str())
                    .collect::<Vec<_>>(),
                ["z-in", "a-out"]
            );
            assert_eq!(
                p.flows
                    .iter()
                    .map(|f| f.cursor.receipt_index)
                    .collect::<Vec<_>>(),
                [0, 1]
            );
            assert_eq!(v::at(&r, 1).unwrap().capital_flows[0].tx_id, "a-out");
        }
    }
    let p = view(&finish(&fixture("sc-hodl-fractional")));
    assert_eq!(
        value(&p.excess_pnl),
        json!({"numerator":"-1","denominator":"2","floor_atoms":"-1"})
    );
}

#[test]
fn canonical_infeasibility_is_permanent_and_last_feasible_has_an_honest_cursor() {
    let r = finish(&fixture("sc-hodl-infeasible"));
    let p = view(&r);
    let Ledger::Unavailable {
        failure,
        last_feasible: Some(last),
    } = &p.ledger
    else {
        panic!("{:?}", p.ledger)
    };
    assert_eq!(failure.reason, "InfeasibleOutflow");
    assert_eq!(
        last.at,
        h::Point::Boundary {
            block: 0.into(),
            timestamp: 0.into()
        }
    );
    assert_eq!(last.quantities.len(), 1);
    let h::Point::Flow { cursor } = &failure.at else {
        panic!()
    };
    assert_eq!(cursor.receipt_index, 1);
    assert_eq!(cursor.tx_id, "a-out");
    assert_eq!(h::at(&r, 1).unwrap().portfolios[0].ledger, p.ledger);
    assert!(p.hodl.mark.value.is_none());
    assert!(p.hodl.assets.is_empty());
    assert!(p.excess_pnl.value.is_none());
    assert_eq!(p.flow_events, 2.into());
    let flows = h::at(&r, 1).unwrap().portfolios.remove(0).flows;
    assert!(matches!(&flows[1].effect,h::Effect::Skipped{reason} if reason=="InfeasibleOutflow"));
    // An earlier successful capital event must be identified as the last point,
    // rather than pretending that all flows in this block were applied.
    let mut s = fixture("sc-hodl-infeasible");
    s["blocks"][0]
        .as_array_mut()
        .unwrap()
        .insert(0, transfer("q-out", "alice", "bob", "Q", 1, 0));
    s["blocks"][0][2]["nonce"] = json!("1");
    label(&mut s, "q-out");
    let p = view(&finish(&s));
    let Ledger::Unavailable {
        last_feasible: Some(last),
        ..
    } = p.ledger
    else {
        panic!()
    };
    assert!(matches!(last.at,h::Point::Flow{cursor} if cursor.tx_id=="q-out"));
    assert_eq!(last.quantities["Q"], v::Exact::integer(99.into()));
}

#[test]
fn withdrawals_do_not_trade_other_assets_or_double_charge_native_fees() {
    let mut s = simple();
    s["accounts"]["alice"]["balances"]["Q"] = json!("1000");
    let mut t = transfer("capital", "alice", "bob", "Q", 900, 0);
    t["price"] = json!("1");
    s["blocks"] = json!([[t]]);
    label(&mut s, "capital");
    let r = finish(&s);
    assert_eq!(r.blocks[0].receipts[0].status, "ProtocolRevert");
    let p = view(&r);
    n(&p.excess_pnl, "-115");
    n(&p.hodl_pnl, "0");
    assert_eq!(p.flow_events, 0.into());
    s["blocks"][0][0]["nonce"] = json!("1");
    let r = finish(&s);
    assert!(r.blocks[0].receipts.is_empty());
    n(&view(&r).excess_pnl, "0");
    s["blocks"][0][0]["nonce"] = json!("0");
    s["blocks"][0][0]["program"][0]["amount"] = json!("100");
    let p = view(&finish(&s));
    n(&p.hodl.mark, "900");
    n(&p.excess_pnl, "-115");
    n(&p.hodl_pnl, "0");
    assert_eq!(p.flow_events, 1.into());
    s["valuation_policy"]["capital_transfers"] = json!([]);
    let p = view(&finish(&s));
    n(&p.hodl.mark, "1000");
    n(&p.excess_pnl, "-215");
    // Actual strategy sells X for Q and then withdraws Q. HODL owns X only.
    let mut s = fixture("sc-hodl-lp");
    s["protocols"]["cpmm"]["owner"] = json!("bob");
    s["accounts"]["alice"]["balances"] = json!({"X":"10","Q":"0"});
    let mut trade = transfer("sale", "alice", "bob", "Q", 0, 0);
    trade["program"] =
        json!([{"type":"SwapExactIn","pool":"cpmm","asset_in":"X","amount":"2","min_out":"1"}]);
    s["blocks"] = json!([[trade, transfer("withdraw", "alice", "bob", "Q", 1, 1)]]);
    label(&mut s, "withdraw");
    let r = finish(&s);
    assert!(r.blocks[0].receipts.iter().all(|r| r.status == "Success"));
    assert!(
        matches!(view(&r).ledger,Ledger::Unavailable{failure:h::Failure{reason,..},..} if reason=="InfeasibleOutflow")
    );
}

#[test]
fn price_freshness_recovery_missing_initial_history_and_zero_inventory() {
    let mut s = simple();
    s["accounts"]["alice"]["balances"]["X"] = json!("1");
    s["blocks"] = json!([[], [], []]);
    let feed = &mut s["valuation_policy"]["reference_feeds"]["X"]["records"];
    let mut later = feed[0].clone();
    later["available_at"] = json!("3");
    later["observed_at"] = json!("3");
    later["sequence"] = json!("1");
    feed.as_array_mut().unwrap().push(later);
    s["valuation_policy"]["max_age_seconds"] = json!("1");
    let r = finish(&s);
    let stale = &h::at(&r, 2).unwrap().portfolios[0];
    assert!(matches!(stale.ledger, Ledger::Ready { .. }));
    assert!(stale.hodl.mark.reasons.contains("Stale"));
    n(&view(&r).excess_pnl, "0");
    // Known quantity survives a missing initial price; current gap may be known.
    s["valuation_policy"]["reference_feeds"]["X"]["records"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    let p = view(&finish(&s));
    n(&p.hodl.mark, "10");
    n(&p.value_gap, "0");
    assert!(p.hodl_pnl.reasons.contains("InitialUnpriced"));
    assert!(p.excess_pnl.value.is_none());
    // Future observation never uses an earlier record as a fallback.
    s["valuation_policy"]["reference_feeds"]["X"]["records"][0]["observed_at"] = json!("4");
    let p = view(&finish(&s));
    assert!(p.hodl.mark.reasons.contains("FutureObservation"));
    let mut s = simple();
    s["accounts"]["alice"]["balances"] = json!({"X":"2","Q":"100"});
    s["blocks"] = json!([[transfer("all", "alice", "bob", "X", 2, 0)], []]);
    label(&mut s, "all");
    s["valuation_policy"]["max_age_seconds"] = json!("1");
    let p = view(&finish(&s));
    n(&p.hodl.mark, "100");
    n(&p.hodl_pnl, "0");
    assert!(!p.hodl.assets.contains_key("X"));
}

#[test]
fn portfolio_flow_and_component_caps_are_local_and_cumulative() {
    let mut s = fixture("sc-hodl-ordered");
    let second = s["blocks"][0].as_array_mut().unwrap().pop().unwrap();
    s["blocks"].as_array_mut().unwrap().push(json!([second]));
    s["hodl_policy"]["limits"]["flow_events_per_portfolio"] = json!(1);
    s["valuation_policy"]["portfolios"]["outside"] = json!({"accounts":["bob"]});
    s["hodl_policy"]["portfolios"]
        .as_array_mut()
        .unwrap()
        .push(json!("outside"));
    let r = finish(&s);
    let b = h::at(&r, 2).unwrap();
    let p = b
        .portfolios
        .iter()
        .find(|p| p.portfolio == "owner")
        .unwrap();
    assert!(
        matches!(&p.ledger,Ledger::Unavailable{failure:h::Failure{reason,..},last_feasible:Some(_)} if reason=="FlowLimit")
    );
    assert_eq!(p.flow_events, 2.into());
    assert!(p.hodl.mark.truncated);
    assert!(matches!(
        b.portfolios
            .iter()
            .find(|p| p.portfolio == "outside")
            .unwrap()
            .ledger,
        Ledger::Ready { .. }
    ));
    s["valuation_policy"]["limits"]["component_checks_per_portfolio"] = json!(1);
    let p = view(&finish(&s));
    assert!(p.initial_holdings.is_none());
    assert!(p.hodl.mark.reasons.contains("ComponentLimit"));
    s["valuation_policy"]["limits"]["component_checks_per_portfolio"] = json!(4096);
    s["hodl_policy"]["limits"]["assets_per_portfolio"] = json!(1);
    let b = h::at(&finish(&s), 1).unwrap();
    let p = b
        .portfolios
        .iter()
        .find(|p| p.portfolio == "owner")
        .unwrap();
    assert!(p.hodl.mark.reasons.contains("AssetLimit"));
}

#[test]
fn bundle_probes_reverts_and_delayed_delivery_use_only_actual_successful_capital() {
    for mode in ["success", "forbidden", "allowed"] {
        let mut s = fixture("sc-bundle-visible");
        enable(&mut s, &["alice"]);
        label(&mut s, "victim");
        s["scheduler"]["builder"] =
            json!({"type":"FixedItems","priority":[{"type":"Bundle","id":"visible-group"}]});
        s["scheduler"]["emissions"][0]["transaction"]["program"] =
            json!([{"type":"Transfer","to":"alice","asset":"Q","amount":"20"}]);
        let bundle = &mut s["scheduler"]["bundles"]["emissions"][0]["bundle"];
        bundle["members"].as_array_mut().unwrap().remove(1);
        bundle["members"][1]["transaction"]["nonce"] =
            json!(if mode == "forbidden" { "2" } else { "0" });
        if mode == "allowed" {
            bundle["allowed_reverts"] = json!(["victim"]);
            s["scheduler"]["emissions"][0]["transaction"]["program"][0]["amount"] =
                json!(Amount::MAX.to_string());
        }
        let r = finish(&s);
        let p = view(&r);
        let actual = r
            .blocks
            .iter()
            .flat_map(|b| &b.receipts)
            .filter(|r| r.tx_id == "victim" && r.status == "Success")
            .count();
        assert_eq!(p.flow_events, (actual as u64).into());
        assert!(actual <= 1);
        n(&p.inflows, if actual == 1 { "20" } else { "0" });
        if mode == "success" {
            assert_eq!(actual, 1);
            n(&p.excess_pnl, "-5");
        }
        if mode == "allowed" {
            assert_eq!(actual, 0);
            assert!(
                r.blocks[0]
                    .receipts
                    .iter()
                    .any(|r| r.status == "ProtocolRevert")
            );
            n(&p.excess_pnl, "-5");
        }
    }
    let mut s = fixture("sc-scheduler-public");
    enable(&mut s, &["alice"]);
    label(&mut s, "victim");
    s["scheduler"]["reactive"] = json!({});
    s["scheduler"]["rounds"] = json!(3);
    s["accounts"]["bob"]["balances"]["X"] = json!("2");
    s["scheduler"]["emissions"][0]["transaction"]["program"] =
        json!([{"type":"Transfer","to":"alice","asset":"X","amount":"2"}]);
    s["scheduler"]["emissions"][0]["delivery"]["builder_delay"] = json!("14");
    s["valuation_policy"]["reference_feeds"]["X"]["records"][1]["available_at"] = json!("25");
    s["valuation_policy"]["reference_feeds"]["X"]["records"][1]["observed_at"] = json!("25");
    let r = finish(&s);
    assert_eq!(h::at(&r, 2).unwrap().portfolios[0].flow_events, 0.into());
    let p = view(&r);
    assert_eq!(p.flows[0].cursor.timestamp, 30.into());
    n(&p.inflows, "30");
}

#[test]
fn borrow_interest_loss_and_all_existing_claims_use_same_actual_mtm() {
    for name in [
        "sc-hodl-credit",
        "sc-mtm-clmm",
        "sc-mtm-book",
        "sc-stable",
        "sc-03-heterogeneous",
    ] {
        let mut s = fixture(name);
        if s.get("hodl_policy").is_none() {
            if s.get("valuation_policy").is_some() {
                s["hodl_policy"] = fixture("sc-hodl-capital")["hodl_policy"].clone();
                s["hodl_policy"]["portfolios"] = serde_json::to_value(
                    s["valuation_policy"]["portfolios"]
                        .as_object()
                        .unwrap()
                        .keys()
                        .collect::<Vec<_>>(),
                )
                .unwrap();
                s["model_versions"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!(h::MODEL));
            } else {
                enable(&mut s, &["alice"]);
            }
        }
        let r = finish(&s);
        for i in 0..=r.blocks.len() {
            let hb = h::at(&r, i).unwrap();
            let vb = v::at(&r, i).unwrap();
            for p in hb.portfolios {
                let actual = vb
                    .portfolios
                    .iter()
                    .find(|a| a.portfolio == p.portfolio)
                    .unwrap();
                assert_eq!(p.actual, actual.current.mark);
                assert_eq!(p.actual_pnl, actual.pnl);
                assert_eq!(p.inflows, actual.inflows);
                assert_eq!(p.outflows, actual.outflows);
                assert_eq!(p.flow_events, 0.into());
                assert!(matches!(p.ledger, Ledger::Ready { .. }));
            }
        }
        if name == "sc-hodl-credit" {
            let b = h::at(&r, r.blocks.len()).unwrap();
            let depositor = b
                .portfolios
                .iter()
                .find(|p| p.portfolio == "depositor")
                .unwrap();
            n(&depositor.hodl.mark, "100");
            n(&depositor.excess_pnl, "-20");
        }
    }
}

#[test]
fn derived_views_and_changed_policies_cannot_change_execution_rng_or_checkpoint() {
    let mut s = fixture("sc-agent-random");
    let control = finish(&s);
    enable(&mut s, &["alice"]);
    let mut r = Run::new(parse(&s)).unwrap();
    for i in 0..=r.scenario.rounds() {
        let checkpoint = r.checkpoint().unwrap();
        let report = h::at(&r, i).unwrap();
        assert_eq!(r.checkpoint().unwrap(), checkpoint);
        assert_eq!(
            h::at(&Run::restore(&checkpoint).unwrap(), i).unwrap(),
            report
        );
        assert!(h::at(&r, i + 1).is_err());
        r.step().unwrap();
    }
    assert_eq!(r.blocks, control.blocks);
    assert_eq!(r.state, control.state);
    assert_eq!(r.scheduler, control.scheduler);
    assert_eq!(
        serde_json::to_value(&r).unwrap()["rng"],
        serde_json::to_value(control).unwrap()["rng"]
    );
    s["hodl_policy"]["limits"]["assets_per_portfolio"] = json!(1);
    s["valuation_policy"]["reference_feeds"]["X"]["records"] = json!([]);
    let other = finish(&s);
    assert_ne!(r.run_id, other.run_id);
    assert_eq!(r.blocks, other.blocks);
    assert_eq!(r.scheduler, other.scheduler);
}

#[test]
fn native_exports_stream_selected_boundary_values_and_resume_byte_identically() {
    let mut r = Run::new(parse(&fixture("sc-hodl-infeasible"))).unwrap();
    r.step().unwrap();
    let mut restored = Run::restore(&r.checkpoint().unwrap()).unwrap();
    r.finish().unwrap();
    restored.finish().unwrap();
    let root = temp();
    std::fs::create_dir(&root).unwrap();
    r.export(&root.join("full")).unwrap();
    restored.export(&root.join("resumed")).unwrap();
    for item in std::fs::read_dir(root.join("full")).unwrap() {
        let name = item.unwrap().file_name();
        assert_eq!(
            std::fs::read(root.join("full").join(&name)).unwrap(),
            std::fs::read(root.join("resumed").join(&name)).unwrap(),
            "{name:?}"
        );
    }
    let lines = std::fs::read_to_string(root.join("full/hodl.jsonl")).unwrap();
    let lines: Vec<Value> = lines
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), r.blocks.len() + 1);
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(*line, serde_json::to_value(h::at(&r, i).unwrap()).unwrap());
    }
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("full/manifest.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["hodl_policy"],
        serde_json::to_value(h::provenance(&r).unwrap()).unwrap()
    );
    assert_eq!(
        manifest["hodl_policy"]["policy_sha256"],
        run::digest(&std::fs::read(root.join("full/hodl-policy.json")).unwrap())
    );
    let old = finish(&fixture("sc-01"));
    old.export(&root.join("old")).unwrap();
    assert!(!root.join("old/hodl.jsonl").exists());
    assert!(!root.join("old/hodl-policy.json").exists());
    assert!(
        serde_json::to_value(&old.scenario)
            .unwrap()
            .get("hodl_policy")
            .is_none()
    );
    assert!(h::at(&old, 0).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_policy_tag_limits_portfolios_and_unknown_fields_are_rejected() {
    let s = fixture("sc-hodl-capital");
    let mut bad = vec![];
    let mut b = s.clone();
    b.as_object_mut().unwrap().remove("hodl_policy");
    bad.push(b);
    let mut b = s.clone();
    b["model_versions"]
        .as_array_mut()
        .unwrap()
        .retain(|m| m != h::MODEL);
    bad.push(b);
    let mut b = s.clone();
    b.as_object_mut().unwrap().remove("valuation_policy");
    bad.push(b);
    for (key, cap) in [
        ("portfolios", 16),
        ("assets_per_portfolio", 64),
        ("flow_events_per_portfolio", 4096),
        ("rational_bits", 32768),
    ] {
        for n in [0, cap + 1] {
            let mut b = s.clone();
            b["hodl_policy"]["limits"][key] = json!(n);
            bad.push(b);
        }
    }
    let mut b = s.clone();
    b["hodl_policy"]["limits"]["rational_bits"] = json!(511);
    bad.push(b);
    for ids in [json!([]), json!(["owner", "owner"]), json!(["missing"])] {
        let mut b = s.clone();
        b["hodl_policy"]["portfolios"] = ids;
        bad.push(b);
    }
    let mut b = s.clone();
    b["hodl_policy"]["version"] = json!("future");
    bad.push(b);
    let mut b = s.clone();
    b["hodl_policy"]["prices"] = json!({});
    bad.push(b);
    for b in bad {
        assert!(
            Scenario::parse(&serde_json::to_vec(&b).unwrap()).is_err(),
            "{b}"
        );
    }
    let mut s = s;
    s["hodl_policy"]["limits"] = json!({"portfolios":1,"assets_per_portfolio":1,"flow_events_per_portfolio":1,"rational_bits":512});
    parse(&s);
}

#[test]
fn overlapping_views_apply_only_their_own_capital_boundary() {
    let mut s = fixture("sc-hodl-ordered");
    s["valuation_policy"]["portfolios"]["outside"] = json!({"accounts":["bob"]});
    s["valuation_policy"]["portfolios"]["combined"] = json!({"accounts":["alice","bob"]});
    s["hodl_policy"]["portfolios"] = json!(["owner", "outside", "combined"]);
    for id in ["z-in", "a-out"] {
        s["valuation_policy"]["capital_transfers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"portfolio":"outside","tx_id":id}));
    }
    let r = finish(&s);
    let b = h::at(&r, 1).unwrap();
    let outside = b
        .portfolios
        .iter()
        .find(|p| p.portfolio == "outside")
        .unwrap();
    assert_eq!(outside.flows[0].flow.direction, "Outflow");
    n(&outside.excess_pnl, "0");
    let combined = b
        .portfolios
        .iter()
        .find(|p| p.portfolio == "combined")
        .unwrap();
    assert!(combined.flows.is_empty());
    assert_eq!(combined.flow_events, 0.into());
    assert_eq!(combined.overlaps, ["outside", "owner"]);
    n(&combined.excess_pnl, "0");
}

#[test]
fn hodl_remains_fixed_input_of_paired_experiments_without_new_selectors() {
    let mut plan: Value = serde_json::from_slice(
        &std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../scenarios/experiments/builder.json"),
        )
        .unwrap(),
    )
    .unwrap();
    plan["cases"].as_array_mut().unwrap().truncate(1);
    for s in plan["cases"][0]["variants"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        s["hodl_policy"] = fixture("sc-hodl-capital")["hodl_policy"].clone();
        s["model_versions"]
            .as_array_mut()
            .unwrap()
            .push(json!(h::MODEL));
    }
    let p = experiment::Plan::parse(&serde_json::to_vec(&plan).unwrap()).unwrap();
    let report = experiment::execute(&p, |run, _| {
        assert!(h::at(run, run.blocks.len()).is_ok());
        Ok(())
    })
    .unwrap();
    report.validate().unwrap();
    let mut changed = plan.clone();
    changed["cases"][0]["variants"]["alternative"]["hodl_policy"]["limits"]["assets_per_portfolio"] =
        json!(1);
    assert!(
        experiment::Plan::parse(&serde_json::to_vec(&changed).unwrap())
            .unwrap_err()
            .contains("undeclared experiment difference")
    );
    plan["metrics"]["hodl"] = json!({"type":"HodlPnl","portfolio":"owner"});
    assert!(experiment::Plan::parse(&serde_json::to_vec(&plan).unwrap()).is_err());
}

#[test]
fn model_error_keeps_hodl_at_the_last_committed_boundary() {
    let mut s = simple();
    s["chain"]["interval"] = json!(Amount::MAX.to_string());
    s["blocks"] = json!([[], []]);
    let mut r = Run::new(parse(&s)).unwrap();
    r.step().unwrap();
    let before = h::at(&r, 1).unwrap();
    assert!(r.step().unwrap_err().contains("overflow"));
    assert_eq!(h::at(&r, 1).unwrap(), before);
    assert!(h::at(&r, 2).is_err());
    assert_eq!(
        h::at(&Run::restore(&r.checkpoint().unwrap()).unwrap(), 1).unwrap(),
        before
    );
}
