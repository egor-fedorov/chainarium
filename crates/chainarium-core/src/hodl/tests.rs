use super::*;
use crate::{Scenario, amount::Price};
use num_bigint::{BigInt, BigUint};
use serde_json::{Value, json};

fn q(n: i64, d: u64) -> Exact {
    Exact::ratio(n.into(), d.into(), 32768).unwrap()
}
fn component(kind: &str, asset: &str, quantity: Option<Exact>) -> Component {
    Component {
        key: valuation::Key {
            account: "alice".into(),
            kind: kind.into(),
            protocol: "test".into(),
            position: "".into(),
            asset: asset.into(),
        },
        quantity,
        mark: Mark::zero(),
        price: None,
        deposit_cash: None,
    }
}
fn snapshot(components: Vec<Component>, initial: Mark) -> Snapshot {
    Snapshot {
        component_checks: components.len(),
        components,
        mark: initial,
        by_asset: BTreeMap::new(),
    }
}
fn policy(price: Option<u64>) -> valuation::Policy {
    let s = Scenario::parse(include_bytes!(
        "../../../../scenarios/demos/sc-mtm-capital.json"
    ))
    .unwrap();
    let mut p = s.valuation_policy.unwrap();
    p.reference_feeds.clear();
    if let Some(n) = price {
        p.reference_feeds.insert(
            "X".into(),
            valuation::Feed {
                source_id: "reference".into(),
                records: vec![valuation::Record {
                    available_at: 0.into(),
                    observed_at: 0.into(),
                    sequence: 0.into(),
                    price: Price::new("X".into(), "Q".into(), n.into(), 1.into()).unwrap(),
                }],
            },
        );
    }
    p
}
fn flow(tx: &str, asset: &str, n: u64, incoming: bool, price: Option<u64>) -> Flow {
    Flow {
        portfolio: "owner".into(),
        block: 1.into(),
        timestamp: 1.into(),
        tx_id: tx.into(),
        from: "outside".into(),
        to: "alice".into(),
        asset: asset.into(),
        amount: n.into(),
        direction: if incoming { "Inflow" } else { "Outflow" }.into(),
        mark: price.map_or_else(
            || unavailable("MissingPrice"),
            |p| Mark::priced(q((n * p) as i64, 1)),
        ),
        price: Some(valuation::price_at(&policy(price), asset, 1.into())),
    }
}
fn cursor(f: &Flow, index: usize) -> Cursor {
    Cursor {
        block: f.block,
        timestamp: f.timestamp,
        receipt_index: index,
        event_index: 0,
        tx_id: f.tx_id.clone(),
    }
}

#[test]
fn eighteen_preimplementation_fraction_cases() {
    // Literal inputs published before implementation; expected arithmetic comes
    // from the independent Python Fraction artifact, not production outputs.
    struct Case {
        name: &'static str,
        x: Exact,
        cash: u64,
        p0: Option<u64>,
        pt: Option<u64>,
        actual: Option<Exact>,
        flows: Vec<Flow>,
        liability: bool,
    }
    let cases = vec![
        Case {
            name: "static_basket",
            x: q(10, 1),
            cash: 100,
            p0: Some(10),
            pt: Some(12),
            actual: Some(q(220, 1)),
            flows: vec![],
            liability: false,
        },
        Case {
            name: "cpmm_price_shock",
            x: q(100, 1),
            cash: 100,
            p0: Some(1),
            pt: Some(4),
            actual: Some(q(400, 1)),
            flows: vec![],
            liability: false,
        },
        Case {
            name: "dynamic_inflow",
            x: q(10, 1),
            cash: 100,
            p0: Some(10),
            pt: Some(15),
            actual: Some(q(250, 1)),
            flows: vec![flow("deposit", "X", 2, true, Some(10))],
            liability: false,
        },
        Case {
            name: "dynamic_outflow",
            x: q(10, 1),
            cash: 100,
            p0: Some(10),
            pt: Some(15),
            actual: Some(q(195, 1)),
            flows: vec![flow("withdraw", "X", 3, false, Some(12))],
            liability: false,
        },
        Case {
            name: "canonical_order_in_then_out",
            x: q(0, 1),
            cash: 100,
            p0: None,
            pt: Some(10),
            actual: Some(q(110, 1)),
            flows: vec![
                flow("z-in", "X", 2, true, Some(10)),
                flow("a-out", "X", 1, false, Some(10)),
            ],
            liability: false,
        },
        Case {
            name: "canonical_order_out_then_in",
            x: q(0, 1),
            cash: 100,
            p0: None,
            pt: Some(10),
            actual: Some(q(110, 1)),
            flows: vec![
                flow("a-out", "X", 1, false, Some(10)),
                flow("z-in", "X", 2, true, Some(10)),
            ],
            liability: false,
        },
        Case {
            name: "outflow_requires_no_implicit_sale",
            x: q(10, 1),
            cash: 0,
            p0: Some(10),
            pt: Some(10),
            actual: Some(q(90, 1)),
            flows: vec![flow("withdraw-Q", "Q", 10, false, Some(1))],
            liability: false,
        },
        Case {
            name: "historical_unpriced_flow",
            x: q(1, 1),
            cash: 10,
            p0: Some(2),
            pt: Some(3),
            actual: Some(q(18, 1)),
            flows: vec![flow("unknown-price", "X", 2, true, None)],
            liability: false,
        },
        Case {
            name: "zero_final_asset_needs_no_price",
            x: q(2, 1),
            cash: 100,
            p0: Some(10),
            pt: None,
            actual: Some(q(100, 1)),
            flows: vec![flow("withdraw-all-X", "X", 2, false, Some(10))],
            liability: false,
        },
        Case {
            name: "fractional_initial_claim",
            x: q(1, 3),
            cash: 0,
            p0: Some(3),
            pt: Some(5),
            actual: Some(q(4, 3)),
            flows: vec![],
            liability: false,
        },
        Case {
            name: "failed_paid_transfer",
            x: q(0, 1),
            cash: 1000,
            p0: None,
            pt: None,
            actual: Some(q(885, 1)),
            flows: vec![],
            liability: false,
        },
        Case {
            name: "unlabelled_transfer_is_performance",
            x: q(0, 1),
            cash: 100,
            p0: None,
            pt: None,
            actual: Some(q(90, 1)),
            flows: vec![],
            liability: false,
        },
        Case {
            name: "borrow_is_not_capital",
            x: q(0, 1),
            cash: 100,
            p0: None,
            pt: None,
            actual: Some(q(100, 1)),
            flows: vec![],
            liability: false,
        },
        Case {
            name: "interest_is_performance",
            x: q(0, 1),
            cash: 100,
            p0: None,
            pt: None,
            actual: Some(q(90, 1)),
            flows: vec![],
            liability: false,
        },
        Case {
            name: "initial_liability_is_not_hodl",
            x: q(0, 1),
            cash: 100,
            p0: None,
            pt: None,
            actual: Some(q(100, 1)),
            flows: vec![],
            liability: true,
        },
        Case {
            name: "missing_initial_price",
            x: q(1, 1),
            cash: 0,
            p0: None,
            pt: Some(3),
            actual: Some(q(3, 1)),
            flows: vec![],
            liability: false,
        },
        Case {
            name: "missing_final_price",
            x: q(1, 1),
            cash: 0,
            p0: Some(2),
            pt: None,
            actual: None,
            flows: vec![],
            liability: false,
        },
        Case {
            name: "fractional_inventory_cannot_fund_one_atom",
            x: q(1, 3),
            cash: 0,
            p0: Some(3),
            pt: Some(3),
            actual: Some(q(0, 1)),
            flows: vec![flow("too-large", "X", 1, false, Some(3))],
            liability: false,
        },
    ];
    let reference: Value = serde_json::from_slice(include_bytes!(
        "../../../../docs/reviews/hodl-reference.json"
    ))
    .unwrap();
    for c in cases {
        let mut initial = Mark::priced(q(c.cash as i64, 1));
        if !c.x.is_zero() {
            initial.add(
                &c.p0.map_or_else(
                    || unavailable("MissingPrice"),
                    |p| Mark::priced(c.x.mul(&q(p as i64, 1), 32768).unwrap()),
                ),
                32768,
            );
        }
        let mut components = vec![
            component("Wallet", "Q", Some(q(c.cash as i64, 1))),
            component("CpmmLP", "X", Some(c.x)),
        ];
        if c.liability {
            // Gross offset cannot turn initial debt into unlevered HODL.
            components.extend([
                component("Wallet", "Q", Some(q(1, 1))),
                component("Debt", "Q", Some(q(-1, 1))),
            ]);
        }
        let mut shadow = Shadow::new(snapshot(components, initial.clone()), &Limits::default());
        for (i, f) in c.flows.iter().enumerate() {
            if f.direction == "Inflow" {
                shadow.inflows.add(&f.mark, 32768);
            } else {
                shadow.outflows.add(&f.mark, 32768);
            }
            shadow.apply(f, &cursor(f, i), &Limits::default());
        }
        let hold = shadow.mark(&policy(c.pt), 1.into(), 32768).mark;
        let actual = c
            .actual
            .map_or_else(|| unavailable("MissingPrice"), Mark::priced);
        let ap = valuation::pnl(&actual, &initial, &shadow.inflows, &shadow.outflows, 32768);
        let hp = valuation::pnl(&hold, &initial, &shadow.inflows, &shadow.outflows, 32768);
        let expected = &reference["cases"][c.name];
        for (name, mark) in [
            ("initial", initial),
            ("current", hold),
            ("actual_pnl", ap.clone()),
            ("hodl_pnl", hp.clone()),
            ("excess_pnl", difference(&ap, &hp, 32768)),
            (
                "value_gap",
                difference(
                    &actual,
                    &shadow.mark(&policy(c.pt), 1.into(), 32768).mark,
                    32768,
                ),
            ),
        ] {
            assert_eq!(
                serde_json::to_value(mark.value).unwrap(),
                expected[name]["value"],
                "{} {name}",
                c.name
            );
        }
        match shadow.ledger {
            Ledger::Ready { current } => {
                assert_eq!(expected["ledger_status"], "Ready", "{}", c.name);
                for (a, v) in expected["holdings"].as_object().unwrap() {
                    let n = current.quantities.get(a).cloned().unwrap_or_default();
                    assert_eq!(serde_json::to_value(n).unwrap(), *v, "{} {a}", c.name);
                }
            }
            Ledger::Unavailable {
                failure,
                last_feasible,
            } => {
                assert_eq!(expected["ledger_status"], "Unavailable", "{}", c.name);
                assert_eq!(failure.reason, expected["failure"]["reason"], "{}", c.name);
                if !c.liability {
                    assert_eq!(
                        serde_json::to_value(failure.available).unwrap(),
                        expected["failure"]["available"]
                    );
                    assert!(last_feasible.is_some());
                } else {
                    assert!(last_feasible.is_none());
                }
            }
        }
    }
}

#[test]
fn fractional_grouping_keeps_all_claim_kinds_and_rejects_unknown_quantity() {
    let kinds = [
        "Wallet",
        "CpmmLP",
        "StableLP",
        "ClmmClaim",
        "BookEscrow",
        "DepositClaim",
        "Collateral",
    ];
    let mut cs: Vec<_> = kinds
        .iter()
        .map(|k| component(k, "X", Some(q(1, 7))))
        .collect();
    cs.push(component("Debt", "Q", Some(q(0, 1))));
    let shadow = Shadow::new(
        snapshot(cs.clone(), Mark::priced(q(3, 1))),
        &Limits::default(),
    );
    assert_eq!(shadow.initial_holdings.unwrap().quantities["X"], q(1, 1));
    cs[0].quantity = None;
    let broken = Shadow::new(snapshot(cs, Mark::zero()), &Limits::default());
    assert!(
        matches!(broken.ledger,Ledger::Unavailable {failure:Failure{ref reason,..},last_feasible:None} if reason == "InitialQuantityUnavailable")
    );
}

#[test]
fn asset_caps_zero_one_sixtyfour_sixtyfive_and_reuse_after_withdrawal() {
    for count in [0, 1, 64, 65] {
        let cs = (0..count)
            .map(|i| component("Wallet", &format!("A{i}"), Some(q(1, 1))))
            .collect();
        let shadow = Shadow::new(snapshot(cs, Mark::zero()), &Limits::default());
        assert_eq!(matches!(shadow.ledger, Ledger::Ready { .. }), count <= 64);
    }
    let limits = Limits {
        assets_per_portfolio: 1,
        ..Limits::default()
    };
    let mut shadow = Shadow::new(
        snapshot(
            vec![
                component("Wallet", "X", Some(q(1, 1))),
                component("Wallet", "Q", Some(q(0, 1))),
            ],
            Mark::zero(),
        ),
        &limits,
    );
    let out = flow("out", "X", 1, false, Some(1));
    shadow.apply(&out, &cursor(&out, 0), &limits);
    let add = flow("add", "Q", 1, true, Some(1));
    assert!(matches!(
        shadow.apply(&add, &cursor(&add, 1), &limits),
        Effect::Applied { .. }
    ));
    let extra = flow("extra", "X", 1, true, Some(1));
    assert!(
        matches!(shadow.apply(&extra,&cursor(&extra,2),&limits),Effect::Failed{reason} if reason=="AssetLimit")
    );
    let failed = shadow.ledger.clone();
    assert!(matches!(
        shadow.apply(&out, &cursor(&out, 3), &limits),
        Effect::Skipped { .. }
    ));
    assert_eq!(shadow.ledger, failed);
    assert!(
        shadow
            .mark(&policy(Some(1)), 1.into(), 32768)
            .mark
            .truncated
    );
}

#[test]
fn quantity_overflow_is_permanent_but_mark_overflow_can_recover() {
    let limits = Limits {
        rational_bits: 512,
        ..Limits::default()
    };
    // Two reduced 512-bit quantities overflow only when aggregated.
    let big = Exact::ratio(BigInt::from(1) << 511, BigUint::from(1u8), 512).unwrap();
    let doubled = Shadow::new(
        snapshot(
            vec![
                component("CpmmLP", "X", Some(big.clone())),
                component("StableLP", "X", Some(big.clone())),
            ],
            Mark::zero(),
        ),
        &limits,
    );
    assert!(
        matches!(doubled.ledger,Ledger::Unavailable{failure:Failure{ref reason,..},..} if reason=="ArithmeticLimit")
    );
    let mut shadow = Shadow::new(
        snapshot(vec![component("CpmmLP", "X", Some(big))], Mark::zero()),
        &limits,
    );
    let failed = shadow.mark(&policy(Some(2)), 0.into(), 512);
    assert!(failed.mark.reasons.contains("ArithmeticLimit"));
    assert!(matches!(shadow.ledger, Ledger::Ready { .. }));
    assert!(
        shadow
            .mark(&policy(Some(1)), 1.into(), 512)
            .mark
            .value
            .is_some()
    );
    // Coprime denominator near the bound: adding one atom exceeds 512 bits.
    let denominator: BigUint = (BigUint::from(1u8) << 512) - BigUint::from(1u8);
    let near = Exact::ratio(
        (denominator.clone() - BigUint::from(1u8)).into(),
        denominator,
        512,
    )
    .unwrap();
    shadow = Shadow::new(
        snapshot(vec![component("CpmmLP", "X", Some(near))], Mark::zero()),
        &limits,
    );
    let add = flow("add", "X", 1, true, Some(1));
    assert!(
        matches!(shadow.apply(&add,&cursor(&add,0),&limits),Effect::Failed{reason} if reason=="ArithmeticLimit")
    );
    assert!(matches!(
        shadow.ledger,
        Ledger::Unavailable {
            last_feasible: Some(_),
            ..
        }
    ));
    assert!(matches!(
        shadow.apply(&add, &cursor(&add, 1), &limits),
        Effect::Skipped { .. }
    ));
}

#[test]
fn gross_liability_and_extraction_limits_never_present_partial_initial_inventory() {
    for cs in [
        vec![component("Wallet", "X", Some(q(-1, 1)))],
        vec![component("Debt", "X", Some(q(1, 1)))],
    ] {
        let shadow = Shadow::new(snapshot(cs, Mark::zero()), &Limits::default());
        assert!(
            matches!(shadow.ledger,Ledger::Unavailable{failure:Failure{ref reason,..},last_feasible:None} if reason=="InitialLiabilityUnsupported")
        );
    }
    let shadow = Shadow::new(
        snapshot(
            vec![component("Wallet", "Q", Some(q(100, 1)))],
            unavailable("ComponentLimit"),
        ),
        &Limits::default(),
    );
    assert!(shadow.initial_holdings.is_none());
    assert_eq!(
        serde_json::to_value(shadow.ledger).unwrap()["failure"]["reason"],
        json!("ComponentLimit")
    );
}
