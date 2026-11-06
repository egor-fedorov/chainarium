use chainarium_core::{
    policy::{
        DecisionAction,
        agents::{Kind, Preflight, ScriptEvent},
    },
    *,
};

fn fixture(name: &str) -> Scenario {
    Scenario::parse(
        &std::fs::read(format!(
            "{}/../../scenarios/demos/sc-agent-{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap()
}
fn first(s: Scenario) -> Run {
    s.validate().unwrap();
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    r
}
fn decision(r: &Run, i: usize) -> &policy::agents::Decision {
    &r.blocks[i].scheduler.as_ref().unwrap().baselines[0]
}

// Regression: two worlds with identical allowed observation must not choose
// different submissions solely because another wallet crosses a threshold.
#[test]
fn other_wallet_assertion_cannot_gate_submission() {
    let mut low = fixture("random");
    let p = low.baseline_policies.get_mut("trader").unwrap();
    p.kind = Kind::ScriptedUser {
        schedule: vec![ScriptEvent {
            block: 1.into(),
            program: vec![
                Call::AssertBalance {
                    account: "bob".into(),
                    asset: "Q".into(),
                    minimum: 1.into(),
                },
                Call::Transfer {
                    to: "bob".into(),
                    asset: "Q".into(),
                    amount: 10.into(),
                },
            ],
        }],
        preflight: Preflight::SuccessfulOnly,
    };
    let mut high = low.clone();
    high.accounts
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("Q".into(), 1.into());
    match (low.validate(), high.validate()) {
        (Err(_), Err(_)) => return, // Rejecting forbidden reads is a valid fix.
        (Ok(()), Ok(())) => (),
        result => panic!("capability validity must not depend on hidden balance: {result:?}"),
    }
    let left = first(low);
    let right = first(high);
    let (l, r) = (decision(&left, 0), decision(&right, 0));
    assert_eq!(
        l.observation, r.observation,
        "the policy's allowed observation is identical"
    );
    eprintln!(
        "low: {:?} {:?}, balance alice={}, nonce={}; high: {:?} {:?}, balance alice={}, nonce={}",
        l.action,
        l.reason,
        left.state.balance("alice", "Q"),
        left.state.accounts["alice"].nonce,
        r.action,
        r.reason,
        right.state.balance("alice", "Q"),
        right.state.accounts["alice"].nonce
    );
    assert_eq!(
        l.action, r.action,
        "hidden wallet must not be a free preflight oracle"
    );
}

// Regression: a policy whose transaction was refused at submission must not
// report Submit / PreflightSuccess with no registry status or receipt link.
#[test]
fn known_cap_refusal_has_explicit_skip_decision() {
    let mut s = fixture("random");
    s.scheduler.as_mut().unwrap().limits.known = 1;
    if let Kind::RandomUser {
        probability_ppm,
        programs,
        ..
    } = &mut s.baseline_policies.get_mut("trader").unwrap().kind
    {
        *probability_ppm = 1_000_000;
        *programs = vec![vec![Call::Transfer {
            to: "bob".into(),
            asset: "Q".into(),
            amount: 10.into(),
        }]];
    }
    s.validate().unwrap();
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    r.step().unwrap();
    let d = decision(&r, 1);
    let report = r.blocks[1].scheduler.as_ref().unwrap();
    assert!(
        report
            .trace
            .iter()
            .any(|t| t.action == "KnownTransactionsLimit")
    );
    assert!(r.blocks[1].receipts.is_empty());
    assert!(!report.state.known.contains_key("policy:baseline:trader:2"));
    eprintln!(
        "decision={:?} {:?}; registry missing; no receipt; alice={}, bob={}, builder={}",
        d.action,
        d.reason,
        r.state.balance("alice", "Q"),
        r.state.balance("bob", "Q"),
        r.state.balance("builder", "Q")
    );
    assert_eq!(d.action, DecisionAction::Skip);
}

#[test]
fn rng_scalar_matrix_seed_name_template_and_probability() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("dec26-independent-vectors.json")).unwrap();
    for vector in vectors.as_array().unwrap() {
        for ppm in [0, 1, 500_000, 999_999, 1_000_000] {
            let mut s = fixture("random");
            s.seed = vector["seed"].as_str().unwrap().parse().unwrap();
            let name = vector["agent"].as_str().unwrap();
            let count = vector["count"].as_u64().unwrap();
            let agent = s.agents.remove("trader").unwrap();
            s.agents.insert(name.into(), agent);
            let cfg = s.scheduler.as_mut().unwrap();
            cfg.rounds = 8;
            let network = cfg.agents.remove("trader").unwrap();
            cfg.agents.insert(name.into(), network);
            let mut p = s.baseline_policies.remove("trader").unwrap();
            p.resource_price = 0.into();
            if let Kind::RandomUser {
                cadence,
                probability_ppm,
                programs,
                ..
            } = &mut p.kind
            {
                cadence.last_block = 8.into();
                *probability_ppm = ppm;
                *programs = (1..=count)
                    .map(|n| {
                        vec![Call::Transfer {
                            to: "bob".into(),
                            asset: "Q".into(),
                            amount: n.into(),
                        }]
                    })
                    .collect();
            }
            s.baseline_policies.insert(name.into(), p);
            s.validate().unwrap();
            let mut r = Run::new(s).unwrap();
            r.finish().unwrap();
            let mut sent = 0u64;
            let mut nonce = 0u64;
            for (i, row) in vector["rounds"].as_array().unwrap().iter().enumerate() {
                let d = decision(&r, i);
                let sel = d.selection.as_ref().unwrap();
                let gate: u64 = row["gate"].as_str().unwrap().parse().unwrap();
                let index: u64 = row["choice"].as_str().unwrap().parse().unwrap();
                assert_eq!(sel.gate.value, Some(gate.into()), "seed/name={vector}");
                assert_eq!(sel.template.as_ref().unwrap().value, Some(index.into()));
                assert_eq!(
                    sel.position_after.to_string(),
                    row["position"].as_str().unwrap()
                );
                if gate < u64::from(ppm) {
                    sent += index + 1;
                    nonce += 1;
                }
                assert_eq!(r.blocks[i].state.balance("bob", "Q"), sent.into());
                assert_eq!(r.blocks[i].state.accounts["alice"].nonce, nonce.into());
            }
            assert_eq!(r.state.balance("alice", "Q"), (1000 - sent).into());
            let before = r.checkpoint().unwrap();
            assert_eq!(r, Run::restore(&before).unwrap());
            assert_eq!(r.checkpoint().unwrap(), before);
        }
    }
}

#[test]
fn borrower_priority_cartesian_matrix_preserves_exact_unfunded_amount() {
    for target_debt in [0u64, 79, 80, 81, 90, 91, 100] {
        for target_collateral in [0u64, 5, 6, 7, 12] {
            let mut s = fixture("borrower");
            let p = s.baseline_policies.get_mut("bob").unwrap();
            p.resource_price = 0.into();
            if let Kind::BorrowerTarget {
                targets,
                max_debt_step,
                max_collateral_step,
                ..
            } = &mut p.kind
            {
                targets.truncate(1);
                targets.push(policy::agents::Target {
                    block: 3.into(),
                    debt: target_debt.into(),
                    collateral: target_collateral.into(),
                });
                *max_debt_step = 80.into();
                *max_collateral_step = 6.into();
            }
            s.validate().unwrap();
            let mut r = Run::new(s).unwrap();
            for _ in 0..3 {
                r.step().unwrap();
            }
            let d = decision(&r, 2);
            let own = d.observation.as_ref().unwrap().credit.as_ref().unwrap();
            assert_eq!((own.debt, own.collateral), (80.into(), 6.into()));
            let expected = if target_debt < 80 {
                Some(("Repay", 80 - target_debt))
            } else if target_collateral > 6 {
                Some(("DepositCollateral", (target_collateral - 6).min(6)))
            } else if target_debt > 80 {
                Some(("Borrow", target_debt - 80))
            } else if target_collateral < 6 {
                Some(("WithdrawCollateral", 6 - target_collateral))
            } else {
                None
            };
            match expected {
                None => {
                    assert!(d.program.is_empty());
                    assert_eq!(d.action, DecisionAction::Hold);
                }
                Some((label, amount)) => {
                    assert_eq!(d.program.len(), 1);
                    assert_eq!(d.program[0].label(), label);
                    let raw = serde_json::to_value(&d.program[0]).unwrap();
                    assert_eq!(raw["amount"], amount.to_string());
                }
            }
            let w = &r.state;
            let reserve = &w.lending["credit"];
            assert_eq!(
                w.balance("reserve", "Q").checked_add(reserve.debt).unwrap(),
                reserve.claims.checked_add(reserve.equity).unwrap()
            );
        }
    }
}

#[test]
fn concurrent_native_publishers_pay_only_actual_included_receipts() {
    let mut s = fixture("spot");
    s.accounts
        .insert("oracle2".into(), s.accounts["oracle"].clone());
    s.agents.insert(
        "oracle2".into(),
        Agent {
            controlled_accounts: ["oracle2".into()].into(),
        },
    );
    let network = s.scheduler.as_ref().unwrap().agents["oracle"].clone();
    s.scheduler
        .as_mut()
        .unwrap()
        .agents
        .insert("oracle2".into(), network);
    s.feeds
        .get_mut("spot")
        .unwrap()
        .publishers
        .insert("oracle2".into());
    let mut p = s.baseline_policies["oracle"].clone();
    p.account = "oracle2".into();
    s.baseline_policies.insert("oracle2".into(), p);
    let r = first(s);
    let ds = &r.blocks[0].scheduler.as_ref().unwrap().baselines;
    assert_eq!(ds.len(), 2);
    assert!(
        ds.iter()
            .all(|d| d.trial.as_ref().unwrap().status == "Success")
    );
    let receipts: Vec<_> = r.blocks[0]
        .receipts
        .iter()
        .filter(|r| r.tx_id.starts_with("policy:baseline:"))
        .collect();
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0].status, "Success");
    assert_eq!(receipts[1].status, "ProtocolRevert");
    assert!(receipts[1].error.as_ref().unwrap().contains("out of order"));
    for who in ["oracle", "oracle2"] {
        assert_eq!(r.state.accounts[who].nonce, 1.into());
        assert_eq!(r.state.balance(who, "Q"), 890.into());
    }
    assert_eq!(
        r.state.feeds["spot"].record.as_ref().unwrap().sequence,
        1.into()
    );
}

#[test]
fn own_wallet_assertion_remains_supported() {
    let mut s = fixture("random");
    let p = s.baseline_policies.get_mut("trader").unwrap();
    p.kind = Kind::ScriptedUser {
        schedule: vec![ScriptEvent {
            block: 1.into(),
            program: vec![
                Call::AssertBalance {
                    account: "alice".into(),
                    asset: "Q".into(),
                    minimum: 1.into(),
                },
                Call::Transfer {
                    to: "bob".into(),
                    asset: "Q".into(),
                    amount: 10.into(),
                },
            ],
        }],
        preflight: Preflight::SuccessfulOnly,
    };
    let r = first(s);
    assert_eq!(decision(&r, 0).action, DecisionAction::Submit);
    assert_eq!(r.state.balance("alice", "Q"), 865.into());
}

#[test]
fn all_roles_ignore_unobserved_private_and_future_transfers() {
    for name in [
        "random",
        "scripted",
        "preflight",
        "borrower",
        "spot",
        "twap",
    ] {
        let mut s = fixture(name);
        for (account, balance) in [("hidden_from", 100u64), ("hidden_to", 0)] {
            s.accounts.insert(
                account.into(),
                Account {
                    nonce: 0.into(),
                    balances: [("Q".into(), balance.into())].into(),
                },
            );
        }
        s.agents.insert(
            "hidden_agent".into(),
            Agent {
                controlled_accounts: ["hidden_from".into()].into(),
            },
        );
        s.scheduler.as_mut().unwrap().agents.insert(
            "hidden_agent".into(),
            scheduler::Network {
                observe_public: false,
                observation_delay: 0.into(),
                outgoing: scheduler::Delivery {
                    channel: scheduler::Channel::Private {
                        recipients: Default::default(),
                    },
                    builder_delay: 0.into(),
                },
            },
        );
        let mut compared = vec![];
        for amount in [1u64, 2] {
            let mut changed = s.clone();
            let cfg = changed.scheduler.as_mut().unwrap();
            for (id, at, nonce) in [("private", 0u64, 0u64), ("future", cfg.rounds as u64, 1u64)] {
                cfg.emissions.push(scheduler::Emission {
                    at: at.into(),
                    producer_id: id.into(),
                    producer_sequence: 0.into(),
                    delivery: scheduler::Delivery {
                        channel: scheduler::Channel::Private {
                            recipients: Default::default(),
                        },
                        builder_delay: 0.into(),
                    },
                    transaction: Transaction {
                        id: id.into(),
                        agent: "hidden_agent".into(),
                        sender: "hidden_from".into(),
                        chain: changed.chain.id.clone(),
                        nonce: nonce.into(),
                        limit: 200.into(),
                        price: 0.into(),
                        deadline: 100.into(),
                        program: vec![Call::Transfer {
                            to: "hidden_to".into(),
                            asset: "Q".into(),
                            amount: amount.into(),
                        }],
                    },
                });
            }
            let mut r = Run::new(changed).unwrap();
            r.finish().unwrap();
            compared.push(
                r.blocks
                    .iter()
                    .map(|b| b.scheduler.as_ref().unwrap().baselines.clone())
                    .collect::<Vec<_>>(),
            );
        }
        assert_eq!(compared[0], compared[1], "{name}");
    }
}

#[test]
fn rehashed_inflight_cursor_queue_and_rng_are_rejected() {
    let mut s = fixture("random");
    s.scheduler
        .as_mut()
        .unwrap()
        .agents
        .get_mut("trader")
        .unwrap()
        .outgoing
        .builder_delay = 10.into();
    let r = first(s);
    let cp: serde_json::Value = serde_json::from_slice(&r.checkpoint().unwrap()).unwrap();
    for field in ["cursor", "queue", "rng"] {
        let mut corrupt = cp.clone();
        match field {
            "cursor" => corrupt["run"]["scheduler"]["emission_cursor"] = serde_json::json!(1),
            "queue" => corrupt["run"]["scheduler"]["queue"][0]["due_at"] = serde_json::json!("11"),
            "rng" => corrupt["run"]["rng"]["positions"]["agent/trader"] = serde_json::json!("8"),
            _ => unreachable!(),
        }
        let changed: Run = serde_json::from_value(corrupt["run"].clone()).unwrap();
        corrupt["hash"] = serde_json::json!(run::digest(&serde_json::to_vec(&changed).unwrap()));
        assert!(
            Run::restore(&serde_json::to_vec(&corrupt).unwrap())
                .unwrap_err()
                .contains("replay mismatch"),
            "{field}"
        );
    }
}
