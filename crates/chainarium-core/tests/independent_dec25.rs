//! Independent DEC-25 review, pinned to d4a4709. Only public execution APIs.
//! Expected economics use u128 arithmetic and the accepted cost schedule.
use chainarium_core::{
    hooks::{Instruction as I, Point as P},
    routing::heterogeneous::{self as h, ResidualPolicy},
    *,
};

fn fixture(name: &str) -> Scenario {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../scenarios/demos/sc-hook-{name}.json"));
    Scenario::parse(&std::fs::read(path).unwrap()).unwrap()
}
fn execute(s: &Scenario) -> Block {
    let w = World::genesis(s).unwrap();
    let old = w.clone();
    let b = engine::build_block(s, &w, &s.blocks[0]).unwrap();
    assert_eq!(w, old, "execution must not mutate supplied parent");
    b.state.invariant(s).unwrap();
    assert_eq!(b.receipts.len(), 1, "{:?}", b.rejected);
    b
}
fn set_balance(s: &mut Scenario, who: &str, asset: &str, n: u64) {
    s.accounts
        .get_mut(who)
        .unwrap()
        .balances
        .insert(asset.into(), n.into());
}
fn swap(asset: &str, n: u64, minimum: u64) -> Call {
    Call::SwapExactIn {
        pool: "cpmm".into(),
        asset_in: asset.into(),
        amount: n.into(),
        min_out: minimum.into(),
    }
}
fn expected_output(x: u64, y: u64, n: u64, bps: u16) -> u64 {
    let e = (n as u128) * u128::from(10000 - bps);
    ((y as u128) * e / ((x as u128) * 10000 + e)) as u64
}
fn assert_rollback(s: &Scenario, b: &Block) {
    let r = &b.receipts[0];
    for (id, a) in &s.accounts {
        for (asset, n) in &a.balances {
            let expected = if id == "alice" && asset == "Q" {
                n.checked_sub(r.fee).unwrap()
            } else if id == &s.chain.fee_account && asset == "Q" {
                n.checked_add(r.fee).unwrap()
            } else {
                *n
            };
            assert_eq!(b.state.balance(id, asset), expected, "{id}/{asset}");
        }
    }
    assert_eq!(b.state.hooks["guard"].counter, Amount::ZERO);
    assert_eq!(b.state.accounts["alice"].nonce, 1.into());
    assert!(r.events.is_empty());
    assert!(r.route_results.is_empty());
    assert!(r.trace.iter().all(|t| t.outcome != "Success"));
}

#[test]
fn independent_integer_swap_matrix_2110_executions() {
    let mut runs = 0;
    for direction in ["X", "Q"] {
        for fee in [0u16, 1, 30, 1000, 9999] {
            for n in 0u64..=210 {
                let mut s = fixture("fee");
                s.protocols.get_mut("cpmm").unwrap().fee_bps = 37;
                set_balance(&mut s, "alice", "X", 10000);
                set_balance(&mut s, "alice", "Q", 10000);
                s.blocks[0][0].program = vec![swap(direction, n, 1)];
                let cfg = s.hooks.get_mut("guard").unwrap();
                cfg.programs.get_mut(&P::BeforeSwap).unwrap()[0] = I::FeeByInput {
                    asset_in: "X".into(),
                    threshold: 100.into(),
                    below_bps: fee / 2,
                    at_or_above_bps: fee,
                };
                let used_fee = if direction == "Q" {
                    37
                } else if n < 100 {
                    fee / 2
                } else {
                    fee
                };
                let out = expected_output(1000, 1000, n, used_fee);
                let b = execute(&s);
                let r = &b.receipts[0];
                if direction == "X" && n > 200 {
                    assert_eq!(r.error.as_deref(), Some("HookInputLimit"));
                    assert_eq!(r.used, 140.into());
                    assert_rollback(&s, &b);
                } else if out == 0 {
                    assert_eq!(r.status, "ProtocolRevert");
                    assert_eq!(r.used, 150.into());
                    assert_rollback(&s, &b);
                } else {
                    assert_eq!(r.status, "Success");
                    assert_eq!(r.used, 195.into());
                    let (x, q) = if direction == "X" {
                        (10000 - n, 10000 + out + 3 - 195)
                    } else {
                        (10000 + out, 10000 - n + 3 - 195)
                    };
                    assert_eq!(b.state.balance("alice", "X"), x.into());
                    assert_eq!(b.state.balance("alice", "Q"), q.into());
                    assert_eq!(b.state.balance("pool", direction), (1000 + n).into());
                    assert_eq!(
                        b.state
                            .balance("pool", if direction == "X" { "Q" } else { "X" }),
                        (1000 - out).into()
                    );
                    assert_eq!(b.state.balance("hook", "Q"), 17.into());
                    assert_eq!(b.state.balance("builder", "Q"), 195.into());
                    assert_eq!(b.state.hooks["guard"].counter, 2.into());
                    let points: Vec<_> = r
                        .trace
                        .iter()
                        .filter_map(|t| t.hook.as_ref())
                        .filter(|h| h.instruction.is_none())
                        .collect();
                    assert_eq!(points.len(), 2);
                    assert_eq!(
                        points[0].observation.as_ref().unwrap().reserves,
                        [1000.into(); 2]
                    );
                    assert_eq!(
                        points[1].observation.as_ref().unwrap().reserves,
                        [b.state.balance("pool", "X"), b.state.balance("pool", "Q")]
                    );
                }
                runs += 1;
            }
        }
    }
    assert_eq!(runs, 2110);
}

#[test]
fn before_after_payout_separation_in_two_hop_routes_30_executions() {
    for point in [P::BeforeSwap, P::AfterSwap] {
        for asset in ["X", "Q", "R"] {
            for reward in [1u64, 3, 82, 100, 300] {
                let mut s = fixture("route");
                set_balance(&mut s, "hook", asset, reward);
                let cfg = s.hooks.get_mut("guard").unwrap();
                cfg.capabilities.spend_own = [(asset.into(), reward.into())].into();
                cfg.programs = [(
                    point,
                    vec![I::PaySender {
                        asset: asset.into(),
                        amount: reward.into(),
                    }],
                )]
                .into();
                let b = execute(&s);
                let r = &b.receipts[0];
                assert_eq!(r.status, "Success");
                // Base fee is zero after replacing the dynamic program: 100 X -> 90 Q -> 82 R.
                let route = &r.route_results["0"];
                assert_eq!(route.input_spent, 100.into());
                assert_eq!(route.hops[0].output, 90.into());
                assert_eq!(route.hops[1].budget, 90.into());
                assert_eq!(route.output, 82.into());
                assert!(route.residuals.is_empty());
                assert_eq!(r.used, 175.into());
                let initial = &s.accounts["alice"].balances;
                for token in ["X", "Q", "R"] {
                    let mut value = initial.get(token).copied().unwrap_or_default();
                    if token == "X" {
                        value = value.checked_sub(100.into()).unwrap();
                    }
                    if token == "Q" {
                        value = value.checked_sub(175.into()).unwrap();
                    }
                    if token == "R" {
                        value = value.checked_add(82.into()).unwrap();
                    }
                    if token == asset {
                        value = value.checked_add(reward.into()).unwrap();
                    }
                    assert_eq!(b.state.balance("alice", token), value);
                }
            }
        }
    }
}

#[test]
fn asymmetric_lp_caps_and_rollback_968_executions() {
    for x in 0u64..=21 {
        for q in 0u64..=21 {
            for late in [false, true] {
                let mut s = fixture("lp");
                s.blocks.truncate(1);
                s.blocks[0][0].program = vec![Call::AddLiquidity {
                    pool: "cpmm".into(),
                    max_x: x.into(),
                    max_y: q.into(),
                    min_shares: 1.into(),
                }];
                if late {
                    s.blocks[0][0].program.push(Call::AssertCondition {
                        condition: false,
                        message: "review late failure".into(),
                    });
                }
                let b = execute(&s);
                let r = &b.receipts[0];
                let minted = x.min(q);
                if minted == 0 || late {
                    assert_eq!(r.status, "ProtocolRevert");
                    assert_eq!(r.used, if minted == 0 { 130.into() } else { 185.into() });
                    assert_rollback(&s, &b);
                    assert_eq!(b.state.shares["cpmm"], 1000.into());
                } else {
                    assert_eq!(r.status, "Success");
                    assert_eq!(r.used, 175.into());
                    assert_eq!(b.state.balance("alice", "X"), (1000 - minted).into());
                    assert_eq!(
                        b.state.balance("alice", "Q"),
                        (1000 - minted + 3 - 175).into()
                    );
                    assert_eq!(b.state.shares["cpmm"], (1000 + minted).into());
                    assert_eq!(b.state.lp_balances["cpmm"]["alice"], minted.into());
                }
            }
        }
    }
}

#[test]
fn charges_precede_payout_counter_and_rejection_424_executions() {
    for point in [P::BeforeSwap, P::AfterSwap] {
        for reject in [false, true] {
            for limit in 100u64..=205 {
                let mut s = fixture("fee");
                let cfg = s.hooks.get_mut("guard").unwrap();
                let mut program = vec![
                    I::PaySender {
                        asset: "Q".into(),
                        amount: 3.into(),
                    },
                    I::IncrementCounter,
                ];
                if reject {
                    program.push(I::Reject {
                        reason: "intentional".into(),
                    });
                }
                cfg.programs = [(point, program)].into();
                s.blocks[0][0].limit = limit.into();
                let natural_cost = if reject {
                    if point == P::BeforeSwap { 155 } else { 165 }
                } else {
                    155
                };
                let b = execute(&s);
                let r = &b.receipts[0];
                assert_eq!(r.used, limit.min(natural_cost).into());
                if limit < natural_cost || reject {
                    assert_eq!(
                        r.status,
                        if limit < natural_cost {
                            "ResourceExhausted"
                        } else {
                            "ProtocolRevert"
                        }
                    );
                    assert_rollback(&s, &b);
                } else {
                    assert_eq!(r.status, "Success");
                    assert_eq!(b.state.balance("alice", "Q"), 938.into());
                    assert_eq!(b.state.hooks["guard"].counter, 1.into());
                    assert_eq!(b.state.balance("hook", "Q"), 17.into());
                }
            }
        }
    }
}

#[test]
fn per_point_per_asset_caps_and_counter_sequence_3_executions() {
    let mut s = fixture("fee");
    set_balance(&mut s, "hook", "X", 20);
    let cfg = s.hooks.get_mut("guard").unwrap();
    cfg.capabilities.spend_own = [("X".into(), 3.into()), ("Q".into(), 3.into())].into();
    let program = vec![
        I::PaySender {
            asset: "X".into(),
            amount: 1.into(),
        },
        I::PaySender {
            asset: "Q".into(),
            amount: 2.into(),
        },
        I::IncrementCounter,
        I::PaySender {
            asset: "X".into(),
            amount: 2.into(),
        },
        I::PaySender {
            asset: "Q".into(),
            amount: 1.into(),
        },
        I::IncrementCounter,
    ];
    cfg.programs = [(P::BeforeSwap, program.clone()), (P::AfterSwap, program)].into();
    let b = execute(&s);
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(b.receipts[0].used, 300.into());
    assert_eq!(b.state.hooks["guard"].counter, 4.into());
    assert_eq!(b.state.balance("hook", "X"), 14.into());
    assert_eq!(b.state.balance("hook", "Q"), 14.into());
    assert_eq!(b.state.balance("alice", "X"), 906.into());
    assert_eq!(b.state.balance("alice", "Q"), 796.into());
    for asset in ["X", "Q"] {
        let mut bad = s.clone();
        bad.hooks
            .get_mut("guard")
            .unwrap()
            .capabilities
            .spend_own
            .insert(asset.into(), 2.into());
        assert!(bad.validate().is_err());
        // Defensive runtime rejection after mutating a formerly validated configuration.
        let w = World::genesis(&s).unwrap();
        let b = engine::build_block(&bad, &w, &bad.blocks[0]).unwrap();
        assert_eq!(b.receipts[0].status, "ProtocolRevert");
        assert_rollback(&bad, &b);
    }
}

#[test]
fn generated_nodes_exactly_match_committed_trace_7_executions() {
    let mut s = fixture("fee");
    set_balance(&mut s, "alice", "Q", 100000);
    set_balance(&mut s, "alice", "X", 100000);
    set_balance(&mut s, "hook", "Q", 100000);
    s.chain.block_limit = 100000.into();
    let mut tx = s.blocks[0][0].clone();
    tx.limit = 100000.into();
    tx.price = 0.into();
    // Only counters: each swap has one own node + two entry and two instruction nodes.
    s.hooks.get_mut("guard").unwrap().programs = [
        (P::BeforeSwap, vec![I::IncrementCounter]),
        (P::AfterSwap, vec![I::IncrementCounter]),
    ]
    .into();
    for n in [1usize, 2, 3, 31, 128, 200, 204] {
        tx.program = (0..n)
            .map(|i| swap(if i % 2 == 0 { "X" } else { "Q" }, 100, 1))
            .collect();
        s.blocks[0] = vec![tx.clone()];
        let b = execute(&s);
        assert_eq!(b.receipts[0].status, "Success");
        assert_eq!(b.receipts[0].trace.len(), 5 * n);
        assert_eq!(b.receipts[0].used, (100 + 60 * n as u64).into());
        assert_eq!(b.state.hooks["guard"].counter, (2 * n as u64).into());
    }
    tx.program = vec![swap("X", 100, 1); 204];
    tx.program.extend((0..4).map(|_| Call::AssertCondition {
        condition: true,
        message: "filler".into(),
    }));
    assert!(s.validate_tx(&tx).is_ok());
    tx.program.push(Call::AssertCondition {
        condition: true,
        message: "1025th".into(),
    });
    assert!(s.validate_tx(&tx).is_err());
}

#[test]
fn hook_context_privacy_and_quotes_100_probes() {
    let s = fixture("fee");
    let run = Run::new(s.clone()).unwrap();
    let checkpoint = run.checkpoint().unwrap();
    let query = h::Query {
        agent: "alice".into(),
        sender: "alice".into(),
        asset_in: "X".into(),
        asset_out: "Q".into(),
        amount: 100.into(),
        min_out: 1.into(),
        residual_policy: ResidualPolicy::RequireFull,
        resource_limit: 500.into(),
        resource_price: 1.into(),
        limits: vec![],
    };
    let first = h::quote(&s, &run.state, &query).unwrap();
    for _ in 0..100 {
        assert_eq!(h::quote(&s, &run.state, &query).unwrap(), first);
    }
    let r = first.search.candidates[0].receipt.as_ref().unwrap();
    assert_eq!(r.route_results["0"].output, 82.into());
    assert_eq!(r.used, 205.into());
    assert_eq!(run.checkpoint().unwrap(), checkpoint);
    let a = routing::Observation::capture(&s, &run.state, "alice").unwrap();
    let mut changed = s.clone();
    set_balance(&mut changed, "bob", "Q", 12345678);
    let w = World::genesis(&changed).unwrap();
    let b = routing::Observation::capture(&changed, &w, "alice").unwrap();
    assert_eq!(a, b);
    assert!(engine::quote(&s, &run.state, "cpmm", "X", 100.into()).is_err());
}

#[test]
fn config_and_protected_role_matrix_21_cases() {
    let base = fixture("fee");
    for role in ["alice", "bob", "builder", "pool"] {
        let mut s = base.clone();
        s.hooks.get_mut("guard").unwrap().account = role.into();
        assert!(s.validate().is_err(), "{role}");
    }
    for case in 0..17 {
        let mut s = base.clone();
        let c = s.hooks.get_mut("guard").unwrap();
        match case {
            0 => c.capabilities.swap_fee = false,
            1 => c.capabilities.reject = false,
            2 => c.capabilities.write_local = false,
            3 => c.capabilities.spend_own.clear(),
            4 => {
                c.programs = [(
                    P::AfterLiquidity,
                    vec![I::FeeByInput {
                        asset_in: "X".into(),
                        threshold: 1.into(),
                        below_bps: 0,
                        at_or_above_bps: 1,
                    }],
                )]
                .into()
            }
            5 => {
                c.programs = [(
                    P::BeforeLiquidity,
                    vec![I::MaxInput {
                        asset_in: "X".into(),
                        maximum: 0.into(),
                    }],
                )]
                .into()
            }
            6 => {
                c.programs = [(
                    P::BeforeSwap,
                    vec![I::FeeByInput {
                        asset_in: "X".into(),
                        threshold: 0.into(),
                        below_bps: 0,
                        at_or_above_bps: 1,
                    }],
                )]
                .into()
            }
            7 => {
                c.programs = [(
                    P::BeforeSwap,
                    vec![I::FeeByInput {
                        asset_in: "X".into(),
                        threshold: 1.into(),
                        below_bps: 10000,
                        at_or_above_bps: 1,
                    }],
                )]
                .into()
            }
            8 => c.programs = [(P::BeforeSwap, vec![I::Reject { reason: "".into() }])].into(),
            9 => {
                c.programs = [(
                    P::BeforeSwap,
                    vec![I::Reject {
                        reason: "я".repeat(65),
                    }],
                )]
                .into()
            }
            10 => c.programs = [(P::BeforeSwap, vec![I::IncrementCounter; 17])].into(),
            11 => {
                c.programs = [(
                    P::BeforeSwap,
                    vec![I::MaxInput {
                        asset_in: "MISSING".into(),
                        maximum: 1.into(),
                    }],
                )]
                .into()
            }
            12 => {
                c.programs = [(
                    P::AfterSwap,
                    vec![I::PaySender {
                        asset: "Q".into(),
                        amount: 0.into(),
                    }],
                )]
                .into()
            }
            13 => c.capabilities.spend_own = [("Q".into(), 0.into())].into(),
            14 => c.pool = "MISSING".into(),
            15 => {
                let i = c.programs[&P::BeforeSwap][0].clone();
                c.programs.get_mut(&P::BeforeSwap).unwrap().push(i);
            }
            16 => {
                let duplicate = c.clone();
                s.hooks.insert("duplicate".into(), duplicate);
            }
            _ => unreachable!(),
        }
        assert!(s.validate().is_err(), "case {case}");
    }
}

#[test]
fn arbitrage_profit_boundary_accounts_for_payout_and_all_resource_8_runs() {
    for price in [0u64, 1] {
        for reward in [100u64, 213, 214, 300] {
            let mut s = fixture("fee");
            s.blocks = vec![vec![]];
            set_balance(&mut s, "alice", "Q", 10000);
            set_balance(&mut s, "hook", "Q", 1000);
            s.accounts
                .insert("pool2".into(), s.accounts["pool"].clone());
            let mut p = s.protocols["cpmm"].clone();
            p.account = "pool2".into();
            s.protocols.insert("second".into(), p);
            let c = s.hooks.get_mut("guard").unwrap();
            c.capabilities.spend_own = [("Q".into(), reward.into())].into();
            c.programs = [(
                P::AfterSwap,
                vec![
                    I::IncrementCounter,
                    I::PaySender {
                        asset: "Q".into(),
                        amount: reward.into(),
                    },
                ],
            )]
            .into();
            s.policies.insert(
                "alice".into(),
                policy::ArbitragePolicy {
                    account: "alice".into(),
                    asset: "Q".into(),
                    input: 100.into(),
                    min_profit: 1.into(),
                    resource_limit: 500.into(),
                    resource_price: price.into(),
                    route_limits: Some(vec![]),
                },
            );
            let mut r = Run::new(s).unwrap();
            r.finish().unwrap();
            let d = &r.blocks[0].decisions[0];
            // 100 Q -> floor(1000*100/1100)=90 X -> floor(1000*90/1090)=82 Q.
            // 100 envelope + 10 route + 2*(10+5+5) swaps + 35 hook + 10 guard = 195.
            let profitable = 82 + reward > 100 + 195 * price;
            assert_eq!(d.transaction.is_some(), profitable);
            if profitable {
                let gain = 82 + reward - 100 - 195 * price;
                assert_eq!(d.expected_net_gain, Some(gain.into()));
                assert_eq!(d.executed_route.as_ref().unwrap().output, 82.into());
                assert_eq!(r.state.balance("alice", "Q"), (10000 + gain).into());
                assert_eq!(r.state.balance("hook", "Q"), (1000 - reward).into());
                assert_eq!(r.state.hooks["guard"].counter, 1.into());
                assert_eq!(r.blocks[0].receipts[0].used, 195.into());
            } else {
                assert!(r.blocks[0].receipts.is_empty());
                assert_eq!(r.state.hooks["guard"].counter, 0.into());
                assert_eq!(r.state.balance("hook", "Q"), 1000.into());
            }
        }
    }
}

#[test]
fn independent_liquidator_trial_numbers_and_hook_rejection_4_runs() {
    for flash in [false, true] {
        for reject in [false, true] {
            let name = "sc-liquidator-own.json";
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../scenarios/demos")
                .join(name);
            let mut s = Scenario::parse(&std::fs::read(path).unwrap()).unwrap();
            s.scheduler.as_mut().unwrap().rounds = 3;
            if flash {
                s.scheduler
                    .as_mut()
                    .unwrap()
                    .liquidator
                    .as_mut()
                    .unwrap()
                    .policies
                    .get_mut("carol")
                    .unwrap()
                    .funding = policy::liquidator::Funding::Flash {
                    provider: "flash".into(),
                };
            }
            s.model_versions.push(hooks::MODEL.into());
            s.accounts.insert(
                "hook".into(),
                Account {
                    nonce: 0.into(),
                    balances: [("Q".into(), 20.into())].into(),
                },
            );
            let mut c = fixture("fee").hooks["guard"].clone();
            c.pool = "sale".into();
            c.programs = [(
                P::AfterSwap,
                vec![
                    I::IncrementCounter,
                    I::PaySender {
                        asset: "Q".into(),
                        amount: 3.into(),
                    },
                ],
            )]
            .into();
            if reject {
                c.programs.get_mut(&P::AfterSwap).unwrap().push(I::Reject {
                    reason: "after writeoff".into(),
                });
            }
            s.hooks.insert("guard".into(), c);
            let mut r = Run::new(s).unwrap();
            r.finish().unwrap();
            let decision = &r.blocks[2]
                .scheduler
                .as_ref()
                .unwrap()
                .liquidators
                .as_ref()
                .unwrap()
                .decisions[0];
            assert_eq!(decision.candidates.len(), 4);
            assert_eq!(decision.transaction.is_some(), !reject);
            let paid = [20u64, 40, 60, 60];
            let seized = [2u64, 4, 6, 6];
            for (index, candidate) in decision.candidates.iter().enumerate() {
                let receipt = candidate.receipt.as_ref().unwrap();
                let sale = 1500 * seized[index] / (100 + seized[index]);
                if reject {
                    assert_eq!(receipt.status, "ProtocolRevert");
                    assert!(receipt.liquidations.is_empty());
                    assert!(receipt.events.is_empty());
                    assert!(candidate.net.is_none());
                } else {
                    let net = sale + 3 - paid[index] - u64::from(flash);
                    assert_eq!(candidate.net, Some(net.into()));
                    assert_eq!(receipt.used, if flash { 235.into() } else { 205.into() });
                    let liquidated = receipt.liquidations.values().next().unwrap();
                    assert_eq!(liquidated.paid, paid[index].into());
                    assert_eq!(liquidated.seized, seized[index].into());
                    assert_eq!(liquidated.route.as_ref().unwrap().output, sale.into());
                }
            }
            assert_eq!(
                r.state.hooks["guard"].counter,
                if reject { 0.into() } else { 1.into() }
            );
            assert_eq!(
                r.state.balance("hook", "Q"),
                if reject { 20.into() } else { 17.into() }
            );
            if reject {
                assert_eq!(r.state.lending["credit"].debt, 80.into());
                assert!(r.state.lending["credit"].losses.is_empty());
            } else {
                assert_eq!(
                    r.state.balance("carol", "Q"),
                    (1027 - u64::from(flash)).into()
                );
                assert_eq!(r.state.lending["credit"].debt, 0.into());
            }
        }
    }
}

#[test]
fn export_observer_and_replay_tampering_six_demos() {
    for demo in ["fee", "limit", "lp", "rollback", "route", "shortfall"] {
        let mut r = Run::new(fixture(demo)).unwrap();
        r.finish().unwrap();
        let checkpoint = r.checkpoint().unwrap();
        assert_eq!(Run::restore(&checkpoint).unwrap(), r);
        let path = std::path::PathBuf::from(format!(
            "/tmp/dec25-review-export-{}-{demo}",
            std::process::id()
        ));
        r.export(&path).unwrap();
        let rows = std::fs::read_to_string(path.join("hook-attempts.jsonl")).unwrap();
        for row in rows.lines() {
            let value: serde_json::Value = serde_json::from_str(row).unwrap();
            let tx = value["tx"].as_str().unwrap();
            let receipt = r
                .blocks
                .iter()
                .flat_map(|b| &b.receipts)
                .find(|r| r.tx_id == tx)
                .unwrap();
            assert_eq!(value["committed"], receipt.status == "Success");
        }
        assert_eq!(r.checkpoint().unwrap(), checkpoint);
        let mut counter = r.clone();
        counter.state.hooks.get_mut("guard").unwrap().counter = 999.into();
        assert!(Run::restore(&counter.checkpoint().unwrap()).is_err());
        let mut config = r.clone();
        config
            .scenario
            .hooks
            .get_mut("guard")
            .unwrap()
            .programs
            .clear();
        assert!(Run::restore(&config.checkpoint().unwrap()).is_err());
    }
}

#[test]
fn backrun_all_ten_candidates_and_greedy_commit_6_runs() {
    for price in [0u64, 1] {
        for reward in [1u64, 3, 400] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../scenarios/demos/sc-backrun-public.json");
            let mut s = Scenario::parse(&std::fs::read(path).unwrap()).unwrap();
            s.model_versions.push(hooks::MODEL.into());
            s.accounts.insert(
                "hook".into(),
                Account {
                    nonce: 0.into(),
                    balances: [("Q".into(), 1000.into())].into(),
                },
            );
            let mut c = fixture("fee").hooks["guard"].clone();
            c.capabilities.spend_own = [("Q".into(), reward.into())].into();
            c.programs = [(
                P::AfterSwap,
                vec![
                    I::IncrementCounter,
                    I::PaySender {
                        asset: "Q".into(),
                        amount: reward.into(),
                    },
                ],
            )]
            .into();
            s.hooks.insert("guard".into(), c);
            let policy = s
                .scheduler
                .as_mut()
                .unwrap()
                .backrun
                .as_mut()
                .unwrap()
                .policies
                .get_mut("trader")
                .unwrap();
            policy.resource_price = price.into();
            policy.payment_price = price.into();
            let mut run = Run::new(s).unwrap();
            run.finish().unwrap();
            let b = &run.blocks[0];
            let report = b.scheduler.as_ref().unwrap().search.as_ref().unwrap();
            let d = &report.decisions[0];
            assert_eq!(d.candidates.len(), 10);
            assert_eq!(d.prefix.as_ref().unwrap().used, 155.into());
            for c in &d.candidates {
                let n = c.input.to_string().parse::<u64>().unwrap();
                let first = c.hops[0].venue();
                let out = if first == "second" {
                    let x = 1000 * n / (1000 + n);
                    1200 * x / (834 + x)
                } else {
                    let x = 834 * n / (1200 + n);
                    1000 * x / (1000 + x)
                };
                let gross = out + reward;
                let spent = n + 5 + 310 * price;
                if gross > spent {
                    assert_eq!(c.net, Some((gross - spent).into()));
                    assert_eq!(c.used, 310.into());
                    assert_eq!(c.receipts[0].route_results["0"].output, out.into());
                } else {
                    assert!(c.net.is_none());
                }
            }
            let profitable = 16 + reward > 5 + 310 * price;
            assert_eq!(d.selected.is_some(), profitable);
            if profitable {
                let gain = 16 + reward - 5 - 310 * price;
                assert_eq!(d.candidates[d.selected.unwrap()].input, 100.into());
                assert_eq!(run.state.balance("alice", "Q"), (10000 + gain).into());
                assert_eq!(run.state.hooks["guard"].counter, 2.into());
                assert_eq!(run.state.balance("hook", "Q"), (1000 - 2 * reward).into());
                assert_eq!(run.state.balance("builder", "Q"), (5 + 310 * price).into());
                assert_eq!(b.receipts.len(), 3);
            } else {
                assert_eq!(run.state.balance("alice", "Q"), 10000.into());
                assert_eq!(run.state.hooks["guard"].counter, 1.into());
                assert_eq!(run.state.balance("hook", "Q"), (1000 - reward).into());
                assert_eq!(b.receipts.len(), 1);
            }
            assert_eq!(run.state.balance("bob", "Q"), (800 + reward).into());
            assert_eq!(run.state.balance("bob", "X"), 166.into());
        }
    }
}

#[test]
fn counter_max_minus_one_late_overflow_and_fee_9999_are_exact() {
    for after in [false, true] {
        let mut s = fixture("fee");
        s.hooks.get_mut("guard").unwrap().programs = [(
            if after { P::AfterSwap } else { P::BeforeSwap },
            vec![I::IncrementCounter, I::IncrementCounter],
        )]
        .into();
        let mut w = World::genesis(&s).unwrap();
        w.hooks.get_mut("guard").unwrap().counter = Amount::MAX.checked_sub(1.into()).unwrap();
        let initial = w.clone();
        let b = engine::build_block(&s, &w, &s.blocks[0]).unwrap();
        assert_eq!(w, initial);
        assert_eq!(b.receipts[0].error.as_deref(), Some("HookStateExhausted"));
        assert_eq!(
            b.receipts[0].used,
            if after { 150.into() } else { 140.into() }
        );
        assert_eq!(
            b.state.hooks["guard"].counter,
            Amount::MAX.checked_sub(1.into()).unwrap()
        );
        assert_eq!(b.state.balance("pool", "Q"), 1000.into());
        assert!(b.receipts[0].events.is_empty());
    }
    let mut s = fixture("fee");
    set_balance(&mut s, "pool", "Q", 1000000);
    s.hooks.get_mut("guard").unwrap().programs = [(
        P::BeforeSwap,
        vec![I::FeeByInput {
            asset_in: "X".into(),
            threshold: 100.into(),
            below_bps: 0,
            at_or_above_bps: 9999,
        }],
    )]
    .into();
    let b = execute(&s);
    // 1,000,000 * (100*1) / (1000*10000 + 100*1) = 9, rounded down.
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(b.receipts[0].used, 140.into());
    assert_eq!(b.state.balance("pool", "Q"), 999991.into());
    assert_eq!(b.state.balance("alice", "Q"), 869.into());
}
