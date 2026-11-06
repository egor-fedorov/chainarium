//! Independent DEC-22 review tests. Copy into crates/chainarium-core/tests/independent_dec22.rs.
//! Only fixture loading uses repository data. The differential oracle below uses ordinary u64
//! arithmetic and the contract's 100/10/5 resource costs, without simulator execution helpers.
use chainarium_core::{
    scheduler::{self as sch, Builder, Channel, Delivery, Status, bundles as g},
    *,
};
use std::collections::BTreeSet;

fn fixture(name: &str) -> Scenario {
    Scenario::parse(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../scenarios/demos/sc-{name}.json")),
        )
        .unwrap(),
    )
    .unwrap()
}
fn clean() -> Scenario {
    let mut s = fixture("bundle-allowed");
    let c = s.scheduler.as_mut().unwrap();
    c.rounds = 1;
    c.emissions.clear();
    c.bundles.as_mut().unwrap().emissions.clear();
    s
}
fn tx(id: &str, sender: &str, nonce: u64, amount: u64, to: &str) -> Transaction {
    Transaction {
        id: id.into(),
        agent: if sender == "bob" {
            "recipient"
        } else {
            "trader"
        }
        .into(),
        sender: sender.into(),
        chain: "lab-1".into(),
        nonce: nonce.into(),
        limit: 200.into(),
        price: 1.into(),
        deadline: 100.into(),
        program: vec![Call::Transfer {
            to: to.into(),
            asset: "Q".into(),
            amount: amount.into(),
        }],
    }
}
fn delivery() -> Delivery {
    Delivery {
        channel: Channel::Public,
        builder_delay: 0.into(),
    }
}
fn native(s: &mut Scenario, at: u64, t: Transaction) {
    s.scheduler.as_mut().unwrap().emissions.push(sch::Emission {
        at: at.into(),
        producer_id: "native".into(),
        producer_sequence: at.into(),
        transaction: t,
        delivery: delivery(),
    });
}
fn group(
    s: &mut Scenario,
    at: u64,
    id: &str,
    members: Vec<Transaction>,
    allowed: BTreeSet<String>,
    bid: u64,
) {
    let agent = members[0].agent.clone();
    let payment_tx_id = if bid == 0 {
        None
    } else {
        Some(members.last().unwrap().id.clone())
    };
    s.scheduler
        .as_mut()
        .unwrap()
        .bundles
        .as_mut()
        .unwrap()
        .emissions
        .push(g::Emission {
            at: at.into(),
            producer_id: "bundles".into(),
            producer_sequence: at.into(),
            delivery: delivery(),
            bundle: g::Bundle {
                id: id.into(),
                agent,
                members: members
                    .into_iter()
                    .map(|transaction| g::Member::Own {
                        transaction: Box::new(transaction),
                    })
                    .collect(),
                min_block: 1.into(),
                max_block: 10.into(),
                deadline: 100.into(),
                allowed_reverts: allowed,
                bid: bid.into(),
                payment_tx_id,
            },
        });
}
fn selected_ids(r: &Run) -> Vec<String> {
    r.blocks[0]
        .scheduler
        .as_ref()
        .unwrap()
        .probes
        .iter()
        .filter(|p| p.selected)
        .map(|p| p.candidate.label())
        .collect()
}

#[derive(Clone, Debug)]
struct RefTx {
    id: String,
    nonce: u64,
    limit: u64,
    price: u64,
    amount: u64,
    builder: bool,
    revert: bool,
    allowed: bool,
}
#[derive(Clone, Debug)]
struct RefCandidate {
    id: String,
    bundle: bool,
    txs: Vec<RefTx>,
    bid: u64,
}
#[derive(Clone, Debug, PartialEq)]
struct RefState {
    alice: u64,
    bob: u64,
    builder: u64,
    nonce: u64,
    used: u64,
    receipts: Vec<(String, String, u64, u64)>,
}
fn trial(c: &RefCandidate, state: &RefState, block_limit: u64) -> Option<(RefState, u64)> {
    let mut w = state.clone();
    let mut score = c.bid;
    for t in &c.txs {
        if t.nonce != w.nonce || t.limit > block_limit - w.used || w.alice < t.limit * t.price {
            return None;
        }
        let available = w.alice - t.limit * t.price;
        let need = if t.revert { 125 } else { 115 };
        let (status, used) = if t.limit < 115 {
            ("ResourceExhausted", t.limit)
        } else if available < t.amount {
            ("ProtocolRevert", 115)
        } else if t.limit < need {
            ("ResourceExhausted", t.limit)
        } else if t.revert {
            ("ProtocolRevert", 125)
        } else {
            ("Success", 115)
        };
        if c.bundle && status != "Success" && !t.allowed {
            return None;
        }
        let fee = used * t.price;
        w.alice -= fee;
        w.builder += fee;
        w.nonce += 1;
        w.used += used;
        score += fee;
        if status == "Success" {
            w.alice -= t.amount;
            if t.builder {
                w.builder += t.amount;
            } else {
                w.bob += t.amount;
            }
        }
        w.receipts.push((t.id.clone(), status.into(), used, fee));
    }
    Some((w, score))
}
fn rand(seed: &mut u64, modulus: u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*seed >> 32) % modulus
}

#[test]
fn differential_640_mixed_candidate_worlds_against_integer_contract() {
    for case in 0..128u64 {
        for mode in 0..5 {
            let mut seed = case + 7;
            let mut s = clean();
            let block_limit = 200 + rand(&mut seed, 701);
            s.chain.block_limit = block_limit.into();
            let mut candidates = vec![];
            for i in 0..5 {
                let bundled = i < 3;
                let count = if bundled { 1 + rand(&mut seed, 3) } else { 1 };
                let start = rand(&mut seed, 4);
                let bid = if bundled && count > 1 && rand(&mut seed, 2) == 0 {
                    1 + rand(&mut seed, 50)
                } else {
                    0
                };
                let mut rr = vec![];
                let mut tt = vec![];
                let mut allowed = BTreeSet::new();
                for j in 0..count {
                    let payment = bid > 0 && j == count - 1;
                    let item = RefTx {
                        id: format!("tx-{i}-{j}"),
                        nonce: start + j,
                        limit: [100, 114, 115, 124, 125, 200][rand(&mut seed, 6) as usize],
                        price: rand(&mut seed, 4),
                        amount: if payment { bid } else { rand(&mut seed, 201) },
                        builder: payment || rand(&mut seed, 4) == 0,
                        revert: !payment && rand(&mut seed, 3) == 0,
                        allowed: !payment && rand(&mut seed, 2) == 0,
                    };
                    let mut t = tx(
                        &item.id,
                        "alice",
                        item.nonce,
                        item.amount,
                        if item.builder { "builder" } else { "bob" },
                    );
                    t.limit = item.limit.into();
                    t.price = item.price.into();
                    if item.revert {
                        t.program.push(Call::AssertCondition {
                            condition: false,
                            message: "late failure".into(),
                        });
                    }
                    if item.allowed {
                        allowed.insert(item.id.clone());
                    }
                    rr.push(item);
                    tt.push(t);
                }
                let id = if bundled {
                    format!("bundle:b-{i}")
                } else {
                    format!("tx:tx-{i}-0")
                };
                if bundled {
                    group(&mut s, i + 1, &format!("b-{i}"), tt, allowed, bid);
                } else {
                    native(&mut s, i + 1, tt.remove(0));
                }
                candidates.push(RefCandidate {
                    id,
                    bundle: bundled,
                    txs: rr,
                    bid,
                });
            }
            s.scheduler.as_mut().unwrap().builder = match mode {
                0 => Builder::Fifo,
                1 => Builder::FeePriority,
                2 => Builder::Greedy,
                3 => Builder::FixedItems {
                    priority: (0..5)
                        .rev()
                        .map(|i| {
                            if i < 3 {
                                g::CandidateId::Bundle {
                                    id: format!("b-{i}"),
                                }
                            } else {
                                g::CandidateId::Transaction {
                                    id: format!("tx-{i}-0"),
                                }
                            }
                        })
                        .collect(),
                },
                _ => Builder::Fixed {
                    priority: vec!["tx-4-0".into(), "tx-3-0".into()],
                },
            };
            let mut expected = RefState {
                alice: 1000,
                bob: 0,
                builder: 0,
                nonce: 0,
                used: 0,
                receipts: vec![],
            };
            let mut remaining: Vec<usize> = (0..5).collect();
            let mut selected = vec![];
            loop {
                let mut order = remaining.clone();
                if mode == 1 {
                    order.sort_by_key(|i| {
                        (
                            std::cmp::Reverse(
                                candidates[*i].txs.iter().map(|t| t.price).min().unwrap(),
                            ),
                            *i,
                        )
                    });
                }
                if mode == 3 {
                    order.reverse();
                }
                if mode == 4 {
                    order.retain(|i| *i >= 3);
                    order.reverse();
                }
                let mut options = vec![];
                for i in order {
                    if let Some((next, score)) = trial(&candidates[i], &expected, block_limit) {
                        options.push((i, next, score));
                        if mode != 2 {
                            break;
                        }
                    }
                }
                if options.is_empty() {
                    break;
                }
                if mode == 2 {
                    options.sort_by_key(|(i, w, score)| {
                        (std::cmp::Reverse(*score), w.used - expected.used, *i)
                    });
                }
                let (i, next, _) = options.remove(0);
                expected = next;
                selected.push(candidates[i].id.clone());
                remaining.retain(|j| *j != i);
            }
            let mut run = Run::new(s).unwrap();
            run.finish().unwrap();
            let actual = RefState {
                alice: run.state.balance("alice", "Q").to_string().parse().unwrap(),
                bob: run.state.balance("bob", "Q").to_string().parse().unwrap(),
                builder: run
                    .state
                    .balance("builder", "Q")
                    .to_string()
                    .parse()
                    .unwrap(),
                nonce: run.state.accounts["alice"]
                    .nonce
                    .to_string()
                    .parse()
                    .unwrap(),
                used: run.blocks[0].used.to_string().parse().unwrap(),
                receipts: run.blocks[0]
                    .receipts
                    .iter()
                    .map(|r| {
                        (
                            r.tx_id.clone(),
                            r.status.clone(),
                            r.used.to_string().parse().unwrap(),
                            r.fee.to_string().parse().unwrap(),
                        )
                    })
                    .collect(),
            };
            assert_eq!(actual, expected, "case {case}, mode {mode}, {candidates:?}");
            assert_eq!(selected_ids(&run), selected, "case {case}, mode {mode}");
        }
    }
}

#[test]
fn earlier_funding_group_unlocks_failed_group_without_double_fees() {
    let mut s = clean();
    s.chain.block_limit = 1000.into();
    group(
        &mut s,
        1,
        "spend",
        vec![tx("spend", "bob", 0, 10, "alice")],
        BTreeSet::new(),
        0,
    );
    group(
        &mut s,
        2,
        "fund",
        vec![tx("fund", "alice", 0, 250, "bob")],
        BTreeSet::new(),
        0,
    );
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert_eq!(selected_ids(&r), vec!["bundle:fund", "bundle:spend"]);
    assert_eq!(r.state.balance("alice", "Q"), 645.into());
    assert_eq!(r.state.balance("bob", "Q"), 125.into());
    assert_eq!(r.state.balance("builder", "Q"), 230.into());
    assert_eq!(r.blocks[0].scheduler.as_ref().unwrap().metrics.probes, 3);
}

#[test]
fn inclusion_slots_and_eligibility_budget_never_commit_bundle_prefix() {
    for (slots, checks, want) in [(2, 65536, 0), (3, 2, 0), (3, 3, 3)] {
        let mut s = fixture("bundle-allowed");
        let c = s.scheduler.as_mut().unwrap();
        c.rounds = 1;
        c.limits.inclusion_attempts = slots;
        c.limits.eligibility_checks = checks;
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert_eq!(r.blocks[0].receipts.len(), want);
        if want == 0 {
            assert_eq!(r.state.accounts, r.initial.accounts);
            assert!(r.blocks[0].scheduler.as_ref().unwrap().metrics.truncated);
        }
    }
}

#[test]
fn mixed_pending_cap_is_enforced_in_both_delivery_orders() {
    for bundle_first in [false, true] {
        let mut s = clean();
        let c = s.scheduler.as_mut().unwrap();
        c.limits.pending = 1;
        c.builder = Builder::FixedItems { priority: vec![] };
        group(
            &mut s,
            if bundle_first { 1 } else { 2 },
            "g",
            vec![tx("member", "alice", 0, 1, "bob")],
            BTreeSet::new(),
            0,
        );
        native(
            &mut s,
            if bundle_first { 2 } else { 1 },
            tx("native", "alice", 0, 1, "bob"),
        );
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        let st = r.scheduler.as_ref().unwrap();
        assert_eq!(st.pending.len() + st.bundle_pending.len(), 1);
        assert_eq!(st.bundle_pending.contains_key("g"), bundle_first);
        assert_eq!(st.pending.contains_key("native"), !bundle_first);
        assert_eq!(r.state.accounts, r.initial.accounts);
    }
}

#[test]
fn invalid_tail_does_not_poison_first_member_or_original_native_registry() {
    let mut s = clean();
    let first = tx("good", "alice", 0, 10, "bob");
    let mut bad = tx("bad", "alice", 1, 10, "bob");
    bad.chain = "foreign-chain".into();
    group(
        &mut s,
        1,
        "invalid",
        vec![first.clone(), bad],
        BTreeSet::new(),
        0,
    );
    group(&mut s, 2, "valid", vec![first], BTreeSet::new(), 0);
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let st = r.scheduler.as_ref().unwrap();
    assert_eq!(selected_ids(&r), vec!["bundle:valid"]);
    assert!(!st.known.contains_key("bad"));
    assert_eq!(r.state.balance("builder", "Q"), 115.into());
}

#[test]
fn duplicate_bundle_after_ttl_does_not_renew_its_arrival() {
    let mut s = clean();
    let c = s.scheduler.as_mut().unwrap();
    c.rounds = 3;
    c.limits.retention = 1;
    group(
        &mut s,
        1,
        "g",
        vec![tx("member", "alice", 0, 1, "bob")],
        BTreeSet::new(),
        0,
    );
    let cfg = s.scheduler.as_mut().unwrap().bundles.as_mut().unwrap();
    cfg.emissions[0].bundle.min_block = 3.into();
    let mut again = cfg.emissions[0].clone();
    again.at = 21.into();
    again.producer_sequence = 21.into();
    cfg.emissions.push(again);
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert!(r.blocks.iter().all(|b| b.receipts.is_empty()));
    assert!(
        matches!(&r.scheduler.as_ref().unwrap().bundles["g"].status,Status::Rejected{reason} if reason=="TTLExpired")
    );
}

#[test]
fn references_are_pinned_before_eviction_and_future_references_stay_rejected() {
    for at in [1, 3] {
        let mut s = clean();
        let c = s.scheduler.as_mut().unwrap();
        c.limits.visible = 1;
        c.builder = Builder::FixedItems {
            priority: vec![g::CandidateId::Bundle { id: "g".into() }],
        };
        let n = c.agents.get_mut("trader").unwrap();
        n.observe_public = true;
        n.observation_delay = 0.into();
        native(&mut s, 2, tx("victim", "bob", 0, 0, "alice"));
        // Give the observed foreign member enough money for execution.
        s.accounts
            .get_mut("bob")
            .unwrap()
            .balances
            .insert("Q".into(), 500.into());
        group(
            &mut s,
            at,
            "g",
            vec![tx("placeholder", "alice", 0, 0, "bob")],
            BTreeSet::new(),
            0,
        );
        let b = &mut s
            .scheduler
            .as_mut()
            .unwrap()
            .bundles
            .as_mut()
            .unwrap()
            .emissions[0]
            .bundle;
        b.members = vec![g::Member::VisibleTxId {
            tx_id: "victim".into(),
        }];
        native(&mut s, 4, tx("evict", "alice", 9, 0, "bob"));
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert!(
            r.scheduler.as_ref().unwrap().visible["trader"]
                .iter()
                .all(|m| m.transaction.id != "victim")
        );
        assert_eq!(r.blocks[0].receipts.len(), if at == 3 { 1 } else { 0 });
        if at == 1 {
            assert!(
                matches!(&r.scheduler.as_ref().unwrap().bundles["g"].status,Status::Rejected{reason} if reason=="UnknownVisibleTxId")
            );
        }
    }
}

#[test]
fn overlapping_and_inflight_candidates_roundtrip_and_tampering_is_detected() {
    let mut s = clean();
    s.scheduler.as_mut().unwrap().rounds = 3;
    let common = tx("shared", "alice", 0, 1, "bob");
    group(&mut s, 1, "one", vec![common.clone()], BTreeSet::new(), 0);
    group(&mut s, 2, "two", vec![common.clone()], BTreeSet::new(), 0);
    let cfg = s.scheduler.as_mut().unwrap().bundles.as_mut().unwrap();
    cfg.emissions[0].bundle.min_block = 2.into();
    cfg.emissions[1].delivery.builder_delay = 15.into();
    native(&mut s, 3, common);
    s.scheduler.as_mut().unwrap().emissions[0]
        .delivery
        .builder_delay = 25.into();
    let mut full = Run::new(s).unwrap();
    full.step().unwrap();
    let bytes = full.checkpoint().unwrap();
    let mut resumed = Run::restore(&bytes).unwrap();
    full.finish().unwrap();
    resumed.finish().unwrap();
    assert_eq!(full, resumed);
    assert_eq!(
        full.blocks.iter().map(|b| b.receipts.len()).sum::<usize>(),
        1
    );
    let mut altered: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    altered["run"]["scheduler"]["bundles"]["one"]["resolved"]["bid"] = serde_json::json!("9");
    altered["hash"] = serde_json::json!(chainarium_core::run::digest(
        &serde_json::to_vec(&altered["run"]).unwrap()
    ));
    assert!(
        Run::restore(&serde_json::to_vec(&altered).unwrap())
            .unwrap_err()
            .contains("ModelError")
    );
}

#[test]
fn many_overlapping_probes_do_not_repeat_credit_accrual() {
    let mut s = fixture("05-credit");
    let mut reference = Run::new(s.clone()).unwrap();
    reference.finish().unwrap();
    let mut cfg = clean().scheduler.unwrap();
    cfg.rounds = s.blocks.len();
    cfg.builder = Builder::Fifo;
    cfg.agents = s
        .agents
        .keys()
        .map(|id| {
            (
                id.clone(),
                sch::Network {
                    observe_public: false,
                    observation_delay: 0.into(),
                    outgoing: delivery(),
                },
            )
        })
        .collect();
    let interval = s.chain.interval.to_string().parse::<u64>().unwrap();
    for (i, block) in s.blocks.iter().enumerate() {
        for (j, t) in block.iter().enumerate() {
            for copy in 0..3 {
                cfg.bundles.as_mut().unwrap().emissions.push(g::Emission {
                    at: ((i as u64 + 1) * interval).into(),
                    producer_id: format!("{j}-{copy}"),
                    producer_sequence: (i as u64).into(),
                    delivery: delivery(),
                    bundle: g::Bundle {
                        id: format!("{i}-{j}-{copy}"),
                        agent: t.agent.clone(),
                        members: vec![g::Member::Own {
                            transaction: Box::new(t.clone()),
                        }],
                        min_block: (i as u64 + 1).into(),
                        max_block: (i as u64 + 1).into(),
                        deadline: t.deadline,
                        allowed_reverts: BTreeSet::from([t.id.clone()]),
                        bid: 0.into(),
                        payment_tx_id: None,
                    },
                });
            }
        }
    }
    // A permanently future nonce forces repeated failed probes ahead of the same FIFO sequence.
    let mut wait = cfg.bundles.as_ref().unwrap().emissions[0].clone();
    wait.producer_id = "!waiting-first".into();
    wait.bundle.id = "waiting".into();
    wait.bundle.max_block = 10.into();
    let g::Member::Own { transaction } = &mut wait.bundle.members[0] else {
        unreachable!()
    };
    transaction.id = "future-nonce".into();
    transaction.nonce = 99.into();
    wait.bundle.allowed_reverts.clear();
    cfg.bundles.as_mut().unwrap().emissions.push(wait);
    s.blocks.clear();
    s.model_versions.retain(|m| m != "scheduler-m1-v1");
    s.model_versions.extend(
        sch::MODELS[..3]
            .iter()
            .chain(g::MODELS.iter())
            .map(|m| m.to_string()),
    );
    s.scheduler = Some(cfg);
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(r.state, reference.state);
    for (actual, expected) in r.blocks.iter().zip(reference.blocks.iter()) {
        assert_eq!(actual.system_events, expected.system_events);
        assert_eq!(actual.receipts, expected.receipts);
    }
    assert!(
        r.blocks
            .iter()
            .any(|b| b.scheduler.as_ref().unwrap().metrics.probes > b.receipts.len())
    );
}

#[test]
fn real_score_overflow_aborts_round_after_successful_prefix() {
    let mut s = clean();
    s.chain.block_limit = 1000.into();
    s.scheduler.as_mut().unwrap().builder = Builder::Fifo;
    // Valid supply MAX, recycled by builder between two high native fees. Individual balances
    // and transaction budgets fit U256; the gross fee sum does not. No injected fault hooks.
    s.protocols.clear();
    for a in s.accounts.values_mut() {
        a.balances.insert("Q".into(), 0.into());
    }
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), Amount::MAX);
    s.agents
        .get_mut("trader")
        .unwrap()
        .controlled_accounts
        .insert("builder".into());
    let mut prefix = tx("prefix", "alice", 0, 0, "bob");
    prefix.price = 0.into();
    native(&mut s, 1, prefix);
    let price = Amount::MAX.mul_div(1.into(), 100.into(), false).unwrap();
    let fee = price.checked_mul(100.into()).unwrap();
    let mut high1 = tx("high1", "alice", 1, 0, "bob");
    high1.limit = 100.into();
    high1.price = price;
    high1.program.clear();
    let mut high2 = high1.clone();
    high2.id = "high2".into();
    high2.nonce = 2.into();
    let mut refund = tx("return-fee", "builder", 0, 0, "alice");
    refund.price = 0.into();
    refund.program = vec![Call::Transfer {
        to: "alice".into(),
        asset: "Q".into(),
        amount: fee,
    }];
    group(
        &mut s,
        2,
        "overflow",
        vec![high1, refund, high2],
        BTreeSet::new(),
        0,
    );
    let mut r = Run::new(s).unwrap();
    let before = r.clone();
    let error = r.step().unwrap_err();
    assert!(
        error.contains("ModelError") && error.contains("overflow"),
        "{error}"
    );
    assert_eq!(
        r, before,
        "state, receipts, cursors and queue must all roll back"
    );
}

#[test]
fn private_bundle_registry_does_not_grant_reference_authority() {
    for (authorized, delay, want) in [(false, 0, false), (true, 0, true), (true, 5, false)] {
        let mut s = clean();
        let c = s.scheduler.as_mut().unwrap();
        c.builder = Builder::FixedItems {
            priority: vec![g::CandidateId::Bundle {
                id: "reference".into(),
            }],
        };
        c.agents.get_mut("recipient").unwrap().observe_public = true;
        c.agents.get_mut("recipient").unwrap().observation_delay = delay.into();
        group(
            &mut s,
            1,
            "private-original",
            vec![tx("secret-member", "alice", 0, 11, "bob")],
            BTreeSet::new(),
            0,
        );
        group(
            &mut s,
            3,
            "reference",
            vec![tx("placeholder", "bob", 0, 0, "alice")],
            BTreeSet::new(),
            0,
        );
        let e = &mut s
            .scheduler
            .as_mut()
            .unwrap()
            .bundles
            .as_mut()
            .unwrap()
            .emissions;
        e[0].delivery.channel = Channel::Private {
            recipients: if authorized {
                BTreeSet::from(["recipient".into()])
            } else {
                BTreeSet::new()
            },
        };
        e[1].bundle.members = vec![g::Member::VisibleTxId {
            tx_id: "secret-member".into(),
        }];
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert_eq!(r.blocks[0].receipts.len(), usize::from(want));
        let st = r.scheduler.as_ref().unwrap();
        assert!(st.known.contains_key("secret-member"));
        if !want {
            assert!(
                matches!(&st.bundles["reference"].status,Status::Rejected{reason} if reason=="UnknownVisibleTxId")
            );
        }
        if !authorized {
            assert!(
                st.visible_transaction("recipient", "secret-member")
                    .is_none()
            );
            let obs = st
                .observation(&r.scenario, &r.state, "recipient", "bob")
                .unwrap();
            assert!(obs.bundle_messages.is_empty());
            assert!(obs.own_bundles.iter().all(|b| b.bundle.is_none()));
        }
    }
}

#[test]
fn exhausted_probe_budget_releases_waiting_candidate_only_next_round() {
    let mut s = clean();
    let c = s.scheduler.as_mut().unwrap();
    c.rounds = 2;
    c.bundles.as_mut().unwrap().limits.probes = 1;
    s.accounts
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("Q".into(), 1000.into());
    group(
        &mut s,
        1,
        "early",
        vec![tx("early", "alice", 0, 0, "bob")],
        BTreeSet::new(),
        0,
    );
    let mut later = tx("later", "bob", 0, 0, "alice");
    later.price = 3.into();
    group(&mut s, 2, "later", vec![later], BTreeSet::new(), 0);
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert_eq!(selected_ids(&r), vec!["bundle:early"]);
    assert!(
        matches!(&r.scheduler.as_ref().unwrap().bundles["later"].status,Status::Waiting{reason} if reason=="ProbeLimit")
    );
    assert_eq!(r.state.balance("builder", "Q"), 115.into());
    r.step().unwrap();
    assert_eq!(r.blocks[1].receipts[0].tx_id, "later");
    assert_eq!(r.state.balance("builder", "Q"), 460.into());
    for b in &r.blocks {
        assert_eq!(b.scheduler.as_ref().unwrap().metrics.probes, 1);
    }
}
