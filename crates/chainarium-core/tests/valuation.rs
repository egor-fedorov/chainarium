use chainarium_core::{
    amount::Price,
    valuation::{self as v, *},
    *,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc};

fn fixture(name: &str) -> Scenario {
    Scenario::parse(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../scenarios/demos/{name}.json")),
        )
        .unwrap(),
    )
    .unwrap()
}
fn policy(accounts: &[&str]) -> Policy {
    Policy {
        version: MODEL.into(),
        numeraire: "Q".into(),
        evaluation_point: "committed-block-end".into(),
        source: "environment-reference".into(),
        max_age_seconds: 10000.into(),
        limits: Limits::default(),
        portfolios: BTreeMap::from([(
            "p".into(),
            Portfolio {
                accounts: accounts.iter().map(|s| s.to_string()).collect(),
            },
        )]),
        reference_feeds: BTreeMap::new(),
        capital_transfers: vec![],
    }
}
fn enable(s: &mut Scenario, accounts: &[&str]) {
    s.model_versions.push(MODEL.into());
    s.valuation_policy = Some(policy(accounts));
}
fn price(s: &mut Scenario, asset: &str, n: Amount, d: Amount) {
    s.valuation_policy.as_mut().unwrap().reference_feeds.insert(
        asset.into(),
        Feed {
            source_id: "test-reference".into(),
            records: vec![Record {
                available_at: 0.into(),
                observed_at: 0.into(),
                sequence: 0.into(),
                price: Price::new(asset.into(), "Q".into(), n, d).unwrap(),
            }],
        },
    );
}
fn simple(accounts: &[&str]) -> Scenario {
    let mut s = fixture("sc-01");
    s.blocks.clear();
    s.protocols.clear();
    s.chain.interval = 1.into();
    for a in s.accounts.values_mut() {
        a.balances.values_mut().for_each(|n| *n = Amount::ZERO);
    }
    enable(&mut s, accounts);
    s
}
fn set(s: &mut Scenario, owner: &str, asset: &str, n: u64) {
    s.accounts
        .get_mut(owner)
        .unwrap()
        .balances
        .insert(asset.into(), n.into());
}
fn tx(id: &str, owner: &str, to: &str, asset: &str, amount: u64, nonce: u64) -> Transaction {
    Transaction {
        id: id.into(),
        agent: if owner == "alice" {
            "trader"
        } else {
            "recipient"
        }
        .into(),
        sender: owner.into(),
        chain: "lab-1".into(),
        nonce: nonce.into(),
        limit: 200.into(),
        price: 0.into(),
        deadline: Amount::MAX,
        program: vec![Call::Transfer {
            to: to.into(),
            asset: asset.into(),
            amount: amount.into(),
        }],
    }
}
fn label(s: &mut Scenario, id: &str) {
    s.valuation_policy
        .as_mut()
        .unwrap()
        .capital_transfers
        .push(CapitalTransfer {
            portfolio: "p".into(),
            tx_id: id.into(),
        });
}
fn finish(s: Scenario) -> Run {
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    r
}
fn view(r: &Run) -> PortfolioView {
    v::at(r, r.blocks.len()).unwrap().portfolios.remove(0)
}
fn exact(mark: &Mark) -> Value {
    assert_eq!(mark.status, "Priced", "{mark:?}");
    serde_json::to_value(mark.value.as_ref().unwrap()).unwrap()
}
fn expect(mark: &Mark, n: &str, d: &str, floor: &str) {
    assert_eq!(
        exact(mark),
        json!({"numerator":n,"denominator":d,"floor_atoms":floor})
    );
}
fn reference(name: &str) -> Value {
    serde_json::from_slice::<Value>(include_bytes!(
        "../../../docs/reviews/valuation-reference.json"
    ))
    .unwrap()["cases"][name]
        .clone()
}

#[test]
fn halves_zero_missing_and_gross_offset_match_reference() {
    let mut s = simple(&["alice", "bob"]);
    set(&mut s, "alice", "X", 1);
    set(&mut s, "bob", "X", 1);
    price(&mut s, "X", 1.into(), 2.into());
    let r = finish(s.clone());
    assert_eq!(
        exact(&view(&r).current.mark),
        reference("two_wallet_halves")["value"]
    );
    assert_eq!(
        view(&r)
            .current
            .components
            .iter()
            .filter(|c| c.key.asset == "X")
            .map(|c| exact(&c.mark)["floor_atoms"].clone())
            .collect::<Vec<_>>(),
        vec![json!("0"), json!("0")]
    );
    s.valuation_policy.as_mut().unwrap().reference_feeds.clear();
    set(&mut s, "alice", "X", 0);
    set(&mut s, "bob", "X", 0);
    assert_eq!(
        exact(&view(&finish(s.clone())).current.mark),
        reference("zero_without_price")["value"]
    );
    set(&mut s, "alice", "X", 1);
    set(&mut s, "alice", "Q", 7);
    let m = view(&finish(s)).current.mark;
    assert!(m.value.is_none());
    assert!(m.reasons.contains("MissingPrice"));
    assert_eq!(
        serde_json::to_value(m.priced_part).unwrap(),
        reference("missing_with_priced_part")["priced_part"]
    );
    let mut s = fixture("sc-05-credit");
    enable(&mut s, &["bob"]);
    s.blocks.clear();
    s.lending.get_mut("credit").unwrap().debt_asset = "X".into();
    // Snapshot-level gross asset/debt test, independent of lending's pair validation.
    let mut w = World::genesis(&fixture("sc-05-credit")).unwrap();
    Arc::make_mut(&mut w.accounts)
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("X".into(), 1.into());
    Arc::make_mut(&mut w.lending)
        .get_mut("credit")
        .unwrap()
        .positions
        .get_mut("bob")
        .unwrap()
        .debt = 1.into();
    let snap = v::snapshot(&s, &w, "p").unwrap();
    assert!(snap.mark.value.is_none());
    assert!(snap.mark.reasons.contains("MissingPrice"));
    price(&mut s, "X", 1.into(), 2.into());
    Arc::make_mut(&mut w.accounts)
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("X".into(), 0.into());
    assert_eq!(
        exact(&v::snapshot(&s, &w, "p").unwrap().mark),
        reference("negative_half")["value"]
    );
}

#[test]
fn capital_fractional_accumulation_and_price_at_execution() {
    let mut s = simple(&["alice"]);
    set(&mut s, "bob", "X", 2);
    price(&mut s, "X", 1.into(), 2.into());
    s.blocks = vec![
        vec![tx("one", "bob", "alice", "X", 1, 0)],
        vec![tx("two", "bob", "alice", "X", 1, 1)],
    ];
    label(&mut s, "one");
    label(&mut s, "two");
    let r = finish(s.clone());
    let p = view(&r);
    assert_eq!(exact(&p.inflows), reference("two_half_inflows")["inflows"]);
    expect(&p.pnl, "0", "1", "0");
    set(&mut s, "alice", "X", 1);
    s.blocks.truncate(1);
    s.valuation_policy
        .as_mut()
        .unwrap()
        .capital_transfers
        .truncate(1);
    let p = view(&finish(s));
    assert_eq!(
        exact(&p.initial),
        reference("rounding_is_presentation")["initial"]
    );
    assert_eq!(exact(&p.pnl), reference("rounding_is_presentation")["pnl"]);
    let mut s = simple(&["alice"]);
    set(&mut s, "alice", "Q", 100);
    set(&mut s, "bob", "X", 2);
    price(&mut s, "X", 10.into(), 1.into());
    s.valuation_policy
        .as_mut()
        .unwrap()
        .reference_feeds
        .get_mut("X")
        .unwrap()
        .records
        .push(Record {
            available_at: 2.into(),
            observed_at: 2.into(),
            sequence: 1.into(),
            price: Price::new("X".into(), "Q".into(), 15.into(), 1.into()).unwrap(),
        });
    s.blocks = vec![vec![tx("in", "bob", "alice", "X", 2, 0)], vec![]];
    label(&mut s, "in");
    let r = finish(s);
    let p = view(&r);
    assert_eq!(exact(&p.pnl), reference("flow_time_price")["pnl"]);
    expect(&p.inflows, "20", "1", "20");
    let flow = &v::at(&r, 1).unwrap().capital_flows[0];
    assert_eq!(flow.timestamp, 1.into());
    expect(&flow.mark, "20", "1", "20");
    assert!(v::at(&r, 2).unwrap().capital_flows.is_empty());
}

#[test]
fn committed_transfer_only_revert_rejection_unlabelled_and_fees() {
    let mut s = simple(&["alice"]);
    set(&mut s, "alice", "Q", 1000);
    let mut t = tx("failed", "alice", "bob", "Q", 900, 0);
    t.price = 1.into();
    s.blocks = vec![vec![t]];
    label(&mut s, "failed");
    let r = finish(s.clone());
    assert_eq!(r.blocks[0].receipts[0].status, "ProtocolRevert");
    assert!(v::at(&r, 1).unwrap().capital_flows.is_empty());
    assert_eq!(
        exact(&view(&r).pnl),
        reference("failed_capital_transfer")["pnl"]
    );
    s.blocks[0][0].nonce = 1.into();
    let r = finish(s.clone());
    assert_eq!(r.blocks[0].receipts.len(), 0);
    expect(&view(&r).pnl, "0", "1", "0");
    s.blocks[0][0].nonce = 0.into();
    s.blocks[0][0].program = vec![Call::Transfer {
        to: "bob".into(),
        asset: "Q".into(),
        amount: 100.into(),
    }];
    let r = finish(s.clone());
    expect(&view(&r).outflows, "100", "1", "100");
    expect(&view(&r).pnl, "-115", "1", "-115");
    s.valuation_policy
        .as_mut()
        .unwrap()
        .capital_transfers
        .clear();
    let r = finish(s);
    expect(&view(&r).pnl, "-215", "1", "-215");
    assert!(
        v::at(&r, 1)
            .unwrap()
            .boundary_transfers
            .iter()
            .any(|n| n.event.amount == 100.into() && !n.capital_label)
    );
}

#[test]
fn internal_transfer_and_two_portfolio_direction_with_overlap() {
    let mut s = simple(&["alice", "bob"]);
    set(&mut s, "alice", "Q", 10);
    s.blocks = vec![vec![tx("in", "alice", "bob", "Q", 3, 0)]];
    let r = finish(s.clone());
    expect(&view(&r).pnl, "0", "1", "0");
    assert!(v::at(&r, 1).unwrap().boundary_transfers.is_empty());
    label(&mut s, "in");
    assert!(s.validate().is_err());
    s.valuation_policy
        .as_mut()
        .unwrap()
        .portfolios
        .get_mut("p")
        .unwrap()
        .accounts = vec!["alice".into()];
    let p = s.valuation_policy.as_mut().unwrap();
    p.portfolios.insert(
        "q".into(),
        Portfolio {
            accounts: vec!["bob".into()],
        },
    );
    p.portfolios.insert(
        "overlap".into(),
        Portfolio {
            accounts: vec!["alice".into()],
        },
    );
    p.capital_transfers.push(CapitalTransfer {
        portfolio: "q".into(),
        tx_id: "in".into(),
    });
    let report = v::at(&finish(s), 1).unwrap();
    assert_eq!(report.capital_flows.len(), 2);
    let a = report
        .portfolios
        .iter()
        .find(|p| p.portfolio == "p")
        .unwrap();
    assert_eq!(a.overlaps, vec!["overlap"]);
    expect(&a.outflows, "3", "1", "3");
    expect(&a.pnl, "0", "1", "0");
    let b = report
        .portfolios
        .iter()
        .find(|p| p.portfolio == "q")
        .unwrap();
    expect(&b.inflows, "3", "1", "3");
    expect(&b.pnl, "0", "1", "0");
}

#[test]
fn freshness_availability_future_and_historical_unpriced_flow() {
    let mut s = simple(&["alice"]);
    price(&mut s, "X", 2.into(), 1.into());
    let p = s.valuation_policy.as_mut().unwrap();
    p.max_age_seconds = 60.into();
    p.reference_feeds
        .get_mut("X")
        .unwrap()
        .records
        .push(Record {
            available_at: 62.into(),
            observed_at: 59.into(),
            sequence: 1.into(),
            price: Price::new("X".into(), "Q".into(), 3.into(), 1.into()).unwrap(),
        });
    for t in [60, 61, 62] {
        let x = price_at(p, "X", t.into());
        let expected = reference("freshness_60_61_and_delayed_record")[t.to_string()].clone();
        assert_eq!(x.mark.status, expected["status"]);
        if t == 61 {
            assert!(x.mark.reasons.contains("Stale"));
        } else {
            assert_eq!(exact(&x.mark), expected["value"]);
        }
    }
    p.reference_feeds.get_mut("X").unwrap().records.truncate(1);
    p.reference_feeds.get_mut("X").unwrap().records[0].observed_at = 10.into();
    assert!(
        price_at(p, "X", 9.into())
            .mark
            .reasons
            .contains("FutureObservation")
    );
    expect(&price_at(p, "X", 10.into()).mark, "2", "1", "2");
    let r = &mut p.reference_feeds.get_mut("X").unwrap().records[0];
    r.available_at = 10.into();
    r.observed_at = 0.into();
    assert!(
        price_at(p, "X", 0.into())
            .mark
            .reasons
            .contains("NoAvailableRecord")
    );
    set(&mut s, "bob", "X", 1);
    s.blocks = vec![vec![tx("in", "bob", "alice", "X", 1, 0)]];
    s.blocks.resize(10, vec![]);
    label(&mut s, "in");
    let r = finish(s);
    let p = view(&r);
    expect(&p.current.mark, "2", "1", "2");
    assert!(p.inflows.value.is_none());
    assert!(p.pnl.reasons.contains("HistoricalFlowUnpriced"));
    assert!(
        v::at(&r, 1).unwrap().capital_flows[0]
            .mark
            .reasons
            .contains("NoAvailableRecord")
    );
}

#[test]
fn lending_borrow_interest_post_loss_and_cash_bound() {
    let mut s = fixture("sc-05-credit");
    enable(&mut s, &["bob"]);
    price(&mut s, "X", 20.into(), 1.into());
    set(&mut s, "bob", "Q", 100);
    s.blocks = vec![s.blocks[0].clone(), vec![]];
    let mut r = Run::new(s.clone()).unwrap();
    r.step().unwrap();
    let p = view(&r);
    expect(&p.current.mark, "220", "1", "220");
    assert_eq!(exact(&p.pnl), reference("borrow_is_not_capital")["pnl"]);
    r.step().unwrap();
    let p = view(&r);
    expect(&p.current.mark, "210", "1", "210");
    assert_eq!(exact(&p.pnl), reference("interest_is_not_capital")["pnl"]);
    expect(&p.inflows, "0", "1", "0");
    // Accepted post-loss native claim arithmetic on a snapshot with partial cash.
    s.valuation_policy
        .as_mut()
        .unwrap()
        .portfolios
        .get_mut("p")
        .unwrap()
        .accounts = vec!["alice".into()];
    let mut w = r.state.clone();
    let state = Arc::make_mut(&mut w.lending).get_mut("credit").unwrap();
    state.claims = 70.into();
    state.total_shares = 100.into();
    state.deposit_shares.insert("alice".into(), 30.into());
    let accounts = Arc::make_mut(&mut w.accounts);
    accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), 7.into());
    accounts
        .get_mut("reserve")
        .unwrap()
        .balances
        .insert("Q".into(), 10.into());
    let snap = v::snapshot(&s, &w, "p").unwrap();
    assert_eq!(
        exact(&snap.mark),
        reference("post_loss_deposit")["mark"]["value"]
    );
    let cash = snap
        .components
        .iter()
        .find_map(|c| c.deposit_cash.as_ref())
        .unwrap();
    assert_eq!(
        (cash.claim, cash.reserve_cash, cash.cash_bound),
        (21.into(), 10.into(), 10.into())
    );
    // Disabled collateral remains owned; viewing never accrues debt or remainder.
    s.valuation_policy
        .as_mut()
        .unwrap()
        .portfolios
        .get_mut("p")
        .unwrap()
        .accounts = vec!["bob".into()];
    let position = Arc::make_mut(&mut w.lending)
        .get_mut("credit")
        .unwrap()
        .positions
        .get_mut("bob")
        .unwrap();
    position.collateral_enabled = false;
    position.remainder = 999.into();
    let before = w.clone();
    let snap = v::snapshot(&s, &w, "p").unwrap();
    expect(&snap.mark, "210", "1", "210");
    assert_eq!(w, before);
}

#[test]
fn cpmm_stable_fractional_claims_and_native_clmm_rounding_surplus() {
    let mut s = fixture("sc-01");
    enable(&mut s, &["alice"]);
    price(&mut s, "X", 2.into(), 1.into());
    let mut w = World::genesis(&s).unwrap();
    let accounts = Arc::make_mut(&mut w.accounts);
    accounts.get_mut("alice").unwrap().balances.clear();
    accounts.get_mut("pool").unwrap().balances =
        BTreeMap::from([("X".into(), 1102.into()), ("Q".into(), 1099.into())]);
    w.shares.insert("cpmm".into(), 1100.into());
    Arc::make_mut(&mut w.lp_balances)
        .get_mut("cpmm")
        .unwrap()
        .insert("alice".into(), 100.into());
    assert_eq!(
        exact(&v::snapshot(&s, &w, "p").unwrap().mark),
        reference("fractional_cpmm_claim")["mark"]["value"]
    );
    let mut s = fixture("sc-stable");
    enable(&mut s, &["alice"]);
    price(&mut s, "R", 2.into(), 3.into());
    let mut w = World::genesis(&s).unwrap();
    let accounts = Arc::make_mut(&mut w.accounts);
    accounts.get_mut("alice").unwrap().balances.clear();
    accounts.get_mut("stable-account").unwrap().balances =
        BTreeMap::from([("Q".into(), 101.into()), ("R".into(), 99.into())]);
    w.shares.insert("stable".into(), 100.into());
    Arc::make_mut(&mut w.lp_balances)
        .get_mut("stable")
        .unwrap()
        .insert("alice".into(), 7.into());
    assert_eq!(
        exact(&v::snapshot(&s, &w, "p").unwrap().mark),
        reference("fractional_stable_claim")["value"]
    );
    let mut s = fixture("sc-02-clmm");
    enable(&mut s, &["alice"]);
    price(&mut s, "X", 1.into(), 1.into());
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    r.step().unwrap();
    let before = r.checkpoint().unwrap();
    let snap = view(&r).current;
    let mut claims = Exact::default();
    for c in snap.components.iter().filter(|c| c.key.kind == "ClmmClaim") {
        claims = claims.add(c.mark.value.as_ref().unwrap(), 32768).unwrap();
    }
    assert_eq!(
        serde_json::to_value(claims).unwrap(),
        reference("clmm_principal_and_collectable_fees")["mark"]["value"]
    );
    assert_eq!(r.checkpoint().unwrap(), before);
}

#[test]
fn book_remaining_escrow_no_double_count_or_foreign_attribution() {
    for (side, expected) in [
        (book::Side::Sell, "resting_sell_order"),
        (book::Side::Buy, "resting_buy_order"),
    ] {
        let mut s = fixture("sc-08-book");
        enable(&mut s, &["alice"]);
        price(&mut s, "X", 2.into(), 1.into());
        s.blocks.clear();
        set(
            &mut s,
            "alice",
            "X",
            if side == book::Side::Sell { 30 } else { 0 },
        );
        set(
            &mut s,
            "alice",
            "Q",
            if side == book::Side::Buy { 307 } else { 7 },
        );
        let mut t = tx("rest", "alice", "bob", "X", 1, 0);
        t.agent = "alice".into();
        t.limit = 1000.into();
        t.program = vec![Call::PlaceOrder {
            book: "book".into(),
            side,
            price_ticks: 50.into(),
            lots: 3.into(),
            tif: book::TimeInForce::GTC,
        }];
        s.blocks = vec![vec![t]];
        let r = finish(s);
        assert_eq!(r.blocks[0].receipts[0].status, "Success");
        assert_eq!(exact(&view(&r).current.mark), reference(expected)["value"]);
        assert_eq!(
            view(&r)
                .current
                .components
                .iter()
                .filter(|c| c.key.kind == "BookEscrow")
                .count(),
            1
        );
    }
}

#[test]
fn full_self_owned_flash_portfolio_differs_from_wallet_cycle_net() {
    let mut s = fixture("sc-flash-arb-success");
    enable(&mut s, &["alice"]);
    price(&mut s, "X", 1.into(), 1.into());
    let r = finish(s);
    let p = view(&r);
    assert_eq!(
        exact(&p.initial),
        reference("flash_cycle_with_self_owned_pools")["portfolio"]["initial"]
    );
    assert_eq!(
        exact(&p.current.mark),
        reference("flash_cycle_with_self_owned_pools")["portfolio"]["end"]
    );
    assert_eq!(
        exact(&p.pnl),
        reference("flash_cycle_with_self_owned_pools")["portfolio"]["pnl"]
    );
    assert_eq!(
        r.state
            .balance("alice", "Q")
            .checked_sub(r.initial.balance("alice", "Q"))
            .unwrap(),
        24104.into()
    );
    expect(&p.inflows, "0", "1", "0");
}

#[test]
fn wide_values_arithmetic_limit_and_component_budget_preserve_execution() {
    let mut s = simple(&["alice"]);
    s.accounts.get_mut("alice").unwrap().balances =
        BTreeMap::from([("X".into(), Amount::MAX), ("Q".into(), Amount::MAX)]);
    price(&mut s, "X", Amount::MAX, 1.into());
    assert_eq!(
        exact(&view(&finish(s)).current.mark),
        reference("wide_signed_report")["value"]
    );
    let mut s = simple(&["alice"]);
    for (asset, power, subtract) in [("X", 255usize, 19u64), ("Y", 254, 127), ("R", 253, 59)] {
        s.assets.insert(asset.into(), Asset { decimals: 0 });
        set(&mut s, "alice", asset, 1);
        price(
            &mut s,
            asset,
            1.into(),
            Amount::from_big((num_bigint::BigUint::from(1u8) << power) - subtract).unwrap(),
        );
    }
    s.valuation_policy.as_mut().unwrap().limits.rational_bits = 512;
    let r = finish(s);
    let p = view(&r);
    assert!(p.current.mark.reasons.contains("ArithmeticLimit"));
    assert!(p.current.mark.truncated);
    assert!(p.pnl.value.is_none());
    let mut s = fixture("sc-01");
    enable(&mut s, &["alice"]);
    price(&mut s, "X", 1.into(), 1.into());
    s.valuation_policy
        .as_mut()
        .unwrap()
        .limits
        .component_checks_per_portfolio = 1;
    let r = finish(s.clone());
    let p = view(&r);
    assert_eq!(p.current.component_checks, 1);
    assert!(p.current.mark.reasons.contains("ComponentLimit"));
    assert!(p.current.mark.truncated);
    s.valuation_policy = None;
    s.model_versions.retain(|m| m != MODEL);
    let control = finish(s);
    assert_eq!(r.blocks, control.blocks);
    assert_eq!(r.state, control.state);
}

#[test]
fn capital_delivery_uses_actual_block_time_and_visible_bundle_counts_once() {
    let mut s = fixture("sc-scheduler-public");
    enable(&mut s, &["alice"]);
    price(&mut s, "X", 10.into(), 1.into());
    let cfg = s.scheduler.as_mut().unwrap();
    cfg.reactive.clear();
    cfg.rounds = 3;
    let emission = &mut cfg.emissions[0];
    emission.transaction.program = vec![Call::Transfer {
        to: "alice".into(),
        asset: "X".into(),
        amount: 2.into(),
    }];
    emission.delivery.builder_delay = 14.into();
    set(&mut s, "bob", "X", 2);
    label(&mut s, "victim");
    s.valuation_policy
        .as_mut()
        .unwrap()
        .reference_feeds
        .get_mut("X")
        .unwrap()
        .records
        .push(Record {
            available_at: 25.into(),
            observed_at: 25.into(),
            sequence: 1.into(),
            price: Price::new("X".into(), "Q".into(), 15.into(), 1.into()).unwrap(),
        });
    let r = finish(s);
    assert!(v::at(&r, 1).unwrap().capital_flows.is_empty());
    assert!(v::at(&r, 2).unwrap().capital_flows.is_empty());
    let f = &v::at(&r, 3).unwrap().capital_flows[0];
    assert_eq!(f.timestamp, 30.into());
    expect(&f.mark, "30", "1", "30");
    let mut s = fixture("sc-bundle-visible");
    enable(&mut s, &["alice"]);
    price(&mut s, "X", 1.into(), 1.into());
    let cfg = s.scheduler.as_mut().unwrap();
    cfg.builder = scheduler::Builder::FixedItems {
        priority: vec![scheduler::bundles::CandidateId::Bundle {
            id: "visible-group".into(),
        }],
    };
    cfg.emissions[0].transaction.program = vec![Call::Transfer {
        to: "alice".into(),
        asset: "Q".into(),
        amount: 20.into(),
    }];
    let bundle = &mut cfg.bundles.as_mut().unwrap().emissions[0].bundle;
    bundle.members.remove(1);
    let scheduler::bundles::Member::Own { transaction } = &mut bundle.members[1] else {
        panic!()
    };
    transaction.nonce = 0.into();
    label(&mut s, "victim");
    let r = finish(s.clone());
    assert_eq!(
        r.blocks[0]
            .receipts
            .iter()
            .filter(|r| r.tx_id == "victim")
            .count(),
        1
    );
    assert!(
        r.blocks[0]
            .scheduler
            .as_ref()
            .unwrap()
            .probes
            .iter()
            .any(|p| p.selected
                && matches!(&p.candidate, scheduler::bundles::CandidateId::Bundle { .. }))
    );
    assert_eq!(v::at(&r, 1).unwrap().capital_flows.len(), 1);
    expect(&view(&r).inflows, "20", "1", "20");
    expect(&view(&r).pnl, "-5", "1", "-5");
    // A failing group contributes no trial flows; the victim then executes
    // ordinarily, once, if the builder includes it outside the failed group.
    let cfg = s.scheduler.as_mut().unwrap();
    let bundle = &mut cfg.bundles.as_mut().unwrap().emissions[0].bundle;
    let scheduler::bundles::Member::Own { transaction } = &mut bundle.members[1] else {
        panic!()
    };
    transaction.program.push(Call::AssertCondition {
        condition: false,
        message: "late group rollback".into(),
    });
    // Payment contract requires a sole Transfer, so induce failure via nonce.
    transaction.program.pop();
    transaction.nonce = 2.into();
    let r = finish(s);
    let boundary = v::at(&r, 1).unwrap();
    assert!(boundary.capital_flows.len() <= 1);
    expect(&view(&r).pnl, "0", "1", "0");
}

#[test]
fn validation_rejects_ambiguous_capital_models_prices_and_protected_portfolios() {
    let mut base = simple(&["alice"]);
    base.blocks = vec![vec![tx("in", "bob", "alice", "Q", 1, 0)]];
    label(&mut base, "in");
    assert!(base.validate().is_ok());
    type Mutation = Box<dyn Fn(&mut Scenario)>;
    let mutate: Vec<Mutation> = vec![
        Box::new(|s| {
            s.valuation_policy
                .as_mut()
                .unwrap()
                .capital_transfers
                .push(CapitalTransfer {
                    portfolio: "p".into(),
                    tx_id: "in".into(),
                });
        }),
        Box::new(|s| {
            s.valuation_policy.as_mut().unwrap().capital_transfers[0].tx_id = "generated".into();
        }),
        Box::new(|s| {
            s.blocks[0][0].program = vec![Call::Sequence {
                calls: s.blocks[0][0].program.clone(),
            }];
        }),
        Box::new(|s| {
            let Call::Transfer { amount, .. } = &mut s.blocks[0][0].program[0] else {
                panic!()
            };
            *amount = 0.into();
        }),
        Box::new(|s| {
            s.valuation_policy
                .as_mut()
                .unwrap()
                .portfolios
                .get_mut("p")
                .unwrap()
                .accounts
                .push("alice".into());
        }),
        Box::new(|s| {
            s.valuation_policy.as_mut().unwrap().limits.rational_bits = 511;
        }),
        Box::new(|s| {
            s.valuation_policy.as_mut().unwrap().limits.updates_total = 16385;
        }),
        Box::new(|s| {
            s.valuation_policy.as_mut().unwrap().limits.portfolios = 0;
        }),
        Box::new(|s| {
            s.valuation_policy.as_mut().unwrap().numeraire = "X".into();
        }),
        Box::new(|s| {
            s.valuation_policy.as_mut().unwrap().source = "spot".into();
        }),
        Box::new(|s| {
            s.model_versions.retain(|m| m != MODEL);
        }),
        Box::new(|s| {
            s.valuation_policy = None;
        }),
        Box::new(|s| {
            s.blocks.resize(10001, vec![]);
        }),
        Box::new(|s| {
            price(s, "X", 1.into(), 1.into());
            s.valuation_policy
                .as_mut()
                .unwrap()
                .reference_feeds
                .get_mut("X")
                .unwrap()
                .source_id
                .clear();
        }),
        Box::new(|s| {
            price(s, "X", 1.into(), 1.into());
            let f = s
                .valuation_policy
                .as_mut()
                .unwrap()
                .reference_feeds
                .get_mut("X")
                .unwrap();
            f.records.push(f.records[0].clone());
        }),
        Box::new(|s| {
            price(s, "X", 1.into(), 1.into());
            let f = s
                .valuation_policy
                .as_mut()
                .unwrap()
                .reference_feeds
                .remove("X")
                .unwrap();
            s.valuation_policy
                .as_mut()
                .unwrap()
                .reference_feeds
                .insert("Q".into(), f);
        }),
    ];
    for (i, change) in mutate.into_iter().enumerate() {
        let mut s = base.clone();
        change(&mut s);
        assert!(s.validate().is_err(), "mutation {i}");
    }
    let mut s = fixture("sc-01");
    enable(&mut s, &["pool"]);
    assert!(s.validate().is_err());
    let mut s = fixture("sc-bundle-visible");
    enable(&mut s, &["alice"]);
    label(&mut s, "payment");
    assert!(s.validate().is_err());
    // A literal payment target is also forbidden, even when independently emitted.
    let payment = match &s
        .scheduler
        .as_ref()
        .unwrap()
        .bundles
        .as_ref()
        .unwrap()
        .emissions[0]
        .bundle
        .members[2]
    {
        scheduler::bundles::Member::Own { transaction } => *transaction.clone(),
        _ => panic!(),
    };
    let mut emission = s.scheduler.as_ref().unwrap().emissions[0].clone();
    emission.transaction = payment;
    emission.producer_sequence = 1.into();
    s.scheduler.as_mut().unwrap().emissions.push(emission);
    assert!(v::validate(&s).is_err());
    // Duplicate literals are ambiguous even if scheduler permits deduplication.
    let mut s = base;
    let duplicate = s.blocks[0][0].clone();
    s.blocks[0].push(duplicate);
    assert!(v::validate(&s).is_err());
}

#[test]
fn foreign_zero_positions_cannot_evade_component_cap() {
    let mut s = fixture("sc-08-book");
    enable(&mut s, &["alice"]);
    s.valuation_policy
        .as_mut()
        .unwrap()
        .limits
        .component_checks_per_portfolio = 10;
    let mut w = World::genesis(&s).unwrap();
    let book = Arc::make_mut(&mut w.books).get_mut("book").unwrap();
    for i in 1..20u64 {
        book.orders.insert(
            i.into(),
            book::Order {
                owner: "bob".into(),
                side: book::Side::Sell,
                price_ticks: 50.into(),
                remaining_lots: 0.into(),
            },
        );
    }
    let snap = v::snapshot(&s, &w, "p").unwrap();
    assert_eq!(snap.component_checks, 10);
    assert!(snap.mark.reasons.contains("ComponentLimit"));
    assert!(snap.components.iter().all(|c| c.key.kind != "BookEscrow"));
}

#[test]
fn views_reference_changes_and_checkpoint_do_not_interfere_with_agents_rng() {
    let mut s = fixture("sc-agent-random");
    let control = finish(s.clone());
    enable(&mut s, &["alice"]);
    price(&mut s, "X", 1.into(), 2.into());
    let mut r = Run::new(s.clone()).unwrap();
    for boundary in 0..=s.rounds() {
        let before = r.checkpoint().unwrap();
        let report = v::at(&r, boundary).unwrap();
        assert_eq!(r.checkpoint().unwrap(), before);
        let restored = Run::restore(&before).unwrap();
        assert_eq!(v::at(&restored, boundary).unwrap(), report);
        assert_eq!(r.step().unwrap().is_some(), boundary < s.rounds());
    }
    assert_eq!(r.blocks, control.blocks);
    assert_eq!(r.state, control.state);
    assert_eq!(r.scheduler, control.scheduler);
    let mut other = s;
    price(&mut other, "X", Amount::MAX, 1.into());
    let other = finish(other);
    assert_ne!(r.run_id, other.run_id);
    assert_eq!(r.blocks, other.blocks);
    assert_eq!(
        serde_json::to_value(&r).unwrap()["rng"],
        serde_json::to_value(&other).unwrap()["rng"]
    );
    // Rehashed valuation inputs cannot bless edited canonical receipts.
    let mut cp: Value = serde_json::from_slice(&r.checkpoint().unwrap()).unwrap();
    cp["run"]["blocks"][0]["state"]["accounts"]["alice"]["balances"]["Q"] = json!("1");
    cp["hash"] = json!(chainarium_core::run::digest(
        &serde_json::to_vec(&serde_json::from_value::<Run>(cp["run"].clone()).unwrap()).unwrap()
    ));
    assert!(Run::restore(&serde_json::to_vec(&cp).unwrap()).is_err());
}

#[test]
fn exports_stream_every_boundary_and_resume_exactly() {
    let mut s = simple(&["alice"]);
    set(&mut s, "bob", "X", 1);
    price(&mut s, "X", 1.into(), 2.into());
    s.blocks = vec![vec![tx("in", "bob", "alice", "X", 1, 0)], vec![]];
    label(&mut s, "in");
    let full = finish(s.clone());
    let mut half = Run::new(s).unwrap();
    half.step().unwrap();
    let mut resumed = Run::restore(&half.checkpoint().unwrap()).unwrap();
    resumed.finish().unwrap();
    let root = std::env::temp_dir().join(format!(
        "chainarium-valuation-export-{}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    full.export(&root.join("full")).unwrap();
    resumed.export(&root.join("resumed")).unwrap();
    for entry in std::fs::read_dir(root.join("full")).unwrap() {
        let name = entry.unwrap().file_name();
        assert_eq!(
            std::fs::read(root.join("full").join(&name)).unwrap(),
            std::fs::read(root.join("resumed").join(&name)).unwrap(),
            "{name:?}"
        );
    }
    let vals = std::fs::read_to_string(root.join("full/valuations.jsonl")).unwrap();
    let lines: Vec<Value> = vals
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["block"], "0");
    assert_eq!(
        lines[2],
        serde_json::to_value(v::at(&full, 2).unwrap()).unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(root.join("full/capital-flows.jsonl"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("full/manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["valuation_policy"]["model"], MODEL);
    assert!(
        manifest["valuation_policy"]["reference_inputs_sha256"]
            .as_str()
            .unwrap()
            .len()
            == 64
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn an_own_bundle_definition_cannot_shadow_a_literal_capital_transfer() {
    let mut s = fixture("sc-bundle-visible");
    enable(&mut s, &["alice"]);
    s.scheduler.as_mut().unwrap().emissions[0]
        .transaction
        .program = vec![Call::Transfer {
        to: "alice".into(),
        asset: "Q".into(),
        amount: 20.into(),
    }];
    label(&mut s, "victim");
    assert!(s.validate().is_ok());
    let mut alternate = s.scheduler.as_ref().unwrap().emissions[0]
        .transaction
        .clone();
    alternate.agent = "trader".into();
    alternate.sender = "alice".into();
    alternate.program = vec![Call::Transfer {
        to: "bob".into(),
        asset: "Q".into(),
        amount: 900.into(),
    }];
    s.scheduler
        .as_mut()
        .unwrap()
        .bundles
        .as_mut()
        .unwrap()
        .emissions[0]
        .bundle
        .members[0] = scheduler::bundles::Member::Own {
        transaction: Box::new(alternate),
    };
    assert_eq!(
        v::validate(&s).unwrap_err(),
        "ambiguous, duplicate or bid capital binding"
    );
    // Even byte-identical extra definitions are ambiguous input declarations;
    // the supported way to reuse the literal transaction is VisibleTxId.
    let original = s.scheduler.as_ref().unwrap().emissions[0]
        .transaction
        .clone();
    s.scheduler
        .as_mut()
        .unwrap()
        .bundles
        .as_mut()
        .unwrap()
        .emissions[0]
        .bundle
        .members[0] = scheduler::bundles::Member::Own {
        transaction: Box::new(original),
    };
    assert!(v::validate(&s).is_err());
}
