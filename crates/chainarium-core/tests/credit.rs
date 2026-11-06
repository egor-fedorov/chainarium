use chainarium_core::{amount::Price, credit::*, engine, *};
use std::collections::{BTreeMap, BTreeSet};
fn amount(n: u64) -> Amount {
    n.into()
}
fn account(x: u64, q: u64) -> Account {
    Account {
        nonce: Amount::ZERO,
        balances: BTreeMap::from([("X".into(), x.into()), ("Q".into(), q.into())]),
    }
}
fn scenario() -> Scenario {
    let mut s = Scenario::parse(include_bytes!("../../../scenarios/demos/sc-01.json")).unwrap();
    s.blocks.clear();
    s.protocols.clear();
    s.accounts.clear();
    s.agents.clear();
    s.model_versions.extend(CREDIT_MODELS.map(String::from));
    for (id, x, q) in [
        ("alice", 0, 100),
        ("bob", 6, 0),
        ("carol", 0, 1000),
        ("oracle", 0, 0),
        ("builder", 0, 0),
        ("reserve", 0, 0),
        ("escrow-bob", 0, 0),
    ] {
        s.accounts.insert(id.into(), account(x, q));
    }
    for id in ["alice", "bob", "carol", "oracle"] {
        s.agents.insert(
            id.into(),
            Agent {
                controlled_accounts: BTreeSet::from([id.into()]),
            },
        );
    }
    s.chain.interval = 1.into();
    s.feeds.insert(
        "feed".into(),
        FeedConfig {
            kind: chainarium_core::twap::FeedKind::External,
            base_asset: "X".into(),
            quote_asset: "Q".into(),
            source_id: "reference".into(),
            publishers: BTreeSet::from(["oracle".into()]),
            initial: Some(OracleRecord {
                price: Price::new("X".into(), "Q".into(), 20.into(), 1.into()).unwrap(),
                source_id: "reference".into(),
                observed_at: Amount::ZERO,
                published_at: Amount::ZERO,
                sequence: Amount::ZERO,
                update_id: "genesis".into(),
            }),
        },
    );
    s.lending.insert(
        "credit".into(),
        LendingConfig {
            cash_account: "reserve".into(),
            debt_asset: "Q".into(),
            collateral_asset: "X".into(),
            feed: "feed".into(),
            ltv_bps: 7500,
            lt_bps: 8000,
            close_bps: 10000,
            bonus_bps: 1000,
            reserve_factor: "200000000000000000".parse().unwrap(),
            supply_cap: 1_000_000.into(),
            borrow_cap: 10000.into(),
            max_age: 60.into(),
            rate: Rate::Constant {
                rate: "125000000000000000".parse().unwrap(),
            },
            positions: BTreeMap::from([("bob".into(), "escrow-bob".into())]),
        },
    );
    s
}
fn tx(id: &str, who: &str, n: u64, calls: Vec<Call>) -> Transaction {
    Transaction {
        id: id.into(),
        agent: who.into(),
        sender: who.into(),
        chain: "lab-1".into(),
        nonce: n.into(),
        limit: 1000.into(),
        price: Amount::ZERO,
        deadline: Amount::MAX,
        program: calls,
    }
}
fn supply(a: u64) -> Call {
    Call::Supply {
        market: "credit".into(),
        amount: a.into(),
        min_shares: 1.into(),
    }
}
fn collateral(a: u64) -> Call {
    Call::DepositCollateral {
        market: "credit".into(),
        amount: a.into(),
    }
}
fn borrow(a: u64) -> Call {
    Call::Borrow {
        market: "credit".into(),
        amount: a.into(),
    }
}
fn repay(a: u64) -> Call {
    Call::Repay {
        market: "credit".into(),
        borrower: "bob".into(),
        amount: a.into(),
    }
}
fn withdraw(shares: u64) -> Call {
    Call::Withdraw {
        market: "credit".into(),
        shares: shares.into(),
        min_assets: Amount::ZERO,
    }
}
fn publish(price: u64, time: u64, seq: u64) -> Call {
    Call::PublishOracle {
        feed: "feed".into(),
        source_id: "reference".into(),
        price: Price::new("X".into(), "Q".into(), price.into(), 1.into()).unwrap(),
        observed_at: time.into(),
        sequence: seq.into(),
        update_id: format!("update-{seq}"),
    }
}
fn liquidation() -> Call {
    Call::Liquidate {
        market: "credit".into(),
        borrower: "bob".into(),
        amount: 1000.into(),
        min_collateral: 6.into(),
    }
}
fn fail() -> Call {
    Call::AssertCondition {
        condition: false,
        message: "after credit transition".into(),
    }
}
fn funded() -> Run {
    let mut s = scenario();
    s.blocks = vec![vec![
        tx("supply", "alice", 0, vec![supply(100)]),
        tx("open", "bob", 0, vec![collateral(6), borrow(80)]),
    ]];
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    r
}
fn tuple(s: &Scenario, w: &World) -> (Amount, Amount, Amount, Amount) {
    let l = &w.lending["credit"];
    (
        w.balance(&s.lending["credit"].cash_account, "Q"),
        l.debt,
        l.claims,
        l.equity,
    )
}
#[test]
fn post_loss_dust_withdrawal_does_not_burn_shares_for_zero_assets() {
    let s = Scenario::parse(include_bytes!("../../../scenarios/demos/sc-05-credit.json")).unwrap();
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    r.step().unwrap();
    assert_eq!(
        (
            r.state.lending["credit"].claims,
            r.state.lending["credit"].total_shares
        ),
        (60.into(), 75.into())
    );
    let mut request = tx(
        "dust-withdrawal",
        "alice",
        0,
        vec![Call::Withdraw {
            market: "credit".into(),
            shares: 1.into(),
            min_assets: Amount::ZERO,
        }],
    );
    request.nonce = r.state.accounts["alice"].nonce;
    let b = engine::build_block(&r.scenario, &r.state, &[request]).unwrap();
    assert_eq!(b.receipts[0].status, "ProtocolRevert");
    assert_eq!(
        b.receipts[0].error.as_deref(),
        Some("zero withdrawal assets")
    );
    let control = engine::build_block(&r.scenario, &r.state, &[]).unwrap();
    assert_eq!(b.state.lending, control.state.lending);
    assert_eq!(b.state.balance("alice", "Q"), r.state.balance("alice", "Q"));
    assert!(b.receipts[0].events.is_empty());
}

#[test]
fn vf_lend_account_interest_loss_and_redemption_exact() {
    let mut r = funded();
    assert_eq!(
        tuple(&r.scenario, &r.state),
        (20.into(), 80.into(), 100.into(), 0.into())
    );
    assert_eq!(r.state.balance("escrow-bob", "X"), 6.into());
    let accrued = engine::build_block(&r.scenario, &r.state, &[]).unwrap();
    assert_eq!(
        tuple(&r.scenario, &accrued.state),
        (20.into(), 90.into(), 108.into(), 2.into())
    );
    assert_eq!(accrued.system_events[0].amount, 10.into());
    let candidate = vec![
        tx("price", "oracle", 0, vec![publish(11, 2, 1)]),
        tx("liquidate", "carol", 0, vec![liquidation()]),
        tx("redeem", "alice", 1, vec![withdraw(25)]),
    ];
    let liquidated = engine::build_block(&r.scenario, &r.state, &candidate[..2]).unwrap();
    assert_eq!(
        tuple(&r.scenario, &liquidated.state),
        (80.into(), 0.into(), 80.into(), 0.into())
    );
    let loss = &liquidated.state.lending["credit"].losses[0];
    assert_eq!(
        (loss.amount, loss.equity_loss, loss.depositor_loss),
        (30.into(), 2.into(), 28.into())
    );
    r.scenario.blocks.push(candidate);
    r.step().unwrap();
    assert!(r.blocks[1].receipts.iter().all(|x| x.status == "Success"));
    assert_eq!(
        tuple(&r.scenario, &r.state),
        (60.into(), 0.into(), 60.into(), 0.into())
    );
    assert_eq!(r.state.lending["credit"].total_shares, 75.into());
    assert_eq!(r.state.balance("alice", "Q"), 20.into());
    assert_eq!(r.state.balance("carol", "X"), 6.into());
    r.state.invariant(&r.scenario).unwrap();
}
#[test]
fn vf_lend_late_liquidation_failure_restores_loss_escrow_claims() {
    let r = funded();
    let mut failed = tx("failed", "carol", 0, vec![liquidation(), fail()]);
    failed.price = 1.into();
    failed.limit = 200.into();
    let b = engine::build_block(
        &r.scenario,
        &r.state,
        &[tx("price", "oracle", 0, vec![publish(11, 2, 1)]), failed],
    )
    .unwrap();
    assert_eq!(b.receipts[1].status, "ProtocolRevert");
    assert_eq!(
        tuple(&r.scenario, &b.state),
        (20.into(), 90.into(), 108.into(), 2.into())
    );
    assert!(b.state.lending["credit"].losses.is_empty());
    assert_eq!(b.state.balance("escrow-bob", "X"), 6.into());
    assert_eq!(b.state.balance("carol", "Q"), 870.into());
    assert_eq!(b.state.balance("builder", "Q"), 130.into());
    assert_eq!(
        (b.receipts[1].used, b.receipts[1].fee, b.receipts[1].refund),
        (130.into(), 130.into(), 70.into())
    );
    assert!(b.receipts[1].events.is_empty());
}
#[test]
fn vf_oracle_auth_freshness_sequence_and_publication_clock() {
    let r = funded();
    for call in [publish(11, 3, 1), publish(11, 1, 0)] {
        let b = engine::build_block(&r.scenario, &r.state, &[tx("bad", "oracle", 0, vec![call])])
            .unwrap();
        assert_eq!(b.receipts[0].status, "ProtocolRevert");
    }
    let b = engine::build_block(
        &r.scenario,
        &r.state,
        &[tx("bad", "bob", 1, vec![publish(11, 2, 1)])],
    )
    .unwrap();
    assert_eq!(
        b.receipts[0].error.as_deref(),
        Some("unauthorized oracle publisher")
    );
    let b = engine::build_block(
        &r.scenario,
        &r.state,
        &[tx("good", "oracle", 0, vec![publish(11, 0, 1)])],
    )
    .unwrap();
    let rec = b.state.feeds["feed"].record.as_ref().unwrap();
    assert_eq!(rec.published_at, 2.into());
    assert_eq!(rec.observed_at, 0.into());
    let mut s = r.scenario.clone();
    s.chain.interval = 59.into();
    s.lending.get_mut("credit").unwrap().rate = Rate::Constant { rate: Amount::ZERO };
    let age60 =
        engine::build_block(&s, &r.state, &[tx("borrow", "bob", 1, vec![borrow(1)])]).unwrap();
    assert_eq!(age60.receipts[0].status, "Success");
    s.chain.interval = 60.into();
    let age61 =
        engine::build_block(&s, &r.state, &[tx("borrow", "bob", 1, vec![borrow(1)])]).unwrap();
    assert_eq!(age61.receipts[0].error.as_deref(), Some("oracle stale"));
    let safe =
        engine::build_block(&s, &r.state, &[tx("repay", "carol", 0, vec![repay(1)])]).unwrap();
    assert_eq!(safe.receipts[0].status, "Success");
}
#[test]
fn vf_lend_caps_ltv_cash_and_ownership() {
    let mut s = scenario();
    s.blocks = vec![vec![
        tx("supply", "alice", 0, vec![supply(100)]),
        tx("bad", "bob", 0, vec![collateral(6), borrow(91)]),
    ]];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(
        r.blocks[0].receipts[1].error.as_deref(),
        Some("borrow violates LTV")
    );
    assert_eq!(r.state.balance("escrow-bob", "X"), Amount::ZERO);
    let r = funded();
    let b = engine::build_block(
        &r.scenario,
        &r.state,
        &[tx("redeem", "alice", 1, vec![withdraw(50)])],
    )
    .unwrap();
    assert_eq!(
        b.receipts[0].error.as_deref(),
        Some("insufficient reserve cash")
    );
    assert_eq!(b.state.lending["credit"].total_shares, 100.into());
    let b = engine::build_block(
        &r.scenario,
        &r.state,
        &[tx("steal", "carol", 0, vec![withdraw(1)])],
    )
    .unwrap();
    assert_eq!(b.receipts[0].status, "ProtocolRevert");
    let mut s = scenario();
    s.lending.get_mut("credit").unwrap().supply_cap = 99.into();
    s.blocks = vec![vec![tx("cap", "alice", 0, vec![supply(100)])]];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(r.blocks[0].receipts[0].error.as_deref(), Some("supply cap"));
}
#[test]
fn vf_repay_clamps_to_debt_and_clears_fraction() {
    let r = funded();
    let b = engine::build_block(
        &r.scenario,
        &r.state,
        &[tx("repay", "carol", 0, vec![repay(1000)])],
    )
    .unwrap();
    assert_eq!(b.state.balance("carol", "Q"), 910.into());
    assert_eq!(
        tuple(&r.scenario, &b.state),
        (110.into(), 0.into(), 108.into(), 2.into())
    );
    let mut s = scenario();
    s.lending.get_mut("credit").unwrap().rate = Rate::Constant {
        rate: "100000000000000000".parse().unwrap(),
    };
    s.blocks = vec![
        vec![
            tx("supply", "alice", 0, vec![supply(100)]),
            tx("open", "bob", 0, vec![collateral(6), borrow(3)]),
        ],
        vec![],
        vec![],
        vec![tx("close", "carol", 0, vec![repay(100)])],
    ];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(
        r.blocks[1].state.lending["credit"].positions["bob"].remainder,
        "300000000000000000".parse().unwrap()
    );
    assert_eq!(
        r.blocks[2].state.lending["credit"].positions["bob"].remainder,
        "600000000000000000".parse().unwrap()
    );
    assert_eq!(
        r.state.lending["credit"].positions["bob"].remainder,
        Amount::ZERO
    );
    assert!(
        r.blocks[3].receipts[0]
            .events
            .iter()
            .any(|e| e.kind == "InterestRemainderCleared")
    );
}
#[test]
fn vf_kink_rate_analytical_points() {
    let rate = Rate::Kink {
        base: Amount::ZERO,
        slope_low: "10000000000000000".parse().unwrap(),
        slope_high: "40000000000000000".parse().unwrap(),
        kink: "800000000000000000".parse().unwrap(),
    };
    for (c, d, expected) in [
        (60, 40, "5000000000000000"),
        (20, 80, "10000000000000000"),
        (10, 90, "30000000000000000"),
    ] {
        assert_eq!(
            rate.rate(amount(c), amount(d)).unwrap(),
            expected.parse().unwrap()
        );
    }
}
#[test]
fn vf_system_accrual_simulation_does_not_mutate_parent_or_double_charge() {
    let r = funded();
    let before = r.state.clone();
    let one = engine::build_block(&r.scenario, &r.state, &[]).unwrap();
    let two = engine::build_block(&r.scenario, &r.state, &[]).unwrap();
    assert_eq!(one, two);
    assert_eq!(r.state, before);
    assert_eq!(one.state.lending["credit"].debt, 90.into());
    let mut same = one.state.clone();
    assert!(accrue(&r.scenario, &mut same).unwrap().is_empty());
    assert_eq!(same, one.state);
}
#[test]
fn vf_credit_checkpoint_and_protected_account_configuration() {
    let mut r = funded();
    r.scenario.blocks.push(vec![
        tx("price", "oracle", 0, vec![publish(11, 2, 1)]),
        tx("liquidate", "carol", 0, vec![liquidation()]),
        tx("redeem", "alice", 1, vec![withdraw(25)]),
    ]);
    let mut resumed = Run::restore(&r.checkpoint().unwrap()).unwrap();
    r.finish().unwrap();
    resumed.finish().unwrap();
    assert_eq!(r, resumed);
    for protected in ["reserve", "escrow-bob"] {
        let mut s = scenario();
        s.chain.fee_account = protected.into();
        assert!(s.validate().is_err());
    }
    let mut s = scenario();
    s.blocks = vec![vec![tx(
        "donation",
        "alice",
        0,
        vec![Call::Transfer {
            to: "reserve".into(),
            asset: "Q".into(),
            amount: 1.into(),
        }],
    )]];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(
        r.blocks[0].receipts[0].error.as_deref(),
        Some("protected protocol account")
    );
}

#[test]
fn vf_deposit_share_rounding_minimums_last_exit_and_income_without_holders() {
    let r = funded();
    // After one interval S/T = 108/100: one atom would mint zero shares.
    for (call, error) in [
        (supply(1), "min_shares or zero mint"),
        (
            Call::Supply {
                market: "credit".into(),
                amount: 10.into(),
                min_shares: 10.into(),
            },
            "min_shares or zero mint",
        ),
        (
            Call::Withdraw {
                market: "credit".into(),
                shares: 10.into(),
                min_assets: 11.into(),
            },
            "min_assets",
        ),
    ] {
        let who = if matches!(call, Call::Withdraw { .. }) {
            "alice"
        } else {
            "carol"
        };
        let b = engine::build_block(
            &r.scenario,
            &r.state,
            &[tx("minimum", who, u64::from(who == "alice"), vec![call])],
        )
        .unwrap();
        assert_eq!(b.receipts[0].error.as_deref(), Some(error));
        assert_eq!(b.state.lending["credit"].total_shares, 100.into());
    }
    let b = engine::build_block(
        &r.scenario,
        &r.state,
        &[
            tx("repay", "carol", 0, vec![repay(1000)]),
            tx("round-down", "alice", 1, vec![withdraw(25)]),
            tx("last", "alice", 2, vec![withdraw(75)]),
        ],
    )
    .unwrap();
    assert_eq!(b.state.balance("alice", "Q"), 108.into());
    assert_eq!(
        tuple(&r.scenario, &b.state),
        (2.into(), 0.into(), 0.into(), 2.into())
    );
    assert_eq!(b.state.lending["credit"].total_shares, Amount::ZERO);
    // Bootstrap does not grant a new depositor the existing two atoms of equity.
    let boot = engine::build_block(
        &r.scenario,
        &b.state,
        &[tx("bootstrap", "carol", 1, vec![supply(10)])],
    )
    .unwrap();
    assert_eq!(
        tuple(&r.scenario, &boot.state),
        (12.into(), 0.into(), 10.into(), 2.into())
    );
    assert_eq!(boot.state.lending["credit"].total_shares, 10.into());

    let mut s = r.scenario.clone();
    s.lending.get_mut("credit").unwrap().rate = Rate::Constant { rate: scale() };
    let no_holders = engine::build_block(
        &s,
        &r.state,
        &[
            tx("repay-150", "carol", 0, vec![repay(150)]),
            tx("exit", "alice", 1, vec![withdraw(100)]),
        ],
    )
    .unwrap();
    assert_eq!(
        tuple(&s, &no_holders.state),
        (6.into(), 10.into(), 0.into(), 16.into())
    );
    let next = engine::build_block(&s, &no_holders.state, &[]).unwrap();
    assert_eq!(
        tuple(&s, &next.state),
        (6.into(), 20.into(), 0.into(), 26.into())
    );
}
#[test]
fn vf_partial_repay_keeps_fraction_and_closed_position_cannot_spend() {
    let mut s = scenario();
    s.lending.get_mut("credit").unwrap().rate = Rate::Constant {
        rate: "100000000000000000".parse().unwrap(),
    };
    s.blocks = vec![
        vec![
            tx("supply", "alice", 0, vec![supply(100)]),
            tx("open", "bob", 0, vec![collateral(6), borrow(3)]),
        ],
        vec![tx("partial", "carol", 0, vec![repay(1)])],
        vec![],
        vec![
            tx("close", "carol", 1, vec![repay(100)]),
            tx("closed", "carol", 2, vec![repay(100)]),
        ],
    ];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(
        r.blocks[1].state.lending["credit"].positions["bob"].debt,
        2.into()
    );
    assert_eq!(
        r.blocks[1].state.lending["credit"].positions["bob"].remainder,
        "300000000000000000".parse().unwrap()
    );
    assert_eq!(
        r.blocks[2].state.lending["credit"].positions["bob"].remainder,
        "500000000000000000".parse().unwrap()
    );
    assert_eq!(r.state.balance("carol", "Q"), 997.into());
    assert_eq!(
        r.blocks[3].receipts[1].error.as_deref(),
        Some("position has no debt")
    );
}
#[test]
fn vf_liquidation_partial_close_dust_and_health_boundary() {
    let r = funded();
    let mut s = r.scenario.clone();
    s.lending.get_mut("credit").unwrap().close_bps = 5000;
    let half = Call::Liquidate {
        market: "credit".into(),
        borrower: "bob".into(),
        amount: 1000.into(),
        min_collateral: 4.into(),
    };
    let b = engine::build_block(
        &s,
        &r.state,
        &[
            tx("price", "oracle", 0, vec![publish(11, 2, 1)]),
            tx("half", "carol", 0, vec![half]),
        ],
    )
    .unwrap();
    assert_eq!(
        tuple(&s, &b.state),
        (65.into(), 45.into(), 108.into(), 2.into())
    );
    assert_eq!(b.state.balance("escrow-bob", "X"), 2.into());
    assert!(b.state.lending["credit"].losses.is_empty());
    s.lending.get_mut("credit").unwrap().rate = Rate::Constant { rate: Amount::ZERO };
    s.lending.get_mut("credit").unwrap().close_bps = 1;
    let mut dust_price = publish(1, 2, 1);
    if let Call::PublishOracle { price, .. } = &mut dust_price {
        *price = Price::new("X".into(), "Q".into(), 1.into(), 10.into()).unwrap();
    }
    let dust = engine::build_block(
        &s,
        &r.state,
        &[
            tx("dust-price", "oracle", 0, vec![dust_price]),
            tx("dust", "carol", 0, vec![liquidation()]),
        ],
    )
    .unwrap();
    assert_eq!(
        tuple(&s, &dust.state),
        (21.into(), 0.into(), 21.into(), 0.into())
    );
    assert_eq!(dust.state.lending["credit"].losses[0].amount, 79.into());
    assert_eq!(dust.state.balance("carol", "Q"), 999.into());
    assert_eq!(dust.state.balance("carol", "X"), 6.into());
    // floor(6*17*.8)=81 >=80: liquidation cannot seize a healthy position.
    let healthy = engine::build_block(
        &s,
        &r.state,
        &[
            tx("price", "oracle", 0, vec![publish(17, 2, 1)]),
            tx("healthy", "carol", 0, vec![liquidation()]),
        ],
    )
    .unwrap();
    assert_eq!(
        healthy.receipts[1].error.as_deref(),
        Some("position is not liquidatable")
    );
}
#[test]
fn vf_oracle_safe_operations_stale_missing_and_actual_consumer_record() {
    let r = funded();
    let mut s = r.scenario.clone();
    s.chain.interval = 60.into();
    s.lending.get_mut("credit").unwrap().rate = Rate::Constant { rate: Amount::ZERO };
    let b = engine::build_block(
        &s,
        &r.state,
        &[
            tx(
                "stale-withdraw",
                "bob",
                1,
                vec![Call::WithdrawCollateral {
                    market: "credit".into(),
                    amount: 1.into(),
                }],
            ),
            tx("stale-liquidation", "carol", 0, vec![liquidation()]),
            tx("safe", "carol", 1, vec![supply(10), repay(100)]),
            tx(
                "no-debt",
                "bob",
                2,
                vec![
                    Call::WithdrawCollateral {
                        market: "credit".into(),
                        amount: 6.into(),
                    },
                    Call::SetCollateralUsage {
                        market: "credit".into(),
                        enabled: false,
                    },
                ],
            ),
        ],
    )
    .unwrap();
    assert_eq!(
        b.receipts
            .iter()
            .map(|r| r.status.as_str())
            .collect::<Vec<_>>(),
        ["ProtocolRevert", "ProtocolRevert", "Success", "Success"]
    );
    let observed = b.receipts[0].trace[0].oracle.as_ref().unwrap();
    assert_eq!(observed.status, "Stale");
    assert_eq!(observed.age, Some(61.into()));
    assert_eq!(b.state.balance("bob", "X"), 6.into());
    let mut s = scenario();
    s.feeds.get_mut("feed").unwrap().initial = None;
    s.blocks = vec![vec![
        tx("safe", "alice", 0, vec![supply(100)]),
        tx("deposit", "bob", 0, vec![collateral(6)]),
        tx("borrow", "bob", 1, vec![borrow(1)]),
        tx(
            "no-debt",
            "bob",
            2,
            vec![Call::WithdrawCollateral {
                market: "credit".into(),
                amount: 6.into(),
            }],
        ),
    ]];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(
        r.blocks[0].receipts[2].error.as_deref(),
        Some("oracle unavailable")
    );
    assert_eq!(r.blocks[0].receipts[3].status, "Success");
    assert_eq!(
        views(&r.scenario, &r.state)[0].positions[0].health,
        "NoDebt"
    );
    let mut s = scenario();
    s.blocks = vec![vec![
        tx("supply", "alice", 0, vec![supply(100)]),
        tx("open", "bob", 0, vec![collateral(6), borrow(80)]),
        tx("later-price", "oracle", 0, vec![publish(11, 1, 1)]),
    ]];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(
        r.blocks[0].receipts[1].trace[1]
            .oracle
            .as_ref()
            .unwrap()
            .record
            .as_ref()
            .unwrap()
            .price
            .numerator(),
        20.into()
    );
    assert_eq!(
        views(&r.scenario, &r.state)[0]
            .oracle
            .record
            .as_ref()
            .unwrap()
            .price
            .numerator(),
        11.into()
    );
    assert_eq!(
        views(&r.scenario, &r.state)[0].positions[0].health,
        "Liquidatable"
    );
}
#[test]
fn vf_collateral_limits_disable_caps_and_liquidation_minimum() {
    let r = funded();
    let mut s = r.scenario.clone();
    s.lending.get_mut("credit").unwrap().rate = Rate::Constant { rate: Amount::ZERO };
    for call in [
        Call::WithdrawCollateral {
            market: "credit".into(),
            amount: 2.into(),
        },
        Call::SetCollateralUsage {
            market: "credit".into(),
            enabled: false,
        },
    ] {
        let b = engine::build_block(&s, &r.state, &[tx("bad", "bob", 1, vec![call])]).unwrap();
        assert_eq!(b.receipts[0].status, "ProtocolRevert");
        assert_eq!(b.state.balance("escrow-bob", "X"), 6.into());
        assert!(b.state.lending["credit"].positions["bob"].collateral_enabled);
    }
    let b = engine::build_block(
        &s,
        &r.state,
        &[tx(
            "boundary",
            "bob",
            1,
            vec![Call::WithdrawCollateral {
                market: "credit".into(),
                amount: 1.into(),
            }],
        )],
    )
    .unwrap();
    // HF = floor(5*20*.8)/80 = 1 exactly.
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(views(&s, &b.state)[0].positions[0].health, "Healthy");
    s.lending.get_mut("credit").unwrap().borrow_cap = 80.into();
    let b = engine::build_block(&s, &r.state, &[tx("cap", "bob", 1, vec![borrow(1)])]).unwrap();
    assert_eq!(b.receipts[0].error.as_deref(), Some("borrow cap"));
    let min = Call::Liquidate {
        market: "credit".into(),
        borrower: "bob".into(),
        amount: 1000.into(),
        min_collateral: 7.into(),
    };
    let b = engine::build_block(
        &s,
        &r.state,
        &[
            tx("price", "oracle", 0, vec![publish(11, 2, 1)]),
            tx("min", "carol", 0, vec![min]),
        ],
    )
    .unwrap();
    assert_eq!(
        b.receipts[1].error.as_deref(),
        Some("zero liquidation or min_collateral")
    );
    assert_eq!(b.state.balance("carol", "Q"), 1000.into());
}
#[test]
fn vf_oracle_invalid_source_pair_duplicate_old_observation_and_rollback() {
    let r = funded();
    let mut bad_source = publish(11, 2, 1);
    if let Call::PublishOracle { source_id, .. } = &mut bad_source {
        *source_id = "forged".into();
    }
    let mut bad_pair = publish(11, 2, 1);
    if let Call::PublishOracle { price, .. } = &mut bad_pair {
        *price = Price::new("Q".into(), "X".into(), 1.into(), 11.into()).unwrap();
    }
    let mut duplicate = publish(11, 2, 2);
    if let Call::PublishOracle { update_id, .. } = &mut duplicate {
        *update_id = "update-1".into();
    }
    let b = engine::build_block(
        &r.scenario,
        &r.state,
        &[
            tx("valid", "oracle", 0, vec![publish(11, 2, 1)]),
            tx("source", "oracle", 1, vec![bad_source]),
            tx("pair", "oracle", 2, vec![bad_pair]),
            tx("duplicate", "oracle", 3, vec![duplicate]),
            tx("old", "oracle", 4, vec![publish(30, 1, 2)]),
            tx("rollback", "oracle", 5, vec![publish(30, 2, 2), fail()]),
        ],
    )
    .unwrap();
    assert_eq!(b.receipts[0].status, "Success");
    assert!(b.receipts[1..].iter().all(|r| r.status == "ProtocolRevert"));
    let record = b.state.feeds["feed"].record.as_ref().unwrap();
    assert_eq!(record.price.numerator(), 11.into());
    assert_eq!(record.sequence, 1.into());
    assert!(!b.state.feeds["feed"].seen_updates.contains("update-2"));
}
#[test]
fn vf_bankrupt_defensive_state_forbids_supply_and_redemption() {
    // An invariant-valid loss snapshot tests the defensive Bankrupt branch.
    // This is not claimed to be reachable by this preset's positive-payment liquidations.
    let s = scenario();
    let mut w = World::genesis(&s).unwrap();
    let l = std::sync::Arc::make_mut(&mut w.lending)
        .get_mut("credit")
        .unwrap();
    l.total_shares = 100.into();
    l.deposit_shares.insert("alice".into(), 100.into());
    l.losses.push(Loss {
        block: Amount::ZERO,
        borrower: "bob".into(),
        amount: 100.into(),
        equity_loss: Amount::ZERO,
        depositor_loss: 100.into(),
    });
    w.invariant(&s).unwrap();
    let b = engine::build_block(
        &s,
        &w,
        &[
            tx("supply", "alice", 0, vec![supply(1)]),
            tx("withdraw", "alice", 1, vec![withdraw(1)]),
        ],
    )
    .unwrap();
    assert!(
        b.receipts
            .iter()
            .all(|r| r.status == "ProtocolRevert"
                && r.error.as_ref().unwrap().starts_with("Bankrupt"))
    );
    assert_eq!(b.state.balance("alice", "Q"), 100.into());
    assert_eq!(b.state.lending["credit"].total_shares, 100.into());
}
#[test]
fn vf_escrow_isolation_multiple_positions_and_aggregate_interest_distribution() {
    let mut s = scenario();
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), 200.into());
    s.accounts
        .get_mut("carol")
        .unwrap()
        .balances
        .insert("X".into(), 6.into());
    s.accounts.insert("escrow-carol".into(), account(0, 0));
    s.lending
        .get_mut("credit")
        .unwrap()
        .positions
        .insert("carol".into(), "escrow-carol".into());
    s.blocks = vec![
        vec![
            tx("supply", "alice", 0, vec![supply(200)]),
            tx("bob", "bob", 0, vec![collateral(6), borrow(80)]),
            tx("carol", "carol", 0, vec![collateral(6), borrow(20)]),
        ],
        vec![
            tx("price", "oracle", 0, vec![publish(11, 2, 1)]),
            tx("liquidate-bob", "carol", 1, vec![liquidation()]),
        ],
    ];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(
        tuple(&r.scenario, &r.state),
        (160.into(), 22.into(), 182.into(), 0.into())
    );
    assert_eq!(r.state.balance("escrow-carol", "X"), 6.into());
    assert_eq!(r.state.balance("escrow-bob", "X"), Amount::ZERO);
    assert_eq!(r.state.lending["credit"].positions["carol"].debt, 22.into());
    assert_eq!(
        r.state.lending["credit"].positions["carol"].remainder,
        "500000000000000000".parse().unwrap()
    );
    assert_eq!(r.state.lending["credit"].losses.len(), 1);
    r.state.invariant(&r.scenario).unwrap();
}
#[test]
fn vf_accrual_overflow_stops_whole_block_without_committing() {
    let r = funded();
    let mut s = r.scenario.clone();
    s.chain.interval = Amount::MAX.checked_sub(1.into()).unwrap();
    s.lending.get_mut("credit").unwrap().rate = Rate::Constant { rate: scale() };
    let before = r.state.clone();
    assert!(
        engine::build_block(&s, &r.state, &[tx("repay", "carol", 0, vec![repay(1000)])]).is_err()
    );
    assert_eq!(r.state, before);
}
#[test]
fn vf_liquidation_deterministic_boundary_sweep_preserves_cash_and_claims() {
    let r = funded();
    let mut random = 701u64;
    let mut successes = 0;
    let mut reverts = 0;
    for i in 0..256 {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        let n = 1 + random % 200;
        let d = 1 + (random >> 12) % 10;
        let request = 1 + (random >> 20) % 100;
        let mut s = r.scenario.clone();
        let cfg = s.lending.get_mut("credit").unwrap();
        cfg.rate = Rate::Constant { rate: Amount::ZERO };
        cfg.close_bps = (1 + (random >> 32) % 10000) as u16;
        let mut price = publish(1, 2, 1);
        if let Call::PublishOracle { price, .. } = &mut price {
            *price = Price::new("X".into(), "Q".into(), n.into(), d.into()).unwrap();
        }
        let b = engine::build_block(
            &s,
            &r.state,
            &[
                tx("price", "oracle", 0, vec![price]),
                tx(
                    "liquidate",
                    "carol",
                    0,
                    vec![Call::Liquidate {
                        market: "credit".into(),
                        borrower: "bob".into(),
                        amount: request.into(),
                        min_collateral: Amount::ZERO,
                    }],
                ),
            ],
        )
        .unwrap();
        b.state.invariant(&s).unwrap();
        if b.receipts[1].status == "Success" {
            successes += 1;
            let paid = 1000u64
                - b.state
                    .balance("carol", "Q")
                    .to_string()
                    .parse::<u64>()
                    .unwrap();
            assert!(paid > 0 && paid <= request && paid <= 80, "case {i}");
            assert_eq!(b.state.balance("reserve", "Q"), (20 + paid).into());
            assert_eq!(
                b.state
                    .balance("escrow-bob", "X")
                    .checked_add(b.state.balance("carol", "X"))
                    .unwrap(),
                6.into()
            );
            if b.state.balance("escrow-bob", "X").is_zero() {
                assert_eq!(b.state.lending["credit"].debt, Amount::ZERO);
            } else {
                assert!(b.state.lending["credit"].losses.is_empty());
            }
        } else {
            reverts += 1;
            assert_eq!(
                tuple(&s, &b.state),
                (20.into(), 80.into(), 100.into(), 0.into())
            );
            assert_eq!(b.state.balance("escrow-bob", "X"), 6.into());
            assert_eq!(b.state.balance("carol", "Q"), 1000.into());
        }
    }
    assert!(successes > 30 && reverts > 30);
}
