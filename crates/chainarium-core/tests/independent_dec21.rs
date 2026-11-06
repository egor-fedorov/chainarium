//! Independent DEC-21 review probes, retained from the reviewer's isolated snapshot.
use chainarium_core::{scheduler::*, *};
use std::collections::{BTreeMap, BTreeSet};

fn fixture(name: &str) -> Scenario {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/demos");
    Scenario::parse(&std::fs::read(root.join(format!("sc-scheduler-{name}.json"))).unwrap())
        .unwrap()
}
fn tx(
    id: &str,
    sender: usize,
    nonce: u64,
    limit: u64,
    price: u64,
    amount: u64,
    deadline: u64,
) -> Transaction {
    Transaction {
        id: id.into(),
        agent: ["trader", "recipient"][sender].into(),
        sender: ["alice", "bob"][sender].into(),
        chain: "lab-1".into(),
        nonce: nonce.into(),
        limit: limit.into(),
        price: price.into(),
        deadline: deadline.into(),
        program: vec![Call::Transfer {
            to: ["bob", "alice"][sender].into(),
            asset: "Q".into(),
            amount: amount.into(),
        }],
    }
}
fn emit(t: Transaction, at: u64, delay: u64, producer: &str, sequence: u64) -> Emission {
    Emission {
        at: at.into(),
        producer_id: producer.into(),
        producer_sequence: sequence.into(),
        transaction: t,
        delivery: Delivery {
            channel: Channel::Private {
                recipients: BTreeSet::new(),
            },
            builder_delay: delay.into(),
        },
    }
}
fn actions(r: &Run, name: &str) -> usize {
    r.blocks
        .iter()
        .flat_map(|b| &b.scheduler.as_ref().unwrap().trace)
        .filter(|t| t.action == name)
        .count()
}

#[derive(Clone)]
struct Case {
    id: String,
    sender: usize,
    nonce: u64,
    limit: u64,
    price: u64,
    amount: u64,
    deadline: u64,
    sent: u64,
    arrival: u64,
    ordinal: usize,
}
fn next(seed: &mut u64, bound: u64) -> u64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*seed >> 24) % bound
}

#[test]
fn independent_transfer_oracle_360_schedules_all_builders() {
    // A small integer accounting oracle: no native executor, scheduler helpers,
    // status reasoning or receipt results are used to derive expected outcomes.
    for mode in 0..3 {
        for seed_number in 0..120u64 {
            let mut seed = seed_number + 91;
            let mut s = fixture("nonce");
            let starting = [next(&mut seed, 1200), next(&mut seed, 1200)];
            for i in 0..2 {
                s.accounts
                    .get_mut(["alice", "bob"][i])
                    .unwrap()
                    .balances
                    .insert("Q".into(), starting[i].into());
            }
            let budget = 300 + next(&mut seed, 700);
            s.chain.block_limit = budget.into();
            let c = s.scheduler.as_mut().unwrap();
            c.rounds = 6;
            c.emissions.clear();
            c.reactive.clear();
            c.limits.pending = 1 + next(&mut seed, 10) as usize;
            c.limits.retention = 1 + next(&mut seed, 3) as usize;
            c.limits.inclusion_attempts = 1 + next(&mut seed, 5) as usize;
            c.limits.eligibility_checks = 1 + next(&mut seed, 16) as usize;
            let mut cases = Vec::new();
            for j in 0..24 {
                let sent = next(&mut seed, 50);
                let x = Case {
                    id: format!("tx-{j:02}"),
                    sender: next(&mut seed, 2) as usize,
                    nonce: next(&mut seed, 5),
                    limit: 115 + next(&mut seed, 500),
                    price: next(&mut seed, 4),
                    amount: next(&mut seed, 700),
                    deadline: next(&mut seed, 75),
                    sent,
                    arrival: sent + next(&mut seed, 17),
                    ordinal: 0,
                };
                cases.push(x);
            }
            // A stable producer maps all equal-time sends into the numerical sequence.
            let mut order: Vec<_> = (0..cases.len()).collect();
            order.sort_by_key(|i| (cases[*i].sent, *i));
            for (rank, i) in order.into_iter().enumerate() {
                cases[i].ordinal = rank;
            }
            for (i, x) in cases.iter().enumerate() {
                c.emissions.push(emit(
                    tx(
                        &x.id, x.sender, x.nonce, x.limit, x.price, x.amount, x.deadline,
                    ),
                    x.sent,
                    x.arrival - x.sent,
                    "producer",
                    i as u64,
                ));
            }
            let priority: Vec<_> = cases
                .iter()
                .rev()
                .filter(|x| x.ordinal % 3 != 0)
                .map(|x| x.id.clone())
                .collect();
            c.builder = match mode {
                0 => Builder::Fifo,
                1 => Builder::FeePriority,
                _ => Builder::Fixed {
                    priority: priority.clone(),
                },
            };
            let limits = c.limits.clone();
            let mut r = Run::new(s).unwrap();
            let mut balances = starting;
            let mut nonce = [0u64; 2];
            let mut fees = 0u64;
            let mut pending: BTreeMap<usize, usize> = BTreeMap::new();
            let mut seen = BTreeSet::new();
            for round in 1..=6usize {
                let cutoff = round as u64 * 10;
                let mut arriving: Vec<_> = cases
                    .iter()
                    .enumerate()
                    .filter(|(i, x)| !seen.contains(i) && x.arrival <= cutoff)
                    .map(|(i, _)| i)
                    .collect();
                arriving.sort_by_key(|i| (cases[*i].arrival, cases[*i].ordinal));
                for i in arriving {
                    let x = &cases[i];
                    seen.insert(i);
                    if x.deadline >= x.arrival
                        && x.limit <= budget
                        && x.nonce >= nonce[x.sender]
                        && pending.len() < limits.pending
                    {
                        pending.insert(i, round);
                    }
                }
                let mut expected = vec![];
                let mut used = 0u64;
                let mut checks = 0usize;
                loop {
                    pending.retain(|i, first| {
                        let x = &cases[*i];
                        x.deadline >= cutoff
                            && x.nonce >= nonce[x.sender]
                            && round - *first < limits.retention
                    });
                    if expected.len() >= limits.inclusion_attempts {
                        break;
                    }
                    let mut available: Vec<_> = pending.keys().copied().collect();
                    available.retain(|i| mode != 2 || priority.contains(&cases[*i].id));
                    available.sort_by_key(|i| {
                        let x = &cases[*i];
                        (
                            if mode == 2 {
                                priority.iter().position(|id| id == &x.id).unwrap() as u64
                            } else if mode == 1 {
                                4 - x.price
                            } else {
                                0
                            },
                            x.arrival,
                            x.ordinal,
                        )
                    });
                    let mut chosen = None;
                    for i in available {
                        if checks == limits.eligibility_checks {
                            break;
                        }
                        checks += 1;
                        let x = &cases[i];
                        if x.nonce == nonce[x.sender]
                            && x.limit <= budget - used
                            && balances[x.sender] >= x.limit * x.price
                        {
                            chosen = Some(i);
                            break;
                        }
                    }
                    let Some(i) = chosen else { break };
                    let x = &cases[i];
                    pending.remove(&i);
                    let success = balances[x.sender] - x.limit * x.price >= x.amount;
                    if success {
                        balances[x.sender] -= x.amount;
                        balances[1 - x.sender] += x.amount;
                    }
                    balances[x.sender] -= 115 * x.price;
                    fees += 115 * x.price;
                    used += 115;
                    nonce[x.sender] += 1;
                    expected.push((
                        x.id.clone(),
                        if success { "Success" } else { "ProtocolRevert" }.to_string(),
                        Amount::from(115 * x.price),
                    ));
                }
                r.step().unwrap();
                let b = r.blocks.last().unwrap();
                let actual: Vec<_> = b
                    .receipts
                    .iter()
                    .map(|x| (x.tx_id.clone(), x.status.clone(), x.fee))
                    .collect();
                assert_eq!(
                    actual, expected,
                    "mode={mode} seed={seed_number} round={round}"
                );
                assert_eq!(b.used, used.into());
                for i in 0..2 {
                    assert_eq!(
                        r.state.balance(["alice", "bob"][i], "Q"),
                        balances[i].into()
                    );
                    assert_eq!(r.state.accounts[["alice", "bob"][i]].nonce, nonce[i].into());
                }
                assert_eq!(r.state.balance("builder", "Q"), fees.into());
                let actual_pending: BTreeSet<_> = r
                    .scheduler
                    .as_ref()
                    .unwrap()
                    .pending
                    .keys()
                    .cloned()
                    .collect();
                let expected_pending: BTreeSet<_> =
                    pending.keys().map(|i| cases[*i].id.clone()).collect();
                assert_eq!(
                    actual_pending, expected_pending,
                    "mode={mode} seed={seed_number} round={round}"
                );
            }
        }
    }
}

#[test]
fn producer_order_and_fee_ties_ignore_input_permutation_and_txid_text() {
    for mode in [Builder::Fifo, Builder::FeePriority] {
        let mut s = fixture("nonce");
        let c = s.scheduler.as_mut().unwrap();
        c.builder = mode;
        c.emissions = vec![
            emit(
                tx("a-lexical-first", 0, 0, 200, 1, 1, 100),
                10,
                0,
                "z-producer",
                0,
            ),
            emit(tx("z-winner", 0, 0, 200, 1, 2, 100), 10, 0, "a-producer", 1),
            emit(tx("m-second", 0, 0, 200, 1, 3, 100), 10, 0, "a-producer", 2),
        ];
        let mut a = Run::new(s.clone()).unwrap();
        a.finish().unwrap();
        s.scheduler.as_mut().unwrap().emissions.reverse();
        let mut b = Run::new(s).unwrap();
        b.finish().unwrap();
        assert_eq!(a.state, b.state);
        assert_eq!(a.blocks[0].receipts[0].tx_id, "z-winner");
        assert_eq!(a.blocks[0].receipts, b.blocks[0].receipts);
    }
}

#[test]
fn clamped_deliveries_use_event_id_before_original_due_time() {
    let mut s = fixture("nonce");
    let c = s.scheduler.as_mut().unwrap();
    c.rounds = 3;
    c.limits.events_per_round = 2;
    c.emissions = vec![
        emit(tx("earlier-generated", 0, 0, 200, 1, 1, 100), 1, 8, "a", 0),
        emit(tx("earlier-due", 0, 0, 200, 1, 2, 100), 2, 0, "b", 0),
    ];
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert!(r.blocks[0].receipts.is_empty());
    r.step().unwrap();
    assert_eq!(r.blocks[1].receipts[0].tx_id, "earlier-generated");
    let admitted: Vec<_> = r.blocks[1]
        .scheduler
        .as_ref()
        .unwrap()
        .trace
        .iter()
        .filter(|t| t.action == "Admitted")
        .map(|t| (t.tx_id.clone(), t.at, t.due_at))
        .collect();
    assert_eq!(
        admitted,
        vec![
            (Some("earlier-generated".into()), 10.into(), 9.into()),
            (Some("earlier-due".into()), 10.into(), 2.into())
        ]
    );
}

#[test]
fn rejected_builder_delivery_is_terminal_even_when_repeated_with_better_route() {
    let mut s = fixture("nonce");
    let c = s.scheduler.as_mut().unwrap();
    c.limits.pending = 1;
    c.builder = Builder::Fixed { priority: vec![] };
    c.limits.retention = 1;
    c.emissions = vec![
        emit(tx("fill", 0, 1, 200, 0, 1, 100), 1, 0, "p", 0),
        emit(tx("rejected", 0, 0, 200, 0, 1, 100), 2, 0, "p", 1),
        emit(tx("rejected", 0, 0, 200, 0, 1, 100), 21, 0, "p", 2),
    ];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(actions(&r, "Duplicate"), 1);
    assert!(
        matches!(&r.scheduler.as_ref().unwrap().known["rejected"].status,Status::Rejected{reason} if reason=="MempoolLimit")
    );
    assert_eq!(r.state.accounts["alice"].nonce, Amount::ZERO);
}

#[test]
fn private_sender_outbox_is_immediate_but_public_subscription_does_not_grant_private_delivery() {
    let mut s = fixture("private");
    let c = s.scheduler.as_mut().unwrap();
    c.emissions[0].delivery.builder_delay = 25.into();
    c.agents.get_mut("trader").unwrap().observe_public = true;
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let state = r.scheduler.as_ref().unwrap();
    let sender = state
        .observation(&r.scenario, &r.state, "recipient", "bob")
        .unwrap();
    assert_eq!(sender.own_outbox.len(), 1);
    assert!(matches!(sender.own_outbox[0].status, Status::InFlight));
    let stranger = state
        .observation(&r.scenario, &r.state, "trader", "alice")
        .unwrap();
    assert!(stranger.messages.is_empty());
    assert!(stranger.own_outbox.is_empty());
    assert!(r.blocks[0].decisions.is_empty());
}

#[test]
fn late_reaction_deadline_uses_actual_time_and_checkpoint_keeps_authorization_token() {
    let mut s = fixture("public");
    let c = s.scheduler.as_mut().unwrap();
    c.rounds = 5;
    c.limits.events_per_round = 1;
    c.limits.retention = 1;
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    r.step().unwrap();
    r.step().unwrap();
    let mut restored = Run::restore(&r.checkpoint().unwrap()).unwrap();
    r.step().unwrap();
    restored.step().unwrap();
    assert_eq!(r, restored);
    let d = &r.blocks[3].decisions[0];
    let intent = d.transaction.as_ref().unwrap();
    assert_eq!(intent.deadline, 60.into());
    assert_eq!(d.observed_block, 3.into());
}

#[test]
fn delayed_time_overflow_rolls_back_prior_event_processing() {
    let mut s = fixture("nonce");
    let c = s.scheduler.as_mut().unwrap();
    c.emissions = vec![
        emit(tx("earlier", 0, 0, 200, 1, 1, 100), 1, 0, "a", 0),
        emit(tx("overflow", 1, 0, 200, 1, 1, 100), 2, 0, "b", 0),
    ];
    c.emissions[1].delivery.builder_delay = Amount::MAX;
    let mut r = Run::new(s).unwrap();
    let before = r.clone();
    assert!(r.step().unwrap_err().contains("ModelError"));
    assert_eq!(r, before);
}

#[test]
fn all_required_scheduler_models_are_required_and_legacy_rejects_every_new_tag() {
    for tag in chainarium_core::scheduler::MODELS {
        let mut s = fixture("public");
        s.model_versions.retain(|m| m != tag);
        assert!(s.validate().is_err(), "{tag}");
    }
    let mut s = fixture("nonce");
    s.scheduler = None;
    s.model_versions
        .retain(|m| !chainarium_core::scheduler::MODELS.contains(&m.as_str()));
    s.model_versions.push("scheduler-m1-v1".into());
    assert!(s.validate().is_ok());
    for tag in chainarium_core::scheduler::MODELS {
        let mut t = s.clone();
        t.model_versions.push(tag.into());
        assert!(t.validate().is_err(), "{tag}");
    }
}

#[test]
fn resource_exhaustion_and_fee_to_self_preserve_exact_accounting() {
    for limit in 100..=120u64 {
        for fee_to_self in [false, true] {
            let mut s = fixture("fifo");
            if fee_to_self {
                s.chain.fee_account = "alice".into();
            }
            let c = s.scheduler.as_mut().unwrap();
            c.emissions = vec![
                emit(tx("first", 0, 0, limit, 2, 10, 100), 1, 0, "p", 0),
                emit(tx("conflict", 0, 0, 200, 1, 10, 100), 2, 0, "p", 1),
            ];
            let mut r = Run::new(s).unwrap();
            r.step().unwrap();
            let receipt = &r.blocks[0].receipts[0];
            assert_eq!(receipt.used, limit.min(115).into());
            assert_eq!(receipt.fee, (limit.min(115) * 2).into());
            assert_eq!(
                receipt.status,
                if limit < 115 {
                    "ResourceExhausted"
                } else {
                    "Success"
                }
            );
            assert_eq!(
                r.state.balance("alice", "Q"),
                (1000
                    - if limit < 115 { 0 } else { 10 }
                    - if fee_to_self { 0 } else { limit.min(115) * 2 })
                .into()
            );
            assert_eq!(r.state.accounts["alice"].nonce, 1.into());
            assert_eq!(r.blocks[0].receipts.len(), 1);
            assert!(
                matches!(&r.scheduler.as_ref().unwrap().known["conflict"].status,Status::Rejected{reason} if reason=="StaleNonce")
            );
        }
    }
}

#[test]
fn reactive_program_has_no_foreign_spending_authority() {
    let mut s = fixture("public");
    let p = s
        .scheduler
        .as_mut()
        .unwrap()
        .reactive
        .get_mut("trader")
        .unwrap();
    p.resource_price = 1.into();
    p.rules.insert(
        "victim".into(),
        vec![Call::TransferFrom {
            from: "bob".into(),
            to: "alice".into(),
            asset: "Q".into(),
            amount: 1.into(),
        }],
    );
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let b = &r.blocks[0];
    assert_eq!(b.receipts.len(), 2);
    let response = &b.receipts[1];
    assert_eq!(response.status, "ProtocolRevert");
    assert_eq!(r.state.balance("bob", "Q"), 800.into());
    assert_eq!(
        r.state.balance("alice", "Q"),
        Amount::from(10000).checked_sub(response.fee).unwrap()
    );
    assert_eq!(r.state.balance("builder", "Q"), response.fee);
    assert_eq!(r.state.accounts["alice"].nonce, 1.into());
}
