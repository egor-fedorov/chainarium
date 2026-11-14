use super::*;
use crate::Call;
use num_bigint::{BigInt, BigUint};
use serde_json::{Value as Json, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
fn fixture(kind: &str) -> Plan {
    Plan::parse(
        &std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../scenarios/experiments/{kind}.json")),
        )
        .unwrap(),
    )
    .unwrap()
}
fn one(kind: &str) -> Plan {
    let mut p = fixture(kind);
    p.cases.truncate(1);
    p
}
fn execute_report(p: &Plan) -> Report {
    execute(p, |_, _| Ok(())).unwrap()
}
fn raw(p: &Plan) -> Json {
    serde_json::to_value(p).unwrap()
}
fn parse(v: &Json) -> Result<Plan, String> {
    Plan::parse(&serde_json::to_vec(v).unwrap())
}
fn exact(n: i64, d: u64) -> Exact {
    Exact::ratio(n.into(), d.into(), 32768).unwrap()
}
fn check_statistics(actual: &Statistics, expected: &Json) {
    let actual = serde_json::to_value(actual).unwrap();
    for k in [
        "minimum",
        "maximum",
        "mean",
        "median",
        "p05",
        "p95",
        "sample_variance",
    ] {
        if expected["status"] == "Unavailable" {
            assert_eq!(actual[k]["reasons"], json!(["NoPricedObservations"]));
        } else if expected[k].is_null() {
            assert_eq!(actual[k]["reasons"], json!(["InsufficientSample"]));
        } else {
            assert_eq!(actual[k]["value"], expected[k], "{k}");
        }
    }
    if expected["status"] != "Unavailable" {
        for k in ["negative", "zero", "positive"] {
            assert_eq!(actual[k], expected[k]);
        }
    }
}
fn reference() -> Json {
    serde_json::from_slice(include_bytes!(
        "../../../../docs/reviews/experiment-series-reference.json"
    ))
    .unwrap()
}
fn temp(label: &str) -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    std::env::temp_dir().join(format!(
        "chainarium-experiment-{}-{}-{label}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn builder_native_runs_and_statistics_match_independent_reference() {
    let p = fixture("builder");
    let refs = reference();
    let refs = &refs["cases"]["builder_four_case_sensitivity"];
    let r = execute(&p, |run, row| {
        let mut standalone = Run::new(run.scenario.clone()).unwrap();
        standalone.finish().unwrap();
        assert_eq!(&standalone, run);
        assert_eq!(
            Run::restore(&run.checkpoint().unwrap()).unwrap(),
            standalone
        );
        let expected = &refs["rows"][row.seed.to_string().parse::<usize>().unwrap()];
        assert_eq!(
            serde_json::to_value(row.metrics["pnl"].exact()).unwrap(),
            expected[format!("{}_pnl", row.variant)]
        );
        assert_eq!(
            row.metrics["fees"].exact().unwrap().floor().to_string(),
            expected[format!("{}_fee", row.variant)]
        );
        assert_eq!(row.outcomes.included["Success"], 1.into());
        assert_eq!(
            row.outcomes.included_failure_fraction,
            Value::available(exact(0, 1))
        );
        assert!(row.outcomes.inclusion_rejection_attempts > Amount::ZERO);
        Ok(())
    })
    .unwrap();
    check_statistics(
        &r.analysis.paired["alternative"]["pnl"].statistics,
        &refs["paired"]["statistics"],
    );
    assert_eq!(r.analysis.paired["alternative"]["pnl"].eligible_n, 4.into());
    assert_eq!(Report::parse(&serde_json::to_vec(&r).unwrap()).unwrap(), r);
}
#[test]
fn twenty_seeds_match_scalar_rng_reference_without_runner_draws() {
    let p = fixture("policy");
    let refs = reference();
    let refs = &refs["cases"]["random_policy_twenty_seeds"];
    let report = execute(&p, |run, row| {
        let mut native = Run::new(run.scenario.clone()).unwrap();
        native.finish().unwrap();
        assert_eq!(run, &native);
        let j = serde_json::to_value(run).unwrap();
        assert_eq!(j["rng"]["positions"]["agent/trader"], "24");
        let e = &refs["rows"][row.seed.to_string().parse::<usize>().unwrap()][&row.variant];
        assert_eq!(
            serde_json::to_value(row.metrics["pnl"].exact()).unwrap(),
            e["pnl"]
        );
        assert_eq!(
            row.metrics["fees"].exact().unwrap().floor().to_string(),
            e["paid_native_fees"]
        );
        assert_eq!(
            row.outcomes.included["Success"].to_string(),
            e["included_success"]
        );
        Ok(())
    })
    .unwrap();
    report.validate().unwrap();
    check_statistics(
        &report.analysis.paired["alternative"]["pnl"].statistics,
        &refs["paired"]["statistics"],
    );
    assert_eq!(
        report.analysis.variants["baseline"]["pnl"].statistics.zero,
        20.into()
    );
    assert_eq!(
        report.analysis.variants["alternative"]["pnl"]
            .statistics
            .negative,
        20.into()
    );
}
#[test]
fn seed_order_is_numeric_and_input_order_does_not_change_report() {
    let p = fixture("policy");
    let a = execute_report(&p);
    let mut p = p;
    p.cases.reverse();
    let b = execute_report(&p);
    assert_eq!(a, b);
    assert_eq!(a.rows[4].seed, 2.into());
    assert_eq!(a.rows[20].seed, 10.into());
    assert_eq!(a.rows[0].variant, "alternative");
}
#[test]
fn fractions_empty_singleton_and_wide_signed_statistics_match_reference() {
    let r = reference();
    let c = &r["cases"];
    check_statistics(
        &statistics(&[-1, 1, 3, 5].map(|n| exact(n, 2)), 32768),
        &c["fractional_even_median"],
    );
    check_statistics(&statistics(&[exact(-1, 2)], 32768), &c["one_observation"]);
    check_statistics(&statistics(&[], 32768), &c["no_priced_observations"]);
    let max = BigInt::from(Amount::MAX.big());
    check_statistics(
        &statistics(
            &[
                Exact::ratio(-&max * &max, 3u64.into(), 32768).unwrap(),
                exact(1, 3),
            ],
            32768,
        ),
        &c["wide_negative_fraction"],
    );
}
#[test]
fn arithmetic_cap_preserves_observations_and_unaffected_statistics() {
    let wide = Exact::ratio(BigInt::from(1u8) << 300, BigUint::from(1u8), 32768).unwrap();
    let values = [wide.neg(), wide];
    let before = values.clone();
    let s = statistics(&values, 512);
    assert_eq!(s.mean, Value::available(exact(0, 1)));
    assert_eq!(s.sample_variance, Value::unavailable("ArithmeticLimit"));
    assert_eq!(values, before);
    let huge = Exact::ratio(BigInt::from(1u8) << 511, 1u8.into(), 32768).unwrap();
    let s = statistics(&[huge.clone(), huge.clone()], 512);
    assert_eq!(s.mean, Value::unavailable("ArithmeticLimit"));
    assert_eq!(s.minimum, Value::available(huge));
}
#[test]
fn exact_wire_rejects_noncanonical_or_lossy_imports() {
    let good = json!({"numerator":"-1","denominator":"2","floor_atoms":"-1"});
    assert_eq!(
        serde_json::from_value::<Exact>(good.clone()).unwrap(),
        exact(-1, 2)
    );
    for (field, value) in [
        ("numerator", json!(-1)),
        ("numerator", json!("-01")),
        ("numerator", json!("+1")),
        ("denominator", json!("0")),
        ("denominator", json!("02")),
        ("floor_atoms", json!("0")),
        ("extra", json!(true)),
    ] {
        let mut bad = good.clone();
        bad[field] = value;
        assert!(serde_json::from_value::<Exact>(bad).is_err());
    }
    assert!(
        serde_json::from_value::<Exact>(
            json!({"numerator":"2","denominator":"4","floor_atoms":"0"})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<Exact>(
            json!({"numerator":"0","denominator":"1","floor_atoms":"-0"})
        )
        .is_err()
    );
}
#[test]
fn paired_missing_values_join_by_seed_and_keep_side_reasons() {
    let mut r = execute_report(&fixture("builder"));
    let baseline = [Some(0), None, Some(4), Some(10)];
    let alternative = [Some(5), Some(100), None, Some(7)];
    for row in &mut r.rows {
        let i = row.seed.to_string().parse::<usize>().unwrap();
        let v = if row.variant == "baseline" {
            baseline[i]
        } else {
            alternative[i]
        };
        row.metrics.insert(
            "pnl".into(),
            v.map(|n| Value::available(exact(n, 1)))
                .unwrap_or_else(|| Value::unavailable("MissingReference")),
        );
    }
    let a = analyze(&r.rows, &r.metrics, &r.baseline_variant, 32768).unwrap();
    let c = &a.paired["alternative"]["pnl"];
    assert_eq!(c.eligible_seeds, vec![0.into(), 3.into()]);
    assert_eq!(c.statistics.mean, Value::available(exact(1, 1)));
    assert_eq!(c.excluded[0].seed, 1.into());
    assert!(c.excluded[0].reasons.contains_key("baseline"));
    assert!(c.excluded[1].reasons.contains_key("variant"));
    check_statistics(
        &c.statistics,
        &reference()["cases"]["paired_missing_is_not_zero"]["statistics"],
    );
    let difference = a.variants["alternative"]["pnl"]
        .statistics
        .mean
        .exact()
        .unwrap()
        .sub(
            a.variants["baseline"]["pnl"]
                .statistics
                .mean
                .exact()
                .unwrap(),
            32768,
        )
        .unwrap();
    assert_eq!(difference, exact(98, 3));
}
#[test]
fn unpriced_pnl_does_not_remove_fees_or_runs() {
    let mut p = one("policy");
    for s in p.cases[0].variants.values_mut() {
        s.valuation_policy.as_mut().unwrap().reference_feeds.clear();
    }
    let r = execute_report(&p);
    r.validate().unwrap();
    assert_eq!(r.rows.len(), 2);
    assert_eq!(r.analysis.paired["alternative"]["pnl"].eligible_n, 0.into());
    assert_eq!(
        r.analysis.paired["alternative"]["fees"].eligible_n,
        1.into()
    );
    assert!(
        r.rows
            .iter()
            .all(|r| matches!(r.status, RunStatus::Completed))
    );
}
#[test]
fn forbidden_paired_changes_fail_before_any_execution() {
    let p = one("policy");
    for (pointer, value) in [
        ("/accounts/alice/balances/Q", json!("5001")),
        ("/chain/interval", json!("2")),
        ("/scheduler/builder/type", json!("FeePriority")),
        ("/scheduler/agents/trader/observation_delay", json!("1")),
        (
            "/valuation_policy/reference_feeds/X/records/0/price/numerator",
            json!("2"),
        ),
        (
            "/valuation_policy/portfolios/owner/accounts/0",
            json!("bob"),
        ),
    ] {
        let mut bad = raw(&p);
        *bad["cases"][0]["variants"]["alternative"]
            .pointer_mut(pointer)
            .unwrap() = value;
        let candidate: Plan = serde_json::from_value(bad).unwrap();
        let mut called = false;
        assert!(
            execute(&candidate, |_, _| {
                called = true;
                Ok(())
            })
            .is_err(),
            "{pointer}"
        );
        assert!(!called);
    }
}
#[test]
fn fixed_treatment_and_environment_across_cases_are_enforced() {
    let p = fixture("policy");
    for (pointer, value) in [
        ("/baseline_policies/trader/kind/probability_ppm", json!(123)),
        ("/accounts/alice/balances/Q", json!("5001")),
        (
            "/valuation_policy/reference_feeds/X/source_id",
            json!("other"),
        ),
        ("/scheduler/rounds", json!(5)),
    ] {
        let mut bad = raw(&p);
        for variant in ["baseline", "alternative"] {
            *bad["cases"][1]["variants"][variant]
                .pointer_mut(pointer)
                .unwrap() = value.clone();
        }
        assert!(parse(&bad).is_err(), "{pointer}");
    }
}
#[test]
fn materialized_external_records_can_vary_across_cases_only() {
    let mut p = raw(&fixture("builder"));
    for variant in ["baseline", "alternative"] {
        p["cases"][1]["variants"][variant]["valuation_policy"]["reference_feeds"]["X"]["records"]
            [0]["price"]["numerator"] = json!("2");
    }
    let p = parse(&p).unwrap();
    let infos = p.validate().unwrap();
    assert_ne!(
        infos[0].external_inputs_sha256,
        infos[1].external_inputs_sha256
    );
    assert_ne!(infos[0].common_input_sha256, infos[1].common_input_sha256);
}
#[test]
fn policy_account_authority_model_tags_and_shared_caps_cannot_be_masked() {
    let mut p = one("policy");
    for s in p.cases[0].variants.values_mut() {
        s.agents
            .get_mut("trader")
            .unwrap()
            .controlled_accounts
            .insert("bob".into());
    }
    let mut bad = raw(&p);
    bad["cases"][0]["variants"]["alternative"]["baseline_policies"]["trader"]["account"] =
        json!("bob");
    assert!(parse(&bad).unwrap_err().contains("policy account"));
    let mut bad = raw(&p);
    bad["cases"][0]["variants"]["alternative"]["model_versions"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    assert!(parse(&bad).is_err());
    let mut bad = raw(&p);
    bad["cases"][0]["variants"]["alternative"]["scheduler"]["limits"]["events_per_round"] =
        json!(4000);
    assert!(parse(&bad).is_err());
    let mut bad = raw(&p);
    bad["cases"][0]["variants"]["alternative"]["agents"]["trader"]["controlled_accounts"] =
        json!(["alice"]);
    assert!(parse(&bad).is_err());
}
#[test]
fn builder_mode_forbids_autonomous_candidates() {
    let mut p = one("policy");
    p.comparison = Comparison::BuilderOrder;
    assert!(p.validate().unwrap_err().contains("autonomous"));
}
#[test]
fn pointer_escaping_cannot_select_a_different_policy() {
    let mut p = raw(&one("policy"));
    let id = "a/~trader";
    p["comparison"]["targets"][0]["agent"] = json!(id);
    for v in ["baseline", "alternative"] {
        let s = &mut p["cases"][0]["variants"][v];
        for k in ["agents", "baseline_policies"] {
            let entry = s[k].as_object_mut().unwrap().remove("trader").unwrap();
            s[k][id] = entry;
        }
        let entry = s["scheduler"]["agents"]
            .as_object_mut()
            .unwrap()
            .remove("trader")
            .unwrap();
        s["scheduler"]["agents"][id] = entry;
    }
    let p = parse(&p).unwrap();
    let info = p.validate().unwrap();
    assert!(
        info[0].variants["alternative"]
            .differences
            .iter()
            .any(|d| d.pointer == "/baseline_policies/a~1~0trader/kind/probability_ppm")
    );
}
#[test]
fn headers_shapes_selectors_and_caps_reject_invalid_plans() {
    let p = one("policy");
    for (pointer, value) in [
        ("/schema_version", json!("future")),
        ("/primary_metric", json!("fees")),
        ("/metrics/pnl/portfolio", json!("missing")),
        ("/metrics/fees/accounts", json!(["alice", "alice"])),
        ("/limits/cases", json!(0)),
        ("/limits/rational_bits", json!(511)),
        ("/limits/variants", json!(9)),
        ("/limits/runs", json!(1)),
        ("/limits/total_blocks", json!(11)),
        ("/limits/blocks_per_run", json!(5)),
        ("/cases/0/seed", json!("1")),
        ("/comparison/targets", json!([])),
    ] {
        let mut bad = raw(&p);
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(parse(&bad).is_err(), "{pointer}");
    }
    let mut p = p;
    p.cases.push(p.cases[0].clone());
    assert!(p.validate().unwrap_err().contains("duplicate"));
    let mut p = fixture("builder");
    p.cases[1].variants.remove("alternative");
    assert!(p.validate().is_err());
}
#[test]
fn full_diff_and_hashes_keep_names_as_labels() {
    let mut p = one("builder");
    let before = p.validate().unwrap();
    p.cases[0].variants.get_mut("alternative").unwrap().name = "different label".into();
    let after = p.validate().unwrap();
    assert_eq!(before[0].common_input_sha256, after[0].common_input_sha256);
    assert_ne!(
        before[0].variants["alternative"].scenario_sha256,
        after[0].variants["alternative"].scenario_sha256
    );
    assert!(
        after[0].variants["alternative"]
            .differences
            .iter()
            .any(|d| d.pointer == "/name" && d.new == Some(json!("different label")))
    );
}
#[test]
fn failed_paid_transfer_preserves_fee_and_no_capital_flow() {
    let mut p = one("builder");
    for s in p.cases[0].variants.values_mut() {
        let c = s.scheduler.as_mut().unwrap();
        c.emissions.truncate(1);
        let tx = &mut c.emissions[0].transaction;
        tx.program = vec![Call::Transfer {
            to: "bob".into(),
            asset: "Q".into(),
            amount: 9000.into(),
        }];
    }
    let r = execute_report(&p);
    for row in &r.rows {
        assert_eq!(row.metrics["pnl"], Value::available(exact(-115, 1)));
        assert_eq!(row.metrics["fees"], Value::available(exact(115, 1)));
        assert_eq!(
            row.outcomes.included_failure_fraction,
            Value::available(exact(1, 1))
        );
        assert_eq!(row.outcomes.included["ProtocolRevert"], 1.into());
    }
}
#[test]
fn model_error_retains_replayable_prefix_and_continues_planned_runs() {
    let mut p = one("builder");
    p.comparison = Comparison::AgentPolicy {
        targets: vec![Target {
            family: Family::Arbitrage,
            agent: "trader".into(),
        }],
    };
    for s in p.cases[0].variants.values_mut() {
        let mut tx = s.scheduler.as_ref().unwrap().emissions[0]
            .transaction
            .clone();
        tx.deadline = Amount::MAX;
        s.scheduler = None;
        s.model_versions
            .retain(|v| !crate::scheduler::MODELS.contains(&v.as_str()));
        s.model_versions.push("scheduler-m1-v1".into());
        s.blocks = vec![vec![tx], vec![]];
        s.chain.interval = Amount::MAX;
        s.valuation_policy.as_mut().unwrap().max_age_seconds = Amount::MAX;
        s.policies.insert(
            "trader".into(),
            crate::policy::ArbitragePolicy {
                account: "alice".into(),
                asset: "Q".into(),
                input: 1.into(),
                min_profit: 1_000_000.into(),
                resource_limit: 200.into(),
                resource_price: 1.into(),
                route_limits: None,
            },
        );
    }
    let mut checkpoints = vec![];
    let r = execute(&p, |run, _| {
        checkpoints.push(run.checkpoint()?);
        Ok(())
    })
    .unwrap();
    assert_eq!(r.outcome, "CompletedWithModelErrors");
    assert_eq!(r.rows.len(), 2);
    r.validate().unwrap();
    for (i, row) in r.rows.iter().enumerate() {
        assert_eq!(row.committed_blocks, 1.into());
        assert_eq!(
            row.prefix_metrics.as_ref().unwrap()["pnl"],
            Value::available(exact(-125, 1))
        );
        assert_eq!(row.metrics["pnl"], Value::unavailable("ModelError"));
        r.restore_row(i, &checkpoints[i]).unwrap();
    }
    assert_eq!(r.analysis.paired["alternative"]["pnl"].eligible_n, 0.into());
    for metric in ["success", "rejections"] {
        let mut forged = r.clone();
        forged.rows[0]
            .prefix_metrics
            .as_mut()
            .unwrap()
            .insert(metric.into(), Value::available(exact(999, 1)));
        assert!(forged.validate().unwrap_err().contains("count metric"));
    }
}
#[test]
fn reports_reject_tampered_statistics_plan_and_paths() {
    let r = execute_report(&one("builder"));
    let good = serde_json::to_value(&r).unwrap();
    for (pointer, value) in [
        ("/rows/0/path", json!("../../elsewhere")),
        ("/analysis/paired/alternative/pnl/eligible_n", json!("0")),
        (
            "/analysis/paired/alternative/pnl/statistics/mean/value/numerator",
            json!("0"),
        ),
        ("/cases/0/common_input_sha256", json!("0".repeat(64))),
        ("/plan/hypothesis", json!("changed")),
        ("/outcome", json!("Incomplete")),
        ("/rows/0/metrics/pnl/value/floor_atoms", json!("999")),
        (
            "/rows/0/outcomes/included_failure_fraction/value/numerator",
            json!("1"),
        ),
    ] {
        let mut bad = good.clone();
        *bad.pointer_mut(pointer).unwrap() = value;
        assert!(
            Report::parse(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "{pointer}"
        );
    }
}
#[test]
fn selected_checkpoint_must_match_row_and_verified_metrics() {
    let mut checkpoints = vec![];
    let r = execute(&one("builder"), |run, _| {
        checkpoints.push(run.checkpoint()?);
        Ok(())
    })
    .unwrap();
    r.restore_row(0, &checkpoints[0]).unwrap();
    assert!(r.restore_row(1, &checkpoints[0]).is_err());
    assert!(r.restore_row(99, &checkpoints[0]).is_err());
    let mut forged = r.clone();
    forged.rows[0]
        .metrics
        .insert("pnl".into(), Value::available(exact(999, 1)));
    forged.analysis = analyze(
        &forged.rows,
        &forged.metrics,
        &forged.baseline_variant,
        32768,
    )
    .unwrap();
    forged.validate().unwrap();
    assert!(
        forged
            .restore_row(0, &checkpoints[0])
            .unwrap_err()
            .contains("results differ")
    );
    let mut fork = Run::restore(&checkpoints[0]).unwrap();
    fork.run_id = r.rows[1].run_id.clone();
    assert!(r.restore_row(1, &fork.checkpoint().unwrap()).is_err());
}
#[test]
fn export_is_new_directory_only_and_all_artifacts_precede_completion() {
    let path = temp("complete");
    let p = one("builder");
    let r = export(&p, &path).unwrap();
    let bytes = std::fs::read(path.join("manifest.json")).unwrap();
    assert_eq!(Report::parse(&bytes).unwrap(), r);
    assert_eq!(
        std::fs::read_to_string(path.join("rows.jsonl"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    for (i, row) in r.rows.iter().enumerate() {
        r.restore_row(
            i,
            &std::fs::read(path.join(&row.path).join("checkpoint.json")).unwrap(),
        )
        .unwrap();
    }
    assert!(
        path.join("plan.json").is_file()
            && path.join("pairs.jsonl").is_file()
            && path.join("summary.json").is_file()
            && path.join("report.md").is_file()
    );
    assert!(export(&p, &path).is_err());
    assert_eq!(std::fs::read(path.join("manifest.json")).unwrap(), bytes);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn io_failure_leaves_incomplete_marker_and_committed_prefix() {
    let path = temp("incomplete");
    let p = one("builder");
    let mut calls = 0;
    let e = super::export::export_using(&p, &path, |run, dir| {
        calls += 1;
        if calls == 2 {
            std::fs::create_dir(dir).unwrap();
        }
        run.export(dir)
    })
    .unwrap_err();
    assert!(e.contains("create new export"));
    let bytes = std::fs::read(path.join("manifest.json")).unwrap();
    assert_eq!(
        serde_json::from_slice::<Json>(&bytes).unwrap()["outcome"],
        "Incomplete"
    );
    assert!(Report::parse(&bytes).is_err());
    assert!(path.join("runs/0000/checkpoint.json").is_file());
    assert!(!path.join("summary.json").exists());
    assert!(export(&p, &path).is_err());
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn invalid_late_case_creates_no_output_directory() {
    let mut p = fixture("builder");
    p.cases
        .last_mut()
        .unwrap()
        .variants
        .get_mut("alternative")
        .unwrap()
        .seed = 999.into();
    let path = temp("invalid");
    assert!(export(&p, &path).is_err());
    assert!(!path.exists());
}
#[test]
fn arbitrary_variant_ids_never_become_file_paths() {
    let mut p = one("builder");
    let scenario = p.cases[0].variants.remove("alternative").unwrap();
    p.cases[0]
        .variants
        .insert("../../<script>".into(), scenario);
    let r = execute_report(&p);
    assert_eq!(r.rows[0].path, "runs/0000");
    r.validate().unwrap();
}
#[test]
fn structural_diff_distinguishes_missing_null_and_escaped_keys() {
    let d = differences(&json!({"a/b":null,"a~b":[1,2]}), &json!({"a~b":[1,3]}));
    assert_eq!(d[0].pointer, "/a~1b");
    assert_eq!(d[0].old, Some(Json::Null));
    assert_eq!(d[0].new, None);
    assert_eq!(d[1].pointer, "/a~0b/1");
}

#[test]
fn all_declared_policy_families_resolve_only_the_selected_object() {
    for (file, family, agent) in [
        ("sc-03-route", Family::Arbitrage, "trader"),
        ("sc-lp-policies", Family::LP, "alice"),
        ("sc-mm-oracle", Family::MM, "alice"),
        ("sc-agent-random", Family::Baseline, "trader"),
        ("sc-scheduler-public", Family::Reactive, "trader"),
        ("sc-backrun-public", Family::Backrun, "trader"),
        ("sc-liquidator-own", Family::Liquidator, "carol"),
        ("sc-flash-arb-success", Family::FlashArbitrage, "trader"),
    ] {
        let mut p = one("policy");
        let mut s = crate::Scenario::parse(
            &std::fs::read(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join(format!("../../scenarios/demos/{file}.json")),
            )
            .unwrap(),
        )
        .unwrap();
        s.seed = 0.into();
        s.chain.fee_asset = "Q".into();
        s.model_versions.push(valuation::MODEL.into());
        let mut v = p.cases[0].variants["baseline"]
            .valuation_policy
            .clone()
            .unwrap();
        v.portfolios.get_mut("owner").unwrap().accounts = vec!["alice".into()];
        s.valuation_policy = Some(v);
        p.comparison = Comparison::AgentPolicy {
            targets: vec![Target {
                family,
                agent: agent.into(),
            }],
        };
        p.cases[0].variants =
            BTreeMap::from([("baseline".into(), s.clone()), ("alternative".into(), s)]);
        let path = p.comparison.paths()[0].clone();
        let mut j = raw(&p);
        let target = j["cases"][0]["variants"]["alternative"]
            .pointer_mut(&path)
            .unwrap();
        if let Some(price) = target.get_mut("resource_price") {
            *price = json!("0");
        } else {
            target["resource_limit"] = json!("900");
        }
        parse(&j).unwrap_or_else(|e| panic!("{file}: {e}"));
        j["cases"][0]["variants"]["alternative"]
            .pointer_mut(&path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown_field".into(), json!(true));
        assert!(parse(&j).is_err());
    }
}
#[test]
fn included_denominator_ignores_repeated_rejections_and_trace_actions() {
    execute(&one("builder"), |run, _| {
        // Isolate aggregation from execution: two successful receipts, one paid
        // revert and one exhausted receipt, plus seven repeated rejection attempts.
        let mut run = run.clone();
        let b = &mut run.blocks[0];
        let template = b.receipts[0].clone();
        b.receipts = ["Success", "Success", "ProtocolRevert", "ResourceExhausted"]
            .iter()
            .map(|s| {
                let mut r = template.clone();
                r.status = (*s).into();
                r
            })
            .collect();
        let rejected = b.rejected[0].clone();
        b.rejected = vec![rejected; 7];
        run.blocks.truncate(1);
        let o = outcomes(&run, 32768).unwrap();
        assert_eq!(o.included_failure_fraction, Value::available(exact(1, 2)));
        assert_eq!(o.inclusion_rejection_attempts, 7.into());
        Ok(())
    })
    .unwrap();
}
#[test]
fn diff_wire_preserves_present_null_separately_from_absence() {
    let diffs = differences(&json!({"a":null}), &json!({"b":null}));
    let encoded = serde_json::to_vec(&diffs).unwrap();
    assert_eq!(
        serde_json::from_slice::<Vec<Difference>>(&encoded).unwrap(),
        diffs
    );
    let j: Json = serde_json::from_slice(&encoded).unwrap();
    assert!(j[0].get("old").is_some());
    assert!(j[0].get("new").is_none());
}

#[test]
fn imported_horizon_must_equal_the_embedded_scenario() {
    let mut r = execute_report(&one("builder"));
    for row in &mut r.rows {
        row.horizon = Amount::ZERO;
        row.committed_blocks = Amount::ZERO;
    }
    assert!(
        r.validate().is_err(),
        "Completed zero-block report cannot describe a three-round Scenario"
    );
}
#[test]
fn imported_count_selectors_must_agree_with_outcomes() {
    let r = execute_report(&one("builder"));
    let mut included = r.clone();
    included.rows[0]
        .outcomes
        .included
        .insert("Success".into(), 999.into());
    assert!(
        included.validate().is_err(),
        "Success outcome and IncludedCount must agree"
    );
    let mut rejected = r;
    rejected.rows[0].outcomes.inclusion_rejection_attempts = 999.into();
    assert!(
        rejected.validate().is_err(),
        "rejection outcome and selector must agree"
    );
}
