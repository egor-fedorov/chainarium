use chainarium_core::{
    amount::Price,
    credit::{FeedConfig, LendingConfig, Rate},
    engine,
    twap::*,
    *,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
fn a(n: u64) -> Amount {
    n.into()
}
fn ratio(n: u64, d: u64) -> Rational {
    Rational::new(n.into(), d.into()).unwrap()
}
fn account(x: u64, q: u64) -> Account {
    Account {
        nonce: Amount::ZERO,
        balances: BTreeMap::from([("X".into(), a(x)), ("Q".into(), a(q))]),
    }
}
fn scenario(window: u64) -> Scenario {
    let mut s = Scenario::parse(include_bytes!("../../../scenarios/demos/sc-01.json")).unwrap();
    s.blocks.clear();
    s.model_versions.extend(M2_MODELS.map(String::from));
    s.model_versions
        .extend(chainarium_core::credit::CREDIT_MODELS.map(String::from));
    s.model_versions.push(MODEL.into());
    s.accounts = BTreeMap::from([
        ("pool".into(), account(4, 400)),
        ("alice".into(), account(1000, 100000)),
        ("lp".into(), account(0, 0)),
        ("oracle".into(), account(0, 0)),
        ("builder".into(), account(0, 0)),
    ]);
    s.agents.clear();
    for id in ["alice", "lp", "oracle"] {
        s.agents.insert(
            id.into(),
            Agent {
                controlled_accounts: BTreeSet::from([id.into()]),
            },
        );
    }
    let p = s.protocols.get_mut("cpmm").unwrap();
    p.owner = "lp".into();
    p.fee_bps = 0;
    s.chain.interval = a(30);
    s.chain.block_limit = a(10000);
    s.price_sources.insert(
        "cpmm-price".into(),
        SourceConfig {
            pool: "cpmm".into(),
            base_asset: "X".into(),
            quote_asset: "Q".into(),
            history_capacity: 128,
        },
    );
    for (id, kind) in [
        (
            "spot",
            FeedKind::NativeSpot {
                source: "cpmm-price".into(),
            },
        ),
        (
            "twap",
            FeedKind::NativeTwap {
                source: "cpmm-price".into(),
                window: a(window),
            },
        ),
    ] {
        s.feeds.insert(
            id.into(),
            FeedConfig {
                kind,
                base_asset: "X".into(),
                quote_asset: "Q".into(),
                source_id: "cpmm-price".into(),
                publishers: BTreeSet::from(["oracle".into()]),
                initial: None,
            },
        );
    }
    s
}
fn tx(id: &str, who: &str, n: u64, calls: Vec<Call>) -> Transaction {
    Transaction {
        id: id.into(),
        agent: who.into(),
        sender: who.into(),
        chain: "lab-1".into(),
        nonce: a(n),
        limit: a(1000),
        price: Amount::ZERO,
        deadline: Amount::MAX,
        program: calls,
    }
}
fn swap(asset: &str, n: u64) -> Call {
    Call::SwapExactIn {
        pool: "cpmm".into(),
        asset_in: asset.into(),
        amount: a(n),
        min_out: a(1),
    }
}
fn publish(end: u64, seq: u64) -> Call {
    Call::PublishTwap {
        feed: "twap".into(),
        end: a(end),
        sequence: a(seq),
        update_id: format!("twap-{seq}"),
    }
}
fn spot(seq: u64) -> Call {
    Call::PublishSpot {
        feed: "spot".into(),
        sequence: a(seq),
        update_id: format!("spot-{seq}"),
    }
}
fn fail() -> Call {
    Call::AssertCondition {
        condition: false,
        message: "after price update".into(),
    }
}
fn first() -> Run {
    let mut s = scenario(60);
    s.blocks = vec![vec![tx("price-to-200", "alice", 0, vec![swap("Q", 200)])]];
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    r
}
fn average(h: &History, end: u64, window: u64) -> Result<Rational, String> {
    h.average(a(end), a(window), &mut || Ok::<(), String>(()), &mut vec![])
}
#[test]
fn vf_twap_exact_150_175_boundaries_and_readonly() {
    let mut r = first();
    let before = r.state.clone();
    assert!(
        average(&r.state.price_history["cpmm-price"], 30, 60)
            .unwrap_err()
            .starts_with("NotReady")
    );
    r.scenario
        .blocks
        .push(vec![tx("twap150", "oracle", 0, vec![publish(60, 1)])]);
    r.step().unwrap();
    let h = &r.state.price_history["cpmm-price"];
    assert_eq!(h.integral, ratio(9000, 1));
    assert_eq!(average(h, 60, 60).unwrap(), ratio(150, 1));
    assert_eq!(average(h, 45, 20).unwrap(), ratio(175, 1));
    assert_eq!(
        r.state.feeds["twap"]
            .record
            .as_ref()
            .unwrap()
            .price
            .numerator(),
        a(150)
    );
    assert_eq!(r.blocks[1].receipts[0].used, a(112));
    assert_eq!(
        r.blocks[1].receipts[0].trace[0]
            .twap
            .as_ref()
            .unwrap()
            .segments
            .len(),
        2
    );
    assert!(average(h, 61, 60).unwrap_err().contains("future"));
    let state = r.state.clone();
    for _ in 0..5 {
        average(&r.state.price_history["cpmm-price"], 60, 60).unwrap();
    }
    assert_eq!(state, r.state);
    assert_eq!(before.timestamp, a(30));
    let mut s = scenario(120);
    s.blocks = vec![
        vec![tx("price", "alice", 0, vec![swap("Q", 200)])],
        vec![],
        vec![],
        vec![tx("twap175", "oracle", 0, vec![publish(120, 1)])],
    ];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(
        r.state.price_history["cpmm-price"].integral,
        ratio(21000, 1)
    );
    assert_eq!(
        r.state.feeds["twap"]
            .record
            .as_ref()
            .unwrap()
            .price
            .numerator(),
        a(175)
    );
}
#[test]
fn vf_twap_same_timestamp_and_nested_rollback() {
    let mut s = scenario(60);
    s.blocks = vec![
        vec![tx(
            "two-prices",
            "alice",
            0,
            vec![swap("Q", 200), swap("X", 1)],
        )],
        vec![],
    ];
    let mut r = Run::new(s.clone()).unwrap();
    r.finish().unwrap();
    let h = &r.state.price_history["cpmm-price"];
    assert_eq!(h.segments.len(), 2);
    assert_eq!(h.segments[1].price, Some(ratio(225, 2)));
    assert_eq!(average(h, 60, 60).unwrap(), ratio(425, 4));
    assert_eq!(
        r.blocks[0].receipts[0].trace[0].source_updates[0].price,
        Some(ratio(200, 1))
    );
    s.blocks[0][0].program = vec![Call::Sequence {
        calls: vec![swap("Q", 200), swap("X", 1), fail()],
    }];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(r.blocks[0].receipts[0].status, "ProtocolRevert");
    assert_eq!(r.state.price_history["cpmm-price"].segments.len(), 1);
    assert_eq!(
        average(&r.state.price_history["cpmm-price"], 60, 60).unwrap(),
        ratio(100, 1)
    );
    assert!(!r.blocks[0].receipts[0].trace[1].source_updates.is_empty());
}
#[test]
fn vf_twap_gap_exit_bootstrap_and_equivalent_price() {
    let mut s = scenario(60);
    let supply = World::genesis(&s).unwrap().shares["cpmm"];
    s.blocks = vec![
        vec![tx(
            "exit",
            "lp",
            0,
            vec![Call::RemoveLiquidity {
                pool: "cpmm".into(),
                shares: supply,
                min_x: a(4),
                min_y: a(400),
            }],
        )],
        vec![tx(
            "restart",
            "lp",
            1,
            vec![Call::AddLiquidity {
                pool: "cpmm".into(),
                max_x: a(4),
                max_y: a(400),
                min_shares: supply,
            }],
        )],
        vec![tx("gap", "oracle", 0, vec![publish(90, 1)])],
        vec![tx("ready", "oracle", 1, vec![publish(120, 2)])],
    ];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(
        r.blocks[2].receipts[0].error.as_deref(),
        Some("NotReady: price gap")
    );
    assert_eq!(r.blocks[3].receipts[0].status, "Success");
    assert_eq!(
        r.state.feeds["twap"]
            .record
            .as_ref()
            .unwrap()
            .price
            .numerator(),
        a(100)
    );
    let mut s = scenario(60);
    s.blocks = vec![
        vec![tx(
            "same-price",
            "alice",
            0,
            vec![Call::AddLiquidity {
                pool: "cpmm".into(),
                max_x: a(4),
                max_y: a(400),
                min_shares: a(40),
            }],
        )],
        vec![],
    ];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(r.state.price_history["cpmm-price"].segments.len(), 1);
    assert_eq!(r.state.price_history["cpmm-price"].integral, ratio(6000, 1));
}
#[test]
fn vf_twap_history_128_segment_eviction_and_rebase() {
    let mut s = scenario(127);
    s.chain.interval = a(1);
    s.accounts.insert("pool".into(), account(1000, 100000));
    for i in 0..150 {
        s.blocks.push(vec![tx(
            &format!("change-{i}"),
            "alice",
            i,
            vec![if i % 2 == 0 {
                swap("Q", 200)
            } else {
                swap("X", 2)
            }],
        )]);
    }
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert!(r.blocks.iter().all(|b| b.receipts[0].status == "Success"));
    let h = &r.state.price_history["cpmm-price"];
    assert_eq!(h.segments.len(), 128);
    assert_eq!(h.anchor, a(23));
    assert!(average(h, 150, 128).unwrap_err().contains("anchor"));
    let avg = average(h, 150, 127).unwrap();
    assert_eq!(avg.mul(a(127)).unwrap(), h.integral);
    r.state.invariant(&r.scenario).unwrap();
    let restored = Run::restore(&r.checkpoint().unwrap()).unwrap();
    assert_eq!(restored, r);
}
#[test]
fn vf_twap_floor_only_once_exact_wire_and_output_range() {
    let h = History {
        anchor: a(0),
        last_time: a(2),
        integral: ratio(1, 1),
        segments: VecDeque::from([
            Segment {
                start: a(0),
                price: Some(ratio(1, 3)),
            },
            Segment {
                start: a(1),
                price: Some(ratio(2, 3)),
            },
        ]),
    };
    let avg = average(&h, 2, 2).unwrap();
    assert_eq!(avg, ratio(1, 2));
    let p = avg.published_price("X", "Q").unwrap();
    assert_eq!((p.numerator(), p.denominator()), (a(1), a(2)));
    let third = ratio(1, 3).published_price("X", "Q").unwrap();
    assert_eq!(
        (third.numerator(), third.denominator()),
        (a(333333333333333333), a(1000000000000000000))
    );
    assert!(
        Rational::new(1u8.into(), Amount::MAX.big())
            .unwrap()
            .published_price("X", "Q")
            .is_err()
    );
    let max = Rational::new(Amount::MAX.big(), 1u8.into())
        .unwrap()
        .published_price("X", "Q")
        .unwrap();
    assert_eq!(max.numerator(), Amount::MAX);
    let bytes = serde_json::to_vec(&h).unwrap();
    assert_eq!(serde_json::from_slice::<History>(&bytes).unwrap(), h);
    for json in [
        r#"{"numerator":"01","denominator":"2"}"#,
        r#"{"numerator":"2","denominator":"4"}"#,
        r#"{"numerator":1,"denominator":"2"}"#,
        r#"{"numerator":"1","denominator":"0"}"#,
    ] {
        assert!(serde_json::from_str::<Rational>(json).is_err());
    }
    assert!(Rational::new(num_bigint::BigUint::from(1u8) << 131072usize, 1u8.into()).is_err());
}
#[test]
fn vf_twap_auth_kind_future_old_window_and_published_time() {
    let r = first();
    let fake = Call::PublishOracle {
        feed: "twap".into(),
        source_id: "cpmm-price".into(),
        price: Price::new("X".into(), "Q".into(), a(999), a(1)).unwrap(),
        observed_at: a(60),
        sequence: a(1),
        update_id: "fake".into(),
    };
    let b = engine::build_block(
        &r.scenario,
        &r.state,
        &[
            tx("wrong-user", "alice", 1, vec![publish(60, 1)]),
            tx("fake", "oracle", 0, vec![fake]),
            tx("future", "oracle", 1, vec![publish(61, 1)]),
            tx("ready", "oracle", 2, vec![publish(60, 1), spot(1)]),
        ],
    )
    .unwrap();
    assert_eq!(
        b.receipts
            .iter()
            .map(|x| x.status.as_str())
            .collect::<Vec<_>>(),
        [
            "ProtocolRevert",
            "ProtocolRevert",
            "ProtocolRevert",
            "Success"
        ]
    );
    let mut s = r.scenario.clone();
    s.chain.interval = a(61);
    let old = engine::build_block(
        &s,
        &b.state,
        &[tx("old-window", "oracle", 3, vec![publish(60, 2)])],
    )
    .unwrap();
    let record = old.state.feeds["twap"].record.as_ref().unwrap();
    assert_eq!((record.observed_at, record.published_at), (a(60), a(121)));
    assert_eq!(record.price.numerator(), a(150));
    assert_eq!(
        old.state.timestamp.checked_sub(record.observed_at).unwrap(),
        a(61)
    );
    let duplicate = engine::build_block(
        &s,
        &old.state,
        &[tx("repeat", "oracle", 4, vec![publish(60, 2)])],
    )
    .unwrap();
    assert_eq!(
        duplicate.receipts[0].error.as_deref(),
        Some("oracle update out of order")
    );
}
#[test]
fn vf_twap_publish_exhaustion_late_failure_and_checkpoint() {
    let mut r = first();
    let mut limited = tx("exhausted", "oracle", 0, vec![publish(60, 1)]);
    limited.limit = a(111);
    r.scenario.blocks.push(vec![
        limited,
        tx("late", "oracle", 1, vec![publish(60, 1), fail()]),
        tx("ready", "oracle", 2, vec![publish(60, 1)]),
    ]);
    let mut restored = Run::restore(&r.checkpoint().unwrap()).unwrap();
    r.finish().unwrap();
    restored.finish().unwrap();
    assert_eq!(r, restored);
    let receipts = &r.blocks[1].receipts;
    assert_eq!(receipts[0].status, "ResourceExhausted");
    assert_eq!(receipts[0].used, a(111));
    assert_eq!(
        receipts[0].trace[0].twap.as_ref().unwrap().segments.len(),
        1
    );
    assert_eq!(receipts[1].status, "ProtocolRevert");
    assert!(receipts[1].events.is_empty());
    assert_eq!(receipts[2].status, "Success");
    assert_eq!(r.state.feeds["twap"].seen_updates.len(), 1);
    assert_eq!(r.state.price_history["cpmm-price"].integral, ratio(9000, 1));
}
#[test]
fn vf_twap_config_validation_and_stale_lending_consumer() {
    let mut s = scenario(60);
    s.feeds.get_mut("twap").unwrap().kind = FeedKind::NativeTwap {
        source: "cpmm-price".into(),
        window: a(0),
    };
    assert!(s.validate().is_err());
    let mut s = scenario(60);
    s.price_sources
        .get_mut("cpmm-price")
        .unwrap()
        .history_capacity = 129;
    assert!(s.validate().is_err());
    let mut s = scenario(60);
    s.accounts.insert("bob".into(), account(1, 0));
    s.accounts.insert("reserve".into(), account(0, 0));
    s.accounts.insert("escrow".into(), account(0, 0));
    s.agents.insert(
        "bob".into(),
        Agent {
            controlled_accounts: BTreeSet::from(["bob".into()]),
        },
    );
    s.lending.insert(
        "credit".into(),
        LendingConfig {
            cash_account: "reserve".into(),
            debt_asset: "Q".into(),
            collateral_asset: "X".into(),
            feed: "twap".into(),
            ltv_bps: 7500,
            lt_bps: 8000,
            close_bps: 10000,
            bonus_bps: 1000,
            reserve_factor: a(0),
            supply_cap: a(10000),
            borrow_cap: a(1000),
            max_age: a(60),
            rate: Rate::Constant { rate: a(0) },
            positions: BTreeMap::from([("bob".into(), "escrow".into())]),
        },
    );
    s.blocks = vec![
        vec![
            tx(
                "price",
                "alice",
                0,
                vec![
                    swap("Q", 200),
                    Call::Supply {
                        market: "credit".into(),
                        amount: a(500),
                        min_shares: a(500),
                    },
                ],
            ),
            tx(
                "deposit",
                "bob",
                0,
                vec![Call::DepositCollateral {
                    market: "credit".into(),
                    amount: a(1),
                }],
            ),
            tx(
                "absent",
                "bob",
                1,
                vec![Call::Borrow {
                    market: "credit".into(),
                    amount: a(100),
                }],
            ),
        ],
        vec![
            tx("publish", "oracle", 0, vec![publish(60, 1)]),
            tx(
                "borrow",
                "bob",
                2,
                vec![Call::Borrow {
                    market: "credit".into(),
                    amount: a(100),
                }],
            ),
        ],
    ];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(
        r.blocks[0].receipts[2].error.as_deref(),
        Some("oracle unavailable")
    );
    assert_eq!(r.blocks[1].receipts[1].status, "Success");
    let used = r.blocks[1].receipts[1].trace[0].oracle.as_ref().unwrap();
    assert!(matches!(used.kind, FeedKind::NativeTwap { .. }));
    assert_eq!(used.record.as_ref().unwrap().price.numerator(), a(150));
    let mut s = r.scenario.clone();
    s.chain.interval = a(61);
    let b = engine::build_block(
        &s,
        &r.state,
        &[
            tx("republish", "oracle", 1, vec![publish(60, 2)]),
            tx(
                "stale",
                "bob",
                3,
                vec![Call::Borrow {
                    market: "credit".into(),
                    amount: a(1),
                }],
            ),
            tx(
                "safe",
                "bob",
                4,
                vec![Call::Repay {
                    market: "credit".into(),
                    borrower: "bob".into(),
                    amount: a(1),
                }],
            ),
        ],
    )
    .unwrap();
    assert_eq!(b.receipts[1].error.as_deref(), Some("oracle stale"));
    assert_eq!(b.receipts[2].status, "Success");
}
#[test]
fn vf_twap_price_range_failure_preserves_last_real_publication() {
    let mut s = scenario(1);
    s.chain.interval = a(1);
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("X".into(), Amount::MAX.checked_sub(a(4)).unwrap());
    let shares = World::genesis(&s).unwrap().shares["cpmm"];
    s.blocks = vec![
        vec![
            tx("publish-100", "oracle", 0, vec![publish(1, 1)]),
            tx(
                "fund-lp",
                "alice",
                0,
                vec![Call::Transfer {
                    to: "lp".into(),
                    asset: "X".into(),
                    amount: Amount::MAX.checked_sub(a(4)).unwrap(),
                }],
            ),
            tx(
                "tiny-price",
                "lp",
                0,
                vec![
                    Call::RemoveLiquidity {
                        pool: "cpmm".into(),
                        shares,
                        min_x: a(4),
                        min_y: a(400),
                    },
                    Call::AddLiquidity {
                        pool: "cpmm".into(),
                        max_x: Amount::MAX,
                        max_y: a(1),
                        min_shares: a(1),
                    },
                ],
            ),
        ],
        vec![tx("cannot-publish", "oracle", 1, vec![publish(2, 2)])],
    ];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(r.blocks[0].receipts[2].status, "Success");
    assert_eq!(
        r.blocks[1].receipts[0].error.as_deref(),
        Some("PriceOutOfRange")
    );
    let record = r.state.feeds["twap"].record.as_ref().unwrap();
    assert_eq!(record.sequence, a(1));
    assert_eq!(record.observed_at, a(1));
    assert_eq!(record.price.numerator(), a(100));
    assert!(!r.state.feeds["twap"].seen_updates.contains("twap-2"));
    assert!(
        r.blocks[1].receipts[0].trace[0]
            .twap
            .as_ref()
            .unwrap()
            .exact_average
            .is_some()
    );
}
#[test]
fn vf_twap_windows_match_independent_per_second_sum() {
    let mut history = History::new(Some(ratio(1, 1)));
    history.segments.clear();
    let mut per_second = vec![];
    let mut random = 83u64;
    for _ in 0..60 {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        let price = 1 + random % 500;
        let duration = 1 + (random >> 20) % 15;
        history.segments.push_back(Segment {
            start: a(per_second.len() as u64),
            price: Some(ratio(price, 1)),
        });
        per_second.extend(std::iter::repeat_n(price, duration as usize));
    }
    history.last_time = a(per_second.len() as u64);
    history.integral = ratio(per_second.iter().sum(), 1);
    let before = history.clone();
    for end in (1..per_second.len()).step_by(7) {
        for window in [1, end / 2 + 1, end] {
            let expected = ratio(per_second[end - window..end].iter().sum(), window as u64);
            assert_eq!(
                average(&history, end as u64, window as u64).unwrap(),
                expected
            );
        }
    }
    assert_eq!(history, before);
}
