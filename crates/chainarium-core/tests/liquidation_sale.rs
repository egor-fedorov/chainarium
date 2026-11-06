use chainarium_core::{routing::heterogeneous::VenueHop, *};
fn fixture(scale: u64) -> Scenario {
    let mut s =
        Scenario::parse(include_bytes!("../../../scenarios/demos/sc-05-credit.json")).unwrap();
    for m in [
        liquidation::MODEL,
        routing::heterogeneous::MODEL,
        "strict-flash-provider-v1",
    ] {
        if !s.model_versions.iter().any(|x| x == m) {
            s.model_versions.push(m.into());
        }
    }
    for a in s.accounts.values_mut() {
        for value in a.balances.values_mut() {
            *value = value.checked_mul(scale.into()).unwrap();
        }
    }
    let cfg = s.lending.get_mut("credit").unwrap();
    cfg.rate = credit::Rate::Constant { rate: Amount::ZERO };
    cfg.supply_cap = cfg.supply_cap.checked_mul(scale.into()).unwrap();
    cfg.borrow_cap = cfg.borrow_cap.checked_mul(scale.into()).unwrap();
    for tx in &mut s.blocks[0] {
        for call in &mut tx.program {
            match call {
                Call::Supply {
                    amount, min_shares, ..
                } => {
                    *amount = amount.checked_mul(scale.into()).unwrap();
                    *min_shares = min_shares.checked_mul(scale.into()).unwrap();
                }
                Call::DepositCollateral { amount, .. } | Call::Borrow { amount, .. } => {
                    *amount = amount.checked_mul(scale.into()).unwrap()
                }
                _ => {}
            }
        }
    }
    s.accounts.insert(
        "sale".into(),
        Account {
            nonce: Amount::ZERO,
            balances: [
                ("X".into(), (100 * scale).into()),
                ("Q".into(), (1500 * scale).into()),
            ]
            .into(),
        },
    );
    s.protocols.insert(
        "sale".into(),
        Pool {
            account: "sale".into(),
            x: "X".into(),
            y: "Q".into(),
            fee_bps: 0,
            owner: "alice".into(),
        },
    );
    s.accounts.insert(
        "flash".into(),
        Account {
            nonce: Amount::ZERO,
            balances: [("Q".into(), (10000 * scale).into())].into(),
        },
    );
    s.flash_providers.insert(
        "flash".into(),
        FlashProvider {
            account: "flash".into(),
            asset: "Q".into(),
            fee_bps: 5,
        },
    );
    s.blocks.truncate(2);
    s.blocks[1].truncate(1);
    s
}
fn sale(amount: u64) -> Call {
    Call::LiquidateAndSell {
        market: "credit".into(),
        borrower: "bob".into(),
        max_repay: amount.into(),
        min_collateral: 1.into(),
        hops: vec![VenueHop::Amm {
            venue: "sale".into(),
        }],
        min_out: 1.into(),
    }
}
fn transaction(call: Call, price: u64) -> Transaction {
    Transaction {
        id: "liquidator".into(),
        agent: "carol".into(),
        sender: "carol".into(),
        chain: "lab-1".into(),
        nonce: Amount::ZERO,
        limit: 500.into(),
        price: price.into(),
        deadline: 100.into(),
        program: vec![
            call,
            Call::AssertTransactionNetGain {
                asset: "Q".into(),
                minimum: 1.into(),
            },
        ],
    }
}
fn finish(mut s: Scenario, tx: Transaction) -> Run {
    s.blocks[1].push(tx);
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    r
}
#[test]
fn own_sale_uses_actual_seizure_and_preserves_inventory() {
    let mut s = fixture(1);
    s.accounts
        .get_mut("carol")
        .unwrap()
        .balances
        .insert("X".into(), 9.into());
    let r = finish(s, transaction(sale(60), 0));
    let receipt = &r.blocks[1].receipts[1];
    assert_eq!(receipt.status, "Success");
    assert_eq!(receipt.used, 170.into());
    assert_eq!(r.state.balance("carol", "Q"), 1024.into());
    assert_eq!(r.state.balance("carol", "X"), 9.into());
    let result = &receipt.liquidations["0"];
    assert_eq!((result.paid, result.seized), (60.into(), 6.into()));
    assert_eq!(result.route.as_ref().unwrap().output, 84.into());
    assert_eq!(result.oracle.as_ref().unwrap().status, "Fresh");
    let l = &r.state.lending["credit"];
    assert_eq!(
        (l.debt, l.claims, l.equity),
        (Amount::ZERO, 80.into(), Amount::ZERO)
    );
    assert_eq!(l.losses[0].amount, 20.into());
}
#[test]
fn prior_partial_liquidation_changes_actual_sale_budget_without_using_own_inventory() {
    let mut s = fixture(1);
    s.accounts
        .get_mut("carol")
        .unwrap()
        .balances
        .insert("X".into(), 9.into());
    let mut earlier = transaction(
        Call::Liquidate {
            market: "credit".into(),
            borrower: "bob".into(),
            amount: 20.into(),
            min_collateral: 2.into(),
        },
        0,
    );
    earlier.id = "partial".into();
    earlier.program.truncate(1);
    s.blocks[1].push(earlier);
    let mut tx = transaction(sale(60), 0);
    tx.nonce = 1.into();
    let r = finish(s, tx);
    let result = &r.blocks[1].receipts[2].liquidations["0"];
    assert_eq!((result.paid, result.seized), (40.into(), 4.into()));
    assert_eq!(result.route.as_ref().unwrap().output, 57.into());
    // The earlier transaction's 2 X are also existing inventory for the wrapper.
    assert_eq!(r.state.balance("carol", "X"), 11.into());
    assert_eq!(r.state.balance("carol", "Q"), 997.into());
}
#[test]
fn flash_wrap_has_metered_callback_and_settles_principal_fee_before_guard() {
    let mut s = fixture(1000);
    s.accounts
        .get_mut("carol")
        .unwrap()
        .balances
        .insert("Q".into(), 1000.into());
    let call = Call::FlashLoan {
        provider: "flash".into(),
        receiver: "carol".into(),
        amount: 60000.into(),
        callback: vec![sale(60000)],
    };
    let r = finish(s, transaction(call, 1));
    let receipt = &r.blocks[1].receipts[1];
    assert_eq!(receipt.status, "Success");
    assert_eq!(receipt.used, 200.into());
    assert_eq!(r.state.balance("carol", "Q"), 25675.into());
    assert_eq!(r.state.balance("builder", "Q"), 200.into());
    assert_eq!(r.state.balance("flash", "Q"), 10000030u64.into());
    assert_eq!(
        receipt.liquidations["0.callback.0"]
            .route
            .as_ref()
            .unwrap()
            .output,
        84905.into()
    );
}
#[test]
fn late_net_guard_reverts_sale_liquidation_and_loss_but_preserves_native_fee() {
    let r = finish(fixture(1), transaction(sale(60), 1));
    let receipt = &r.blocks[1].receipts[1];
    assert_eq!(receipt.status, "ProtocolRevert");
    assert_eq!(receipt.fee, 170.into());
    assert!(receipt.liquidations.is_empty());
    assert!(receipt.route_results.is_empty());
    assert_eq!(r.state.balance("carol", "Q"), 830.into());
    assert_eq!(r.state.balance("escrow-bob", "X"), 6.into());
    assert_eq!(r.state.balance("sale", "X"), 100.into());
    assert_eq!(r.state.lending["credit"].debt, 80.into());
    assert!(r.state.lending["credit"].losses.is_empty());
    assert!(
        receipt
            .trace
            .iter()
            .any(|t| t.liquidation.is_some() && t.outcome == "RolledBack")
    );
}
#[test]
fn exhausted_wrapper_rolls_back_and_exact_unit_boundary_succeeds() {
    for (flash, needed) in [(false, 170), (true, 200)] {
        for limit in [needed - 1, needed] {
            let call = if flash {
                Call::FlashLoan {
                    provider: "flash".into(),
                    receiver: "carol".into(),
                    amount: 60.into(),
                    callback: vec![sale(60)],
                }
            } else {
                sale(60)
            };
            let mut tx = transaction(call, 0);
            tx.limit = limit.into();
            let r = finish(fixture(1), tx);
            let receipt = &r.blocks[1].receipts[1];
            assert_eq!(
                receipt.status,
                if limit == needed {
                    "Success"
                } else {
                    "ResourceExhausted"
                }
            );
            assert_eq!(receipt.used, limit.into());
            assert_eq!(receipt.liquidations.is_empty(), limit < needed);
        }
    }
}
#[test]
fn no_debt_competitor_pays_only_entered_calls() {
    let mut s = fixture(1);
    let mut first = transaction(sale(60), 0);
    first.id = "winner".into();
    s.blocks[1].push(first);
    let mut loser = transaction(sale(60), 2);
    loser.nonce = 1.into();
    let r = finish(s, loser);
    let receipt = &r.blocks[1].receipts[2];
    assert_eq!(receipt.status, "ProtocolRevert");
    assert_eq!(receipt.error.as_deref(), Some("NoDebt"));
    assert_eq!(receipt.used, 120.into());
    assert_eq!(receipt.fee, 240.into());
}
#[test]
fn model_and_generated_child_depth_are_validated_before_execution() {
    let mut s = fixture(1);
    let tx = transaction(sale(60), 0);
    s.model_versions.retain(|m| m != liquidation::MODEL);
    assert!(s.validate_tx(&tx).is_err());
    let s = fixture(1);
    let mut call = sale(60);
    for _ in 0..30 {
        call = Call::Sequence { calls: vec![call] };
    }
    assert!(s.validate_tx(&transaction(call, 0)).is_err());
}

#[test]
fn sale_minimum_or_flash_settlement_failure_reverts_writeoff_and_all_transfers() {
    for flash in [false, true] {
        let mut s = fixture(1);
        let mut call = sale(60);
        if flash {
            s.flash_providers.get_mut("flash").unwrap().fee_bps = 9999;
            s.accounts
                .get_mut("carol")
                .unwrap()
                .balances
                .insert("Q".into(), 0.into());
            call = Call::FlashLoan {
                provider: "flash".into(),
                receiver: "carol".into(),
                amount: 60.into(),
                callback: vec![call],
            };
        } else if let Call::LiquidateAndSell { min_out, .. } = &mut call {
            *min_out = 85.into();
        }
        let r = finish(s, transaction(call, 0));
        let receipt = &r.blocks[1].receipts[1];
        assert_eq!(receipt.status, "ProtocolRevert");
        assert!(receipt.liquidations.is_empty());
        assert!(receipt.route_results.is_empty());
        assert_eq!(r.state.balance("sale", "Q"), 1500.into());
        assert_eq!(r.state.balance("flash", "Q"), 10000.into());
        assert_eq!(r.state.balance("escrow-bob", "X"), 6.into());
        assert_eq!(r.state.lending["credit"].debt, 80.into());
        assert!(r.state.lending["credit"].losses.is_empty());
    }
}
#[test]
fn stable_route_charges_actual_solver_work_and_fee() {
    let mut s = fixture(1000);
    s.model_versions.push(stable::MODEL.into());
    let mut pool = s.protocols.remove("sale").unwrap();
    pool.fee_bps = 4;
    s.stable_pools.insert(
        "sale".into(),
        stable::StablePool {
            pool,
            amplification: 100,
            iteration_limit: 255,
        },
    );
    let mut tx = transaction(sale(60000), 1);
    tx.program.truncate(1);
    let r = finish(s, tx);
    let receipt = &r.blocks[1].receipts[1];
    assert_eq!(receipt.status, "Success");
    let work = receipt.trace.iter().map(|t| t.solver.len()).sum::<usize>();
    assert!(work > 0);
    assert_eq!(receipt.used, Amount::from(160 + work as u64));
    assert_eq!(receipt.fee, receipt.used);
    assert_eq!(r.state.balance("builder", "Q"), receipt.fee);
    assert_eq!(
        receipt.liquidations["0"]
            .route
            .as_ref()
            .unwrap()
            .input_spent,
        6000.into()
    );
}
#[test]
fn book_partial_fill_reverts_liquidation_writeoff_and_matching() {
    let mut s = fixture(1);
    s.model_versions.push(book::MODEL.into());
    s.accounts.insert(
        "book".into(),
        Account {
            nonce: Amount::ZERO,
            balances: Default::default(),
        },
    );
    s.books.insert(
        "book".into(),
        book::Config {
            account: "book".into(),
            base_asset: "X".into(),
            quote_asset: "Q".into(),
            lot_atoms: 1.into(),
            quote_tick_atoms: 1.into(),
            taker_fee_bps: 100,
            fee_account: "builder".into(),
        },
    );
    let mut maker = transaction(
        Call::PlaceOrder {
            book: "book".into(),
            side: book::Side::Buy,
            price_ticks: 15.into(),
            lots: 4.into(),
            tif: book::TimeInForce::GTC,
        },
        0,
    );
    maker.agent = "bob".into();
    maker.sender = "bob".into();
    maker.nonce = 1.into();
    maker.id = "partial-bid".into();
    maker.program.truncate(1);
    s.blocks[0].push(maker);
    let mut call = sale(60);
    if let Call::LiquidateAndSell { hops, .. } = &mut call {
        *hops = vec![VenueHop::Book {
            venue: "book".into(),
            limit_price_ticks: 15.into(),
        }];
    }
    let r = finish(s, transaction(call, 0));
    let receipt = &r.blocks[1].receipts[1];
    assert_eq!(receipt.status, "ProtocolRevert");
    assert_eq!(r.state.books, r.blocks[0].state.books);
    assert_eq!(r.state.balance("book", "Q"), 60.into());
    assert_eq!(r.state.balance("escrow-bob", "X"), 6.into());
    assert!(r.state.lending["credit"].losses.is_empty());
    let result = receipt
        .trace
        .iter()
        .find_map(|t| t.liquidation.as_ref())
        .unwrap();
    assert_eq!(result.seized, 6.into());
    assert!(result.route.as_ref().unwrap().input_unspent > Amount::ZERO);
}
#[test]
fn clmm_limit_partial_fill_rolls_back_range_state_and_credit_loss() {
    let mut s = fixture(1000);
    s.model_versions.push(clmm::MODEL.into());
    s.accounts.insert(
        "clmm".into(),
        Account {
            nonce: Amount::ZERO,
            balances: Default::default(),
        },
    );
    s.clmm_pools.insert(
        "clmm".into(),
        clmm::Config {
            account: "clmm".into(),
            x: "X".into(),
            q: "Q".into(),
            fee_bps: 30,
            initial_sqrt: clmm::math::tick_sqrt(0).unwrap(),
        },
    );
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("X".into(), 10000.into());
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), 200000.into());
    let mut mint = transaction(
        Call::MintPosition {
            pool: "clmm".into(),
            lower: -100,
            upper: 100,
            liquidity: 1000000.into(),
            max_x: 4988.into(),
            max_q: 4988.into(),
        },
        0,
    );
    mint.agent = "alice".into();
    mint.sender = "alice".into();
    mint.nonce = 1.into();
    mint.id = "mint-range".into();
    mint.program.truncate(1);
    s.blocks[0].push(mint);
    let mut call = sale(60000);
    if let Call::LiquidateAndSell { hops, .. } = &mut call {
        *hops = vec![VenueHop::Clmm {
            venue: "clmm".into(),
            limit_tick: -100,
        }];
    }
    let r = finish(s, transaction(call, 0));
    let receipt = &r.blocks[1].receipts[1];
    assert_eq!(receipt.status, "ProtocolRevert");
    assert_eq!(r.state.clmm, r.blocks[0].state.clmm);
    assert_eq!(r.state.balance("escrow-bob", "X"), 6000.into());
    assert_eq!(r.state.lending["credit"].debt, 80000.into());
    assert!(r.state.lending["credit"].losses.is_empty());
    let result = receipt
        .trace
        .iter()
        .find_map(|t| t.liquidation.as_ref())
        .unwrap();
    assert!(result.route.as_ref().unwrap().input_unspent > Amount::ZERO);
}
#[test]
fn path_units_and_flash_receiver_cannot_change_authority() {
    let mut s = fixture(1);
    let mut call = sale(60);
    if let Call::LiquidateAndSell { hops, .. } = &mut call {
        hops.push(hops[0].clone());
    }
    assert!(s.validate_tx(&transaction(call, 0)).is_err());
    let mut call = sale(60);
    if let Call::LiquidateAndSell { borrower, .. } = &mut call {
        *borrower = "alice".into();
    }
    assert!(s.validate_tx(&transaction(call, 0)).is_err());
    let tx = transaction(
        Call::FlashLoan {
            provider: "flash".into(),
            receiver: "bob".into(),
            amount: 60.into(),
            callback: vec![sale(60)],
        },
        0,
    );
    s.blocks[1].push(tx);
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(r.blocks[1].receipts[1].status, "ProtocolRevert");
    assert_eq!(r.state.lending["credit"].debt, 80.into());
}
