//! Independent neighboring-boundary verification for fix 467876f only.
use chainarium_core::*;
use serde_json::{Value, json};

#[test]
fn whole_own_nonce_sequence_at_max_minus_zero_through_four() {
    let max: Amount = serde_json::from_value(json!(
        "115792089237316195423570985008687907853269984665640564039457584007913129639935"
    ))
    .unwrap();
    for gap in 0..=4u64 {
        for bid in [0, 5u64] {
            let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../scenarios/demos/sc-backrun-public.json");
            let mut v: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
            let initial = max.checked_sub(gap.into()).unwrap();
            v["accounts"]["alice"]["nonce"] = json!(initial.to_string());
            v["scheduler"]["emissions"][0]["delivery"]["builder_delay"] = json!("4");
            v["scheduler"]["backrun"]["policies"]["trader"]["bid"] = json!(bid.to_string());
            v["scheduler"]["backrun"]["policies"]["trader"]["inputs"] = json!(["100"]);
            let s = Scenario::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
            let mut r = Run::new(s).unwrap();
            r.finish().unwrap();
            let report = r.blocks[0]
                .scheduler
                .as_ref()
                .unwrap()
                .search
                .as_ref()
                .unwrap();
            let d = &report.decisions[0];
            let increments = if bid == 0 { 1u64 } else { 2u64 };
            let eligible = gap >= increments;
            assert_eq!(d.bundle.is_some(), eligible, "gap={gap}, bid={bid}");
            assert_eq!(
                r.state.accounts["alice"].nonce,
                if eligible {
                    initial.checked_add(increments.into()).unwrap()
                } else {
                    initial
                }
            );
            assert_eq!(
                r.state.balance("alice", "Q"),
                (10000 + if eligible { 16 - bid } else { 0 }).into()
            );
            assert_eq!(
                r.state.balance("builder", "Q"),
                (if eligible { bid } else { 0 }).into()
            );
            assert_eq!(
                r.blocks
                    .iter()
                    .flat_map(|b| &b.receipts)
                    .filter(|r| r.tx_id == "victim")
                    .count(),
                1
            );
            if !eligible {
                assert_eq!(d.reason, "NonceExhausted");
                assert_eq!(report.metrics.candidates, 0);
                assert_eq!(report.metrics.members, 1);
                assert_eq!(report.metrics.venue_checks, 0);
            }
            let mut replay = Run::restore(&r.checkpoint().unwrap()).unwrap();
            replay.finish().unwrap();
            assert_eq!(
                serde_json::to_vec(&r).unwrap(),
                serde_json::to_vec(&replay).unwrap()
            );
        }
    }
}
