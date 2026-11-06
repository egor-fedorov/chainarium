//! Independent adversarial checks for reviewed revision 976a5cb.
//! Integer expectations below do not invoke simulator quote/math helpers.
use chainarium_core::{policy::liquidator as l, routing::heterogeneous::VenueHop, scheduler, *};

fn fixture(name: &str) -> Scenario {
    Scenario::parse(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../scenarios/demos/sc-liquidator-{name}.json")),
        )
        .unwrap(),
    )
    .unwrap()
}
fn policy(s: &mut Scenario) -> &mut l::Policy {
    s.scheduler
        .as_mut()
        .unwrap()
        .liquidator
        .as_mut()
        .unwrap()
        .policies
        .get_mut("carol")
        .unwrap()
}
fn report(r: &Run) -> &l::Report {
    r.blocks[2]
        .scheduler
        .as_ref()
        .unwrap()
        .liquidators
        .as_ref()
        .unwrap()
}
fn finish(s: Scenario) -> Run {
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    r
}
fn sale() -> Call {
    Call::LiquidateAndSell {
        market: "credit".into(),
        borrower: "bob".into(),
        max_repay: 60.into(),
        min_collateral: 1.into(),
        hops: vec![VenueHop::Amm {
            venue: "sale".into(),
        }],
        min_out: 1.into(),
    }
}
fn tx(program: Vec<Call>) -> Transaction {
    Transaction {
        id: "independent".into(),
        agent: "carol".into(),
        sender: "carol".into(),
        chain: "lab-1".into(),
        nonce: 0.into(),
        limit: 20000.into(),
        price: 0.into(),
        deadline: 100.into(),
        program,
    }
}

#[test]
fn integer_oracle_768_own_flash_capital_close_fee_cases() {
    let base = fixture("own");
    let mut count = 0;
    for flash in [false, true] {
        for capital in [0u64, 60, 500, 1000] {
            for price in [0u64, 1] {
                for close in [1u64, 5000, 10000] {
                    for request in [1u64, 9, 10, 19, 20, 59, 60, 80] {
                        for pool_fee in [0u64, 30] {
                            count += 1;
                            let mut s = base.clone();
                            s.scheduler.as_mut().unwrap().rounds = 3;
                            s.accounts
                                .get_mut("carol")
                                .unwrap()
                                .balances
                                .insert("Q".into(), capital.into());
                            s.accounts
                                .get_mut("carol")
                                .unwrap()
                                .balances
                                .insert("X".into(), 9.into());
                            s.protocols.get_mut("sale").unwrap().fee_bps = pool_fee as u16;
                            s.lending.get_mut("credit").unwrap().close_bps = close as u16;
                            let p = policy(&mut s);
                            p.targets[0].repay_sizes = vec![request.into()];
                            p.resource_price = price.into();
                            p.funding = if flash {
                                l::Funding::Flash {
                                    provider: "flash".into(),
                                }
                            } else {
                                l::Funding::Own
                            };
                            let paid = request.min(80).min((80 * close).div_ceil(10000)).min(60);
                            let seized = 6.min(paid / 10);
                            let effective = seized * (10000 - pool_fee);
                            let output = 1500 * effective / (100 * 10000 + effective);
                            let fee = if flash {
                                (request * 5).div_ceil(10000)
                            } else {
                                0
                            };
                            let units = if flash { 200 } else { 170 };
                            let net =
                                output as i64 - paid as i64 - fee as i64 - (units * price) as i64;
                            let reserve = 500 * price;
                            let executable = capital >= reserve
                                && (flash || capital - reserve >= paid)
                                && seized > 0
                                && output > 0
                                && net >= 1;
                            let r = finish(s);
                            let d = &report(&r).decisions[0];
                            assert_eq!(
                                d.transaction.is_some(),
                                executable,
                                "case {count}: flash={flash}, capital={capital}, price={price}, close={close}, request={request}, fee={pool_fee}"
                            );
                            assert_eq!(d.candidates.len(), 1);
                            assert_eq!(report(&r).metrics.trials, 1);
                            assert_eq!(r.state.balance("carol", "X"), 9.into());
                            if executable {
                                let receipt = &r.blocks[2].receipts[0];
                                let v = receipt.liquidations.values().next().unwrap();
                                assert_eq!(
                                    (v.paid, v.seized, v.route.as_ref().unwrap().output),
                                    (paid.into(), seized.into(), output.into())
                                );
                                assert_eq!(receipt.used, units.into());
                                assert_eq!(d.candidates[0].net, Some((net as u64).into()));
                                assert_eq!(
                                    r.state.balance("carol", "Q"),
                                    (capital + net as u64).into()
                                );
                                assert_eq!(r.state.balance("builder", "Q"), (units * price).into());
                                let loss = if seized == 6 { 80 - paid } else { 0 };
                                assert_eq!(
                                    r.state.lending["credit"].debt,
                                    (80 - paid - loss).into()
                                );
                                assert_eq!(r.state.lending["credit"].claims, (100 - loss).into());
                                assert_eq!(r.state.balance("reserve", "Q"), (20 + paid).into());
                            } else {
                                assert!(r.blocks[2].receipts.is_empty());
                                assert_eq!(r.state.balance("carol", "Q"), capital.into());
                                assert_eq!(r.state.balance("builder", "Q"), 0.into());
                                assert_eq!(r.state.lending["credit"].debt, 80.into());
                                assert!(r.state.lending["credit"].losses.is_empty());
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 768);
}

#[test]
fn every_resource_unit_boundary_restores_sale_writeoff_and_flash() {
    let mut s = fixture("flash");
    s.accounts
        .get_mut("carol")
        .unwrap()
        .balances
        .insert("Q".into(), 100000.into());
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    r.step().unwrap();
    let preview = engine::build_block(&r.scenario, &r.state, &[])
        .unwrap()
        .state;
    for flash in [false, true] {
        for price in [0u64, 1] {
            let required = if flash { 200u64 } else { 170 };
            for limit in 100..=required {
                let mut scaled_sale = sale();
                if let Call::LiquidateAndSell { max_repay, .. } = &mut scaled_sale {
                    *max_repay = 60000.into();
                }
                let call = if flash {
                    Call::FlashLoan {
                        provider: "flash".into(),
                        receiver: "carol".into(),
                        amount: 80000.into(),
                        callback: vec![scaled_sale],
                    }
                } else {
                    scaled_sale
                };
                let mut t = tx(vec![
                    call,
                    Call::AssertTransactionNetGain {
                        asset: "Q".into(),
                        minimum: 1.into(),
                    },
                ]);
                t.limit = limit.into();
                t.price = price.into();
                let block = engine::build_block(&r.scenario, &r.state, &[t]).unwrap();
                let receipt = &block.receipts[0];
                assert_eq!(receipt.used, limit.into());
                assert_eq!(receipt.fee, (limit * price).into());
                assert_eq!(block.state.accounts["carol"].nonce, 1.into());
                assert_eq!(block.state.balance("builder", "Q"), (limit * price).into());
                if limit < required {
                    assert_eq!(receipt.status, "ResourceExhausted");
                    assert_eq!(
                        block.state.balance("carol", "Q"),
                        (100000 - limit * price).into()
                    );
                    assert!(receipt.liquidations.is_empty());
                    assert!(receipt.route_results.is_empty());
                    assert_eq!(block.state.lending, preview.lending);
                    for account in ["reserve", "escrow-bob", "sale", "flash"] {
                        assert_eq!(block.state.accounts[account], r.state.accounts[account]);
                    }
                } else {
                    assert_eq!(receipt.status, "Success");
                }
            }
        }
    }
}

#[test]
fn exact_generated_node_and_depth_caps_cover_flash_children() {
    let s = fixture("own");
    for flash in [false, true] {
        let basic = if flash {
            Call::FlashLoan {
                provider: "flash".into(),
                receiver: "carol".into(),
                amount: 60.into(),
                callback: vec![sale()],
            }
        } else {
            sale()
        };
        let generated_nodes = if flash { 6 } else { 4 };
        let mut program = vec![basic.clone()];
        let mut remaining = 1024 - generated_nodes - 4;
        for _ in 0..4 {
            let width = remaining.min(256);
            remaining -= width;
            program.push(Call::Sequence {
                calls: (0..width)
                    .map(|_| Call::AssertCondition {
                        condition: true,
                        message: "padding".into(),
                    })
                    .collect(),
            });
        }
        assert!(
            s.validate_tx(&tx(program.clone())).is_ok(),
            "flash={flash}: {:?}",
            s.validate_tx(&tx(program.clone()))
        );
        program.push(Call::AssertCondition {
            condition: true,
            message: "over".into(),
        });
        assert!(s.validate_tx(&tx(program)).is_err());
        let mut at_boundary = basic;
        for _ in 0..(if flash { 27 } else { 29 }) {
            at_boundary = Call::Sequence {
                calls: vec![at_boundary],
            };
        }
        assert!(s.validate_tx(&tx(vec![at_boundary.clone()])).is_ok());
        assert!(
            s.validate_tx(&tx(vec![Call::Sequence {
                calls: vec![at_boundary]
            }]))
            .is_err()
        );
    }
}

#[test]
fn fixed_and_greedy_preserve_paid_native_losing_attempts() {
    for greedy in [false, true] {
        let mut s = fixture("fifo");
        if greedy {
            s.model_versions
                .extend(["native-bundles-v1".into(), "bounded-greedy-v1".into()]);
            let c = s.scheduler.as_mut().unwrap();
            c.builder = scheduler::Builder::Greedy;
            c.bundles = Some(scheduler::bundles::Config {
                limits: Default::default(),
                emissions: vec![],
            });
        } else {
            s.scheduler.as_mut().unwrap().builder = scheduler::Builder::Fixed {
                priority: [
                    "supply-100",
                    "borrow-80",
                    "publish-11",
                    "policy:liquidator:carol:3",
                    "policy:liquidator:alice:3",
                ]
                .map(str::to_owned)
                .into(),
            };
        }
        let r = finish(s);
        let receipts = &r.blocks[2].receipts;
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].sender, "carol");
        assert_eq!(receipts[0].used, 170.into());
        assert_eq!(receipts[1].sender, "alice");
        assert_eq!(receipts[1].error.as_deref(), Some("NoDebt"));
        assert_eq!(receipts[1].fee, 120.into());
        assert_eq!(r.state.balance("alice", "Q"), 99880.into());
        assert_eq!(r.state.balance("carol", "Q"), 124565.into());
        assert_eq!(r.state.balance("builder", "Q"), 460.into());
    }
}

#[test]
fn u256_request_and_minimum_overflow_are_protocol_failures_or_capped_actual_payment() {
    let max: Amount =
        "115792089237316195423570985008687907853269984665640564039457584007913129639935"
            .parse()
            .unwrap();
    for flash in [false, true] {
        for huge_minimum in [false, true] {
            let mut s = fixture("own");
            let p = policy(&mut s);
            p.targets[0].repay_sizes = vec![max];
            if flash {
                p.funding = l::Funding::Flash {
                    provider: "flash".into(),
                };
            }
            if huge_minimum {
                p.minimum_net_profit = max;
            }
            let r = finish(s);
            let d = &report(&r).decisions[0];
            assert_eq!(d.transaction.is_some(), !flash && !huge_minimum);
            assert_eq!(
                d.candidates[0].outcome,
                if flash || huge_minimum {
                    "ProtocolRevert"
                } else {
                    "Success"
                }
            );
            if !flash && !huge_minimum {
                assert_eq!(
                    d.candidates[0].receipt.as_ref().unwrap().liquidations["0"].paid,
                    60.into()
                );
            }
        }
    }
}

#[test]
fn all_six_demos_restore_every_boundary_and_read_only_inspection_preserves_rng() {
    for name in ["own", "flash", "fifo", "fee", "stale", "delayed"] {
        let mut r = Run::new(fixture(name)).unwrap();
        while r.step().unwrap().is_some() {
            let bytes = r.checkpoint().unwrap();
            let restored = Run::restore(&bytes).unwrap();
            assert_eq!(bytes, restored.checkpoint().unwrap());
            let _ = routing::Observation::capture(&r.scenario, &r.state, "carol").unwrap();
            let _ = credit::views(&r.scenario, &r.state);
            assert_eq!(bytes, r.checkpoint().unwrap());
        }
    }
}

#[test]
fn deterministic_dfs_and_selection_prefer_net_then_work_market_request_path() {
    for duplicate_direct in [false, true] {
        let mut s = fixture("own");
        s.scheduler.as_mut().unwrap().rounds = 3;
        s.assets.insert("R".into(), Asset { decimals: 0 });
        for (id, x, y, rx, ry) in [
            ("a-hop", "X", "R", 100u64, 100u64),
            ("b-hop", "R", "Q", 100, 1764),
        ] {
            s.accounts.insert(
                id.into(),
                Account {
                    nonce: 0.into(),
                    balances: [(x.into(), rx.into()), (y.into(), ry.into())].into(),
                },
            );
            s.protocols.insert(
                id.into(),
                Pool {
                    account: id.into(),
                    x: x.into(),
                    y: y.into(),
                    fee_bps: 0,
                    owner: "lender".into(),
                },
            );
        }
        if duplicate_direct {
            s.accounts
                .insert("c-direct".into(), s.accounts["sale"].clone());
            let mut pool = s.protocols["sale"].clone();
            pool.account = "c-direct".into();
            s.protocols.insert("c-direct".into(), pool);
        }
        let mut second = s.lending["credit"].clone();
        second.cash_account = "reserve2".into();
        second.positions.insert("bob".into(), "escrow2".into());
        s.lending.insert("aaa-credit".into(), second);
        s.accounts
            .insert("reserve2".into(), s.accounts["reserve"].clone());
        s.accounts
            .insert("escrow2".into(), s.accounts["escrow-bob"].clone());
        s.accounts
            .get_mut("bob")
            .unwrap()
            .balances
            .insert("X".into(), 12.into());
        s.accounts
            .get_mut("lender")
            .unwrap()
            .balances
            .insert("Q".into(), 200.into());
        let c = s.scheduler.as_mut().unwrap();
        c.emissions[0].transaction.program.push(Call::Supply {
            market: "aaa-credit".into(),
            amount: 100.into(),
            min_shares: 100.into(),
        });
        c.emissions[1].transaction.program.extend([
            Call::DepositCollateral {
                market: "aaa-credit".into(),
                amount: 6.into(),
            },
            Call::Borrow {
                market: "aaa-credit".into(),
                amount: 80.into(),
            },
        ]);
        let p = policy(&mut s);
        p.targets[0].repay_sizes = vec![80.into(), 60.into()];
        p.targets.push(l::Target {
            market: "aaa-credit".into(),
            borrower: "bob".into(),
            repay_sizes: vec![80.into(), 60.into()],
        });
        let r = finish(s.clone());
        let d = &report(&r).decisions[0];
        let best = &d.candidates[d.selected.unwrap()];
        assert_eq!(best.market, "aaa-credit");
        assert_eq!(best.repayment, 60.into());
        assert_eq!(best.net, Some(24.into()));
        assert_eq!(best.receipt.as_ref().unwrap().used, 170.into());
        assert_eq!(
            best.hops[0].venue(),
            if duplicate_direct { "c-direct" } else { "sale" }
        );
        assert_eq!(
            d.candidates[0]
                .hops
                .iter()
                .map(VenueHop::venue)
                .collect::<Vec<_>>(),
            ["a-hop", "b-hop"]
        );
        assert_eq!(d.candidates[0].receipt.as_ref().unwrap().used, 190.into());
        assert_eq!(d.candidates[0].net, Some(24.into()));
        policy(&mut s).targets.reverse();
        for target in &mut policy(&mut s).targets {
            target.repay_sizes.reverse();
        }
        let permuted = finish(s);
        assert_eq!(report(&r), report(&permuted));
        assert_eq!(r.state, permuted.state);
    }
}
