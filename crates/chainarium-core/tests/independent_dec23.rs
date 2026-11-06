//! Independent DEC-23 review of 07c5018..99c26d1.
//! Copy to crates/chainarium-core/tests/independent_dec23.rs to run.
//! Integer oracle uses u128 CPMM arithmetic and native-cost-v1 constants;
//! it does not call engine quote/execution helpers.
use chainarium_core::{scheduler::backrun as b, *};
use serde_json::{Value, json};

fn fixture(name: &str) -> Value {
    serde_json::from_slice(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../scenarios/demos/sc-{name}.json")),
        )
        .unwrap(),
    )
    .unwrap()
}
fn scenario(v: Value) -> Scenario {
    Scenario::parse(&serde_json::to_vec(&v).unwrap()).unwrap()
}
fn finish(v: Value) -> Run {
    let mut r = Run::new(scenario(v)).unwrap();
    r.finish().unwrap();
    r
}
fn search(r: &Run, block: usize) -> &b::Report {
    r.blocks[block]
        .scheduler
        .as_ref()
        .unwrap()
        .search
        .as_ref()
        .unwrap()
}
fn num(n: u64) -> Value {
    Value::String(n.to_string())
}
fn normalized(report: &b::Report) -> Value {
    let mut v = serde_json::to_value(report).unwrap();
    for d in v["decisions"].as_array_mut().unwrap() {
        d["bundle"] = Value::Null;
        for c in d["candidates"].as_array_mut().unwrap() {
            for r in c["receipts"].as_array_mut().unwrap() {
                r["tx_id"] = Value::Null;
            }
        }
    }
    v
}
fn no_recipients() -> Value {
    json!({"type":"Private","recipients":[]})
}
fn coin(seed: &mut u64, max: u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*seed >> 20) % max
}
fn swap(pool: &mut [u64; 2], input_asset: usize, amount: u64, fee: u64) -> Option<u64> {
    let output_asset = 1 - input_asset;
    let eff = amount as u128 * (10000 - fee) as u128;
    let out = (eff * pool[output_asset] as u128 / (pool[input_asset] as u128 * 10000 + eff)) as u64;
    if out == 0 {
        return None;
    }
    pool[input_asset] += amount;
    pool[output_asset] -= out;
    Some(out)
}

#[test]
fn independent_integer_oracle_768_capital_fee_reserve_and_route_cases() {
    let mut seed = 0xdec2307u64;
    let mut profitable = 0;
    for case in 0..768 {
        let mut v = fixture("backrun-public");
        v["scheduler"]["rounds"] = json!(1);
        let mut pools = [[0; 2]; 2];
        for (i, account) in ["pool", "pool-b"].iter().enumerate() {
            for (j, asset) in ["X", "Q"].iter().enumerate() {
                pools[i][j] = 500 + coin(&mut seed, 4500);
                v["accounts"][account]["balances"][asset] = num(pools[i][j]);
            }
        }
        let fees = [coin(&mut seed, 251), coin(&mut seed, 251)];
        v["protocols"]["cpmm"]["fee_bps"] = json!(fees[0]);
        v["protocols"]["second"]["fee_bps"] = json!(fees[1]);
        let vin = 50 + coin(&mut seed, 450);
        let asset = coin(&mut seed, 2) as usize;
        v["accounts"]["bob"]["balances"]["X"] = num(10000);
        v["accounts"]["bob"]["balances"]["Q"] = num(10000);
        v["scheduler"]["emissions"][0]["transaction"]["program"][0]["asset_in"] =
            json!(["X", "Q"][asset]);
        v["scheduler"]["emissions"][0]["transaction"]["program"][0]["amount"] = num(vin);
        assert!(swap(&mut pools[0], asset, vin, fees[0]).is_some());
        let capital = 100 + coin(&mut seed, 10000);
        v["accounts"]["alice"]["balances"]["Q"] = num(capital);
        let response_price = coin(&mut seed, 4);
        let payment_price = coin(&mut seed, 4);
        let response_limit = [159, 160, 200, 500][coin(&mut seed, 4) as usize];
        let payment_limit = [100, 114, 115, 200, 1000][coin(&mut seed, 5) as usize];
        let block_limit = [619, 620, 1000, 10000][coin(&mut seed, 4) as usize].max(payment_limit);
        let bid = 1 + coin(&mut seed, 20);
        let minimum = 1 + coin(&mut seed, 20);
        v["chain"]["block_limit"] = num(block_limit);
        let p = &mut v["scheduler"]["backrun"]["policies"]["trader"];
        p["inputs"] = json!(["10", "25", "50", "100", "200", "300", "400", "500"]);
        p["resource_limit"] = num(response_limit);
        p["resource_price"] = num(response_price);
        p["payment_limit"] = num(payment_limit);
        p["payment_price"] = num(payment_price);
        p["bid"] = num(bid);
        p["minimum_net_profit"] = num(minimum);
        let r = finish(v);
        let d = &search(&r, 0).decisions[0];
        assert_eq!(d.candidates.len(), 16, "case {case}");
        let mut expected_best: Option<(u64, usize)> = None;
        for (i, c) in d.candidates.iter().enumerate() {
            let input = c.input.to_string().parse::<u64>().unwrap();
            let first = if c.hops[0].venue() == "cpmm" { 0 } else { 1 };
            let mut preview = pools;
            let output = swap(&mut preview[first], 1, input, fees[first])
                .and_then(|x| swap(&mut preview[1 - first], 0, x, fees[1 - first]));
            let expected = output.and_then(|out| {
                let net = out as i64
                    - input as i64
                    - (160 * response_price + 115 * payment_price + bid) as i64;
                let after_response =
                    capital as i64 + out as i64 - input as i64 - (160 * response_price) as i64;
                (net >= minimum as i64
                    && response_limit >= 160
                    && payment_limit >= 115
                    && response_limit <= block_limit - 120
                    && payment_limit <= block_limit - 280
                    && capital >= response_limit * response_price + input
                    && after_response >= (payment_limit * payment_price + bid) as i64)
                    .then_some(net as u64)
            });
            assert_eq!(
                c.net,
                expected.map(Amount::from),
                "case {case}, candidate {i}"
            );
            if let Some(net) = expected {
                assert_eq!(c.used, 275.into());
                assert_eq!(c.fees, (160 * response_price + 115 * payment_price).into());
                if expected_best.is_none_or(|(n, _)| net > n) {
                    expected_best = Some((net, i));
                }
            }
        }
        assert_eq!(d.selected, expected_best.map(|(_, i)| i), "case {case}");
        assert_eq!(
            r.state.balance("alice", "Q"),
            (capital + expected_best.map_or(0, |(n, _)| n)).into(),
            "case {case}"
        );
        assert_eq!(
            r.state.balance("builder", "Q"),
            expected_best
                .map_or(0, |_| 160 * response_price + 115 * payment_price + bid)
                .into(),
            "case {case}"
        );
        assert_eq!(
            r.blocks[0].receipts.len(),
            if expected_best.is_some() { 3 } else { 1 }
        );
        if expected_best.is_some() {
            profitable += 1;
        }
    }
    assert!(
        profitable > 30,
        "need nontrivial positive coverage: {profitable}"
    );
    eprintln!("768 independent integer cases; {profitable} profitable selected groups");
}

#[test]
fn hidden_pending_and_future_payloads_do_not_change_search_results() {
    let original = finish(fixture("backrun-public"));
    for at in [0, 8, 10, 17] {
        for amount in [1, 200, 900] {
            let mut v = fixture("backrun-public");
            v["accounts"]["carol"] = v["accounts"]["alice"].clone();
            v["agents"]["interferer"] = json!({"controlled_accounts":["carol"]});
            v["scheduler"]["agents"]["interferer"] = v["scheduler"]["agents"]["recipient"].clone();
            let mut e = v["scheduler"]["emissions"][0].clone();
            e["at"] = num(at);
            e["producer_sequence"] = num(1);
            e["transaction"]["id"] = json!("secret");
            e["transaction"]["agent"] = json!("interferer");
            e["transaction"]["sender"] = json!("carol");
            e["transaction"]["price"] = num(1);
            e["transaction"]["program"][0]["pool"] = json!("second");
            e["transaction"]["program"][0]["amount"] = num(amount);
            e["delivery"]["channel"] = no_recipients();
            e["delivery"]["builder_delay"] = num(0);
            v["scheduler"]["emissions"].as_array_mut().unwrap().push(e);
            let changed = finish(v);
            assert_eq!(
                normalized(search(&original, 0)),
                normalized(search(&changed, 0)),
                "at {at}, amount {amount}"
            );
        }
    }
}

#[test]
fn rejected_authorship_prefix_and_hidden_native_bundle_members_never_start_routes() {
    for field in ["agent", "sender", "chain"] {
        let mut v = fixture("backrun-public");
        v["scheduler"]["emissions"][0]["transaction"][field] = json!("forged");
        let r = finish(v);
        assert_eq!(search(&r, 0).metrics.prefixes, 1);
        assert_eq!(search(&r, 0).metrics.candidates, 0);
        assert!(r.blocks[0].receipts.is_empty());
    }
    let mut v = fixture("backrun-public");
    let mut e = v["scheduler"]["emissions"][0].clone();
    let tx = e["transaction"].clone();
    e.as_object_mut().unwrap().remove("transaction");
    e["bundle"] = json!({"id":"external-group","agent":"recipient",
        "members":[{"type":"Own","transaction":tx}],"min_block":"1","max_block":"1",
        "deadline":"100","allowed_reverts":[],"bid":"0","payment_tx_id":null});
    v["scheduler"]["bundles"]["emissions"] = json!([e]);
    v["scheduler"]["emissions"] = json!([]);
    let r = finish(v);
    assert!(search(&r, 0).decisions.is_empty());
    assert_eq!(r.blocks[0].receipts.len(), 1);
}

#[test]
fn ttl_and_stale_trigger_checkpoint_replay_are_byte_identical() {
    for retention in [1, 32] {
        let mut v = fixture("backrun-public");
        v["scheduler"]["limits"]["retention"] = json!(retention);
        v["scheduler"]["backrun"]["policies"]["trader"]["reaction_delay"] = num(5);
        let mut r = Run::new(scenario(v)).unwrap();
        r.step().unwrap();
        let checkpoint = r.checkpoint().unwrap();
        let mut resumed = Run::restore(&checkpoint).unwrap();
        r.finish().unwrap();
        resumed.finish().unwrap();
        assert_eq!(
            serde_json::to_vec(&r).unwrap(),
            serde_json::to_vec(&resumed).unwrap()
        );
        let report = search(&r, 1);
        assert_eq!(report.decisions.len(), 1);
        if retention == 1 {
            assert_eq!(report.decisions[0].reason, "TriggerUnavailable");
            assert_eq!(report.metrics.prefixes, 0);
        } else {
            assert!(report.decisions[0].reason.starts_with("TriggerRejected:"));
            assert_eq!(report.metrics.prefixes, 1);
        }
        assert_eq!(report.metrics.candidates, 0);
    }
}

#[test]
fn inflight_own_group_busy_guard_does_not_depend_on_visibility() {
    let mut v = fixture("backrun-public");
    let mut e = v["scheduler"]["emissions"][0].clone();
    e["at"] = num(0);
    e["producer_id"] = json!("own");
    e["delivery"]["builder_delay"] = num(30);
    e["delivery"]["channel"] = no_recipients();
    let mut tx = e["transaction"].clone();
    tx["id"] = json!("own-member");
    tx["agent"] = json!("trader");
    tx["sender"] = json!("alice");
    tx["program"] = json!([]);
    e.as_object_mut().unwrap().remove("transaction");
    e["bundle"] = json!({"id":"own-inflight","agent":"trader","members":[{"type":"Own","transaction":tx}],
        "min_block":"1","max_block":"4","deadline":"100","allowed_reverts":[],"bid":"0","payment_tx_id":null});
    v["scheduler"]["bundles"]["emissions"] = json!([e]);
    let r = finish(v);
    assert_eq!(search(&r, 0).decisions[0].reason, "BusyNonce");
    assert_eq!(search(&r, 0).metrics.prefixes, 0);
    assert_eq!(r.state.balance("alice", "Q"), 10000.into());
}

#[test]
fn own_nonce_future_funding_is_not_available_in_preview() {
    let mut v = fixture("backrun-public");
    v["accounts"]["alice"]["balances"]["Q"] = num(0);
    v["accounts"]["fund"] = v["accounts"]["bob"].clone();
    v["agents"]["funder"] = json!({"controlled_accounts":["fund"]});
    v["scheduler"]["agents"]["funder"] = v["scheduler"]["agents"]["recipient"].clone();
    let mut e = v["scheduler"]["emissions"][0].clone();
    e["at"] = num(0);
    e["producer_id"] = json!("funding");
    e["delivery"]["channel"] = no_recipients();
    e["delivery"]["builder_delay"] = num(0);
    e["transaction"]["id"] = json!("funding");
    e["transaction"]["agent"] = json!("funder");
    e["transaction"]["sender"] = json!("fund");
    e["transaction"]["price"] = num(1);
    e["transaction"]["program"] =
        json!([{"type":"Transfer","asset":"Q","to":"alice","amount":"500"}]);
    v["scheduler"]["emissions"].as_array_mut().unwrap().push(e);
    let r = finish(v);
    assert!(search(&r, 0).decisions[0].selected.is_none());
    assert_eq!(r.state.balance("alice", "Q"), 500.into());
    assert_eq!(r.state.accounts["alice"].nonce, 0.into());
}

#[test]
fn reaction_limit_is_shared_and_skip_does_not_consume_intent() {
    let mut v = fixture("backrun-competition");
    v["scheduler"]["limits"]["reactions_per_round"] = json!(1);
    let r = finish(v);
    assert_eq!(search(&r, 0).decisions.len(), 2);
    assert_eq!(search(&r, 0).decisions[1].reason, "ReactionLimit");
    assert_eq!(search(&r, 0).metrics.prefixes, 1);
    let mut v = fixture("backrun-public");
    let mut early = v["scheduler"]["emissions"][0].clone();
    early["at"] = num(0);
    early["producer_sequence"] = num(1);
    early["transaction"]["id"] = json!("skip-gap");
    early["transaction"]["nonce"] = num(7);
    v["scheduler"]["emissions"]
        .as_array_mut()
        .unwrap()
        .push(early);
    let r = finish(v);
    assert_eq!(search(&r, 0).decisions.len(), 2);
    assert!(search(&r, 0).decisions[0].bundle.is_none());
    assert!(search(&r, 0).decisions[1].bundle.is_some());
    assert_eq!(r.state.balance("alice", "Q"), 10011.into());
}

#[test]
fn every_small_search_cap_preserves_prefix_metrics_and_global_bounds() {
    for cap in ["venue_checks", "candidates_per_round", "member_attempts"] {
        for n in 1..=32 {
            let mut v = fixture("backrun-competition");
            v["scheduler"]["backrun"]["limits"][cap] = json!(n);
            let r = finish(v);
            let m = &search(&r, 0).metrics;
            let actual = match cap {
                "venue_checks" => m.venue_checks,
                "candidates_per_round" => m.candidates,
                _ => m.members,
            };
            assert!(actual <= n, "{cap}={n}: {actual}");
            assert_eq!(
                m.candidates,
                search(&r, 0)
                    .decisions
                    .iter()
                    .map(|d| d.candidates.len())
                    .sum::<usize>()
            );
            assert_eq!(
                m.members,
                m.prefixes
                    + search(&r, 0)
                        .decisions
                        .iter()
                        .flat_map(|d| &d.candidates)
                        .map(|c| c.attempts)
                        .sum::<usize>()
            );
            let expected_used = search(&r, 0).decisions.iter().fold(Amount::ZERO, |acc, d| {
                acc.checked_add(d.prefix.as_ref().map_or(Amount::ZERO, |r| r.used))
                    .unwrap()
                    .checked_add(
                        d.candidates
                            .iter()
                            .fold(Amount::ZERO, |a, c| a.checked_add(c.used).unwrap()),
                    )
                    .unwrap()
            });
            assert_eq!(m.used, expected_used);
        }
    }
}

#[test]
fn dfs_enumerates_only_closed_simple_paths_in_public_venue_order() {
    let mut v = fixture("backrun-public");
    v["assets"]["R"] = json!({"decimals":0});
    for (venue, account, x, y) in [
        ("middle", "pool-r", "X", "R"),
        ("last", "pool-q", "R", "Q"),
        ("unused", "isolated", "X", "R"),
    ] {
        v["accounts"][account] = json!({"nonce":"0","balances":{x:"1000",y:"1000"}});
        v["protocols"][venue] = json!({"account":account,"x":x,"y":y,"fee_bps":0,"owner":"alice"});
    }
    v["scheduler"]["backrun"]["policies"]["trader"]["inputs"] = json!(["100"]);
    let r = finish(v);
    let paths: Vec<Vec<&str>> = search(&r, 0).decisions[0]
        .candidates
        .iter()
        .map(|c| c.hops.iter().map(|h| h.venue()).collect())
        .collect();
    assert_eq!(
        paths,
        vec![
            vec!["cpmm", "middle", "last"],
            vec!["cpmm", "second"],
            vec!["cpmm", "unused", "last"],
            vec!["last", "middle", "cpmm"],
            vec!["last", "middle", "second"],
            vec!["last", "unused", "cpmm"],
            vec!["last", "unused", "second"],
            vec!["second", "cpmm"],
            vec!["second", "middle", "last"],
            vec!["second", "unused", "last"]
        ]
    );
}

#[test]
fn checked_event_id_model_error_keeps_committed_run_and_queue_unchanged() {
    let max = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    let mut r = Run::new(scenario(fixture("backrun-public"))).unwrap();
    r.scheduler.as_mut().unwrap().next_event_id = serde_json::from_value(json!(max)).unwrap();
    let before = r.clone();
    let err = r.step().unwrap_err();
    assert!(err.contains("ModelError"));
    assert_eq!(r, before);
    assert_eq!(r.step().unwrap_err(), err);
}

#[test]
fn exhausted_searcher_nonce_must_skip_without_aborting_unrelated_victim() {
    let max = "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    for bid in [0, 5] {
        let mut v = fixture("backrun-public");
        v["accounts"]["alice"]["nonce"] = json!(max);
        v["scheduler"]["backrun"]["policies"]["trader"]["bid"] = num(bid);
        let mut r = Run::new(scenario(v)).unwrap();
        r.step().unwrap_or_else(|e|panic!("bid={bid}: valid scenario must reject exhausted search nonce without aborting round: {e}"));
        assert!(search(&r, 0).decisions[0].bundle.is_none());
        assert_eq!(r.blocks[0].receipts.len(), 1);
        assert_eq!(r.blocks[0].receipts[0].tx_id, "victim");
    }
}

#[test]
fn nonrepresentable_user_lifetime_is_input_rejection_or_skip_not_model_error() {
    let max: Amount = serde_json::from_value(json!(
        "115792089237316195423570985008687907853269984665640564039457584007913129639935"
    ))
    .unwrap();
    // Actual reaction is t=9. MAX-9 has the exactly representable deadline MAX;
    // MAX-8 and MAX must be rejected as input or skipped without a round failure.
    for gap in [9u64, 8, 0] {
        let mut v = fixture("backrun-public");
        v["scheduler"]["backrun"]["policies"]["trader"]["lifetime"] =
            json!(max.checked_sub(gap.into()).unwrap().to_string());
        let parsed = Scenario::parse(&serde_json::to_vec(&v).unwrap());
        if gap < 9 && parsed.is_err() {
            continue;
        }
        let mut r = Run::new(parsed.unwrap()).unwrap();
        r.finish().unwrap_or_else(|e| {
            panic!("lifetime MAX-{gap}: input-domain overflow must not abort a round: {e}")
        });
        let d = &search(&r, 0).decisions[0];
        if gap == 9 {
            assert_eq!(d.bundle.as_ref().unwrap().deadline, max);
            assert_eq!(r.state.balance("alice", "Q"), 10011.into());
        } else {
            assert!(d.bundle.is_none());
            assert_eq!(r.state.balance("alice", "Q"), 10000.into());
        }
        assert_eq!(
            r.blocks
                .iter()
                .flat_map(|b| &b.receipts)
                .filter(|r| r.tx_id == "victim")
                .count(),
            1
        );
    }
}

#[test]
fn nonrepresentable_reaction_delay_is_rejected_without_round_failure() {
    let max: Amount = serde_json::from_value(json!(
        "115792089237316195423570985008687907853269984665640564039457584007913129639935"
    ))
    .unwrap();
    for gap in [8u64, 7, 0] {
        let mut v = fixture("backrun-public");
        v["scheduler"]["backrun"]["policies"]["trader"]["reaction_delay"] =
            json!(max.checked_sub(gap.into()).unwrap().to_string());
        let parsed = Scenario::parse(&serde_json::to_vec(&v).unwrap());
        if gap < 8 && parsed.is_err() {
            continue;
        }
        let mut r = Run::new(parsed.unwrap()).unwrap();
        r.finish().unwrap_or_else(|e| {
            panic!("reaction_delay MAX-{gap}: valid delivery must not abort round: {e}")
        });
        assert!(search(&r, 0).decisions.is_empty());
        assert_eq!(r.state.balance("alice", "Q"), 10000.into());
        assert_eq!(
            r.blocks
                .iter()
                .flat_map(|b| &b.receipts)
                .filter(|r| r.tx_id == "victim")
                .count(),
            1
        );
        let queued = r
            .scheduler
            .as_ref()
            .unwrap()
            .queue
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    chainarium_core::scheduler::EventKind::BackrunReaction { .. }
                )
            })
            .collect::<Vec<_>>();
        if gap == 8 {
            assert_eq!(queued.len(), 1);
            assert_eq!(queued[0].due_at, max);
        } else {
            assert!(queued.is_empty());
        }
    }
}

#[test]
fn full_64_candidate_cap_and_4096_venue_cap_are_hard_shared_bounds() {
    let mut v = fixture("backrun-competition");
    v["accounts"]["pool-c"] = v["accounts"]["pool"].clone();
    v["protocols"]["third"] = v["protocols"]["cpmm"].clone();
    v["protocols"]["third"]["account"] = json!("pool-c");
    for p in v["scheduler"]["backrun"]["policies"]
        .as_object_mut()
        .unwrap()
        .values_mut()
    {
        p["inputs"] = json!(["10", "25", "50", "100", "150", "200", "300", "400"]);
    }
    let r = finish(v);
    let report = search(&r, 0);
    assert_eq!(report.metrics.candidates, 64);
    assert_eq!(report.decisions[0].candidates.len(), 48);
    assert_eq!(report.decisions[1].candidates.len(), 16);
    assert!(report.metrics.truncated && report.decisions[1].truncated);
    for venues in [512, 513] {
        let mut v = fixture("backrun-public");
        v["scheduler"]["rounds"] = json!(1);
        v["assets"]["R"] = json!({"decimals":0});
        v["protocols"] = json!({});
        v["scheduler"]["emissions"][0]["transaction"]["program"] = json!([]);
        v["scheduler"]["backrun"]["policies"]["trader"]["inputs"] =
            json!(["10", "25", "50", "100", "150", "200", "300", "400"]);
        for i in 0..venues {
            let id = format!("disconnected-{i:04}");
            v["accounts"][&id] = json!({"nonce":"0","balances":{"X":"1000","R":"1000"}});
            v["protocols"][&id] = json!({"account":id,"x":"X","y":"R","fee_bps":0,"owner":"alice"});
        }
        let r = finish(v);
        let report = search(&r, 0);
        assert_eq!(report.metrics.venue_checks, 4096);
        assert_eq!(report.metrics.candidates, 0);
        assert_eq!(report.metrics.truncated, venues == 513);
    }
}

#[test]
fn all_six_presets_restore_every_boundary_byte_for_byte() {
    for name in [
        "public",
        "private",
        "delayed",
        "fifo",
        "competition",
        "fees",
    ] {
        let v = fixture(&format!("backrun-{name}"));
        let r = finish(v.clone());
        for boundary in 0..=r.blocks.len() {
            let mut partial = Run::new(scenario(v.clone())).unwrap();
            for _ in 0..boundary {
                partial.step().unwrap();
            }
            let mut resumed = Run::restore(&partial.checkpoint().unwrap()).unwrap();
            resumed.finish().unwrap();
            assert_eq!(
                serde_json::to_vec(&r).unwrap(),
                serde_json::to_vec(&resumed).unwrap(),
                "{name}:{boundary}"
            );
        }
    }
}

#[test]
fn system_transitions_match_legacy_with_backrun_previews_on_credit_and_twap() {
    for name in ["05-credit", "twap"] {
        let original = fixture(name);
        let legacy = finish(original.clone());
        let mut v = original;
        let base = fixture("backrun-public");
        let models = v["model_versions"].as_array_mut().unwrap();
        models.retain(|m| m != "scheduler-m1-v1");
        for m in base["model_versions"].as_array().unwrap() {
            if !models.contains(m) {
                models.push(m.clone());
            }
        }
        v["accounts"]["search-wallet"] = json!({"nonce":"0","balances":{"Q":"10000","X":"1000"}});
        v["agents"]["search-agent"] = json!({"controlled_accounts":["search-wallet"]});
        let blocks = v["blocks"].as_array().unwrap().clone();
        let interval = v["chain"]["interval"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let mut c = base["scheduler"].clone();
        c["rounds"] = json!(blocks.len());
        c["builder"] = json!({"type":"Fifo"});
        c["agents"] = json!({});
        c["emissions"] = json!([]);
        for a in v["agents"].as_object().unwrap().keys() {
            c["agents"][a] = json!({"observe_public":a=="search-agent","observation_delay":"0",
                "outgoing":{"channel":{"type":"Public"},"builder_delay":"0"}});
        }
        let mut p = base["scheduler"]["backrun"]["policies"]["trader"].clone();
        p["account"] = json!("search-wallet");
        p["reaction_delay"] = num(0);
        c["backrun"]["policies"] = json!({"search-agent":p});
        for (i, block) in blocks.iter().enumerate() {
            for (j, tx) in block.as_array().unwrap().iter().enumerate() {
                c["emissions"].as_array_mut().unwrap().push(
                    json!({"at":((i as u64+1)*interval).to_string(),
                "producer_id":"inputs","producer_sequence":(i*100+j).to_string(),"transaction":tx,
                "delivery":{"channel":{"type":"Public"},"builder_delay":"0"}}),
                );
            }
        }
        v["blocks"] = json!([]);
        v["scheduler"] = c;
        let run = finish(v);
        assert!(
            run.blocks
                .iter()
                .map(|b| b
                    .scheduler
                    .as_ref()
                    .unwrap()
                    .search
                    .as_ref()
                    .unwrap()
                    .metrics
                    .prefixes)
                .sum::<usize>()
                > 0
        );
        for (a, b) in run.blocks.iter().zip(&legacy.blocks) {
            assert_eq!(a.receipts, b.receipts, "{name} block {}", a.number);
            assert_eq!(
                a.system_events, b.system_events,
                "{name} block {}",
                a.number
            );
            assert_eq!(a.state.lending, b.state.lending);
            assert_eq!(a.state.price_history, b.state.price_history);
            assert_eq!(a.state.feeds, b.state.feeds);
        }
    }
}
