use chainarium_core::{
    hooks::{self, Instruction as I, Point as P},
    routing::heterogeneous::{ResidualPolicy, VenueHop},
    *,
};
fn fixture(kind: &str) -> Scenario {
    Scenario::parse(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../scenarios/demos/sc-hook-{kind}.json")),
        )
        .unwrap(),
    )
    .unwrap()
}
fn finish(s: Scenario) -> Run {
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    r
}
fn first(r: &Run) -> &Receipt {
    &r.blocks[0].receipts[0]
}
fn clean() -> Scenario {
    let mut s = fixture("fee");
    s.blocks.truncate(1);
    s
}
fn mutate_swap(s: &mut Scenario, amount: u64, minimum: u64) {
    if let Call::SwapExactIn {
        amount: a, min_out, ..
    } = &mut s.blocks[0][0].program[0]
    {
        *a = amount.into();
        *min_out = minimum.into();
    }
}
#[test]
fn exact_reference_success_control_and_threshold_boundary() {
    for (n, fee, out) in [
        (50u64, 0, 47u64),
        (99, 0, 90),
        (100, 1000, 82),
        (200, 1000, 152),
    ] {
        let mut s = clean();
        mutate_swap(&mut s, n, 1);
        let r = finish(s);
        let q = first(&r);
        assert_eq!(q.status, "Success");
        assert_eq!(q.used, 195.into());
        assert_eq!(r.state.balance("alice", "Q"), (1000 + out + 3 - 195).into());
        assert_eq!(r.state.balance("pool", "Q"), (1000 - out).into());
        assert_eq!(r.state.hooks["guard"].counter, 2.into());
        assert_eq!(r.state.balance("hook", "Q"), 17.into());
        assert!(q.trace.iter().any(|t| {
            t.hook
                .as_ref()
                .is_some_and(|h| h.effective_fee == Some(fee))
        }));
    }
    for enabled in [false, true] {
        let mut s = clean();
        s.hooks.clear();
        if !enabled {
            s.model_versions.retain(|m| m != hooks::MODEL);
        }
        let r = finish(s);
        assert_eq!(first(&r).used, 120.into());
        assert_eq!(r.state.balance("alice", "Q"), 970.into());
    }
}
#[test]
fn reference_late_limit_minimum_shortage_and_exhaustion_rollback() {
    for (kind, used) in [
        ("rollback", 205u64),
        ("limit", 140),
        ("shortfall", 195),
        ("minimum", 150),
        ("exhausted", 194),
    ] {
        let mut s = if kind == "minimum" || kind == "exhausted" {
            clean()
        } else {
            fixture(kind)
        };
        s.blocks.truncate(1);
        if kind == "minimum" {
            mutate_swap(&mut s, 100, 83);
        }
        if kind == "exhausted" {
            s.blocks[0][0].limit = 194.into();
        }
        let hook_before = s.accounts["hook"].clone();
        let r = finish(s);
        let q = first(&r);
        assert_eq!(
            q.status,
            if kind == "exhausted" {
                "ResourceExhausted"
            } else {
                "ProtocolRevert"
            }
        );
        assert_eq!(q.used, used.into());
        assert_eq!(r.state.balance("alice", "Q"), (1000 - used).into());
        assert_eq!(r.state.balance("alice", "X"), 1000.into());
        assert_eq!(r.state.balance("pool", "Q"), 1000.into());
        assert_eq!(r.state.accounts["hook"], hook_before);
        assert_eq!(r.state.hooks["guard"].counter, 0.into());
        assert!(!q.events.iter().any(|e| e.kind.starts_with("Hook")));
        assert_eq!(r.state.accounts["alice"].nonce, 1.into());
    }
}
#[test]
fn lp_mint_burn_and_late_failure_share_hook_journal() {
    let r = finish(fixture("lp"));
    assert_eq!(r.blocks[0].state.balance("alice", "Q"), 728.into());
    assert_eq!(r.state.balance("alice", "Q"), 656.into());
    assert_eq!(r.state.balance("alice", "X"), 1000.into());
    assert_eq!(r.state.hooks["guard"].counter, 4.into());
    assert_eq!(r.state.balance("hook", "Q"), 14.into());
    assert_eq!(r.state.balance("builder", "Q"), 350.into());
    assert_eq!(r.state.shares["cpmm"], 1000.into());
    for b in &r.blocks {
        assert_eq!(b.receipts[0].used, 175.into());
    }
    let mut s = fixture("lp");
    s.blocks.truncate(1);
    s.blocks[0][0].program.push(Call::AssertCondition {
        condition: false,
        message: "late".into(),
    });
    let r = finish(s);
    assert_eq!(first(&r).used, 185.into());
    assert_eq!(r.state.balance("alice", "Q"), 815.into());
    assert_eq!(r.state.hooks["guard"].counter, 0.into());
    assert_eq!(r.state.shares["cpmm"], 1000.into());
}
#[test]
fn hook_output_reward_is_separate_from_route_output_and_next_hop_budget() {
    let r = finish(fixture("route"));
    let q = first(&r);
    assert_eq!(q.status, "Success");
    assert_eq!(q.used, 225.into());
    let route = &q.route_results["0"];
    assert_eq!(route.hops[0].output, 82.into());
    assert_eq!(route.hops[1].budget, 82.into());
    assert_eq!(route.output, 75.into());
    assert_eq!(r.state.balance("alice", "R"), 75.into());
    assert_eq!(r.state.balance("alice", "Q"), 778.into());
    assert!(route.residuals.is_empty());
}
#[test]
fn input_reward_never_makes_a_full_route_partial_or_causes_balance_underflow() {
    for reward in [3u64, 200] {
        let mut s = clean();
        s.accounts
            .get_mut("hook")
            .unwrap()
            .balances
            .insert("X".into(), reward.into());
        let h = s.hooks.get_mut("guard").unwrap();
        h.capabilities.spend_own = [("X".into(), reward.into())].into();
        h.programs.get_mut(&P::AfterSwap).unwrap()[1] = I::PaySender {
            asset: "X".into(),
            amount: reward.into(),
        };
        s.blocks[0][0].program = vec![Call::ExecuteRoute {
            asset_in: "X".into(),
            amount: 100.into(),
            hops: vec![VenueHop::Amm {
                venue: "cpmm".into(),
            }],
            min_out: 82.into(),
            residual_policy: ResidualPolicy::RequireFull,
        }];
        let r = finish(s);
        assert_eq!(first(&r).status, "Success");
        let route = &first(&r).route_results["0"];
        assert_eq!(route.input_spent, 100.into());
        assert_eq!(route.input_unspent, 0.into());
        assert_eq!(route.output, 82.into());
        assert_eq!(r.state.balance("alice", "X"), (900 + reward).into());
    }
}
#[test]
fn quote_and_every_demo_checkpoint_are_read_only_and_replay_is_checked() {
    for kind in ["fee", "limit", "rollback", "shortfall", "lp", "route"] {
        let mut r = Run::new(fixture(kind)).unwrap();
        while r.step().unwrap().is_some() {
            let before = r.checkpoint().unwrap();
            let obs = routing::Observation::capture(&r.scenario, &r.state, "alice").unwrap();
            assert!(obs.hooks.contains_key("guard"));
            let _ = hooks::views(&r.scenario, &r.state);
            assert_eq!(before, r.checkpoint().unwrap());
            assert_eq!(r, Run::restore(&before).unwrap());
        }
        let mut tampered = r.clone();
        tampered.state.hooks.get_mut("guard").unwrap().counter = 999.into();
        assert!(Run::restore(&tampered.checkpoint().unwrap()).is_err());
    }
    let s = clean();
    let w = World::genesis(&s).unwrap();
    assert!(
        engine::quote(&s, &w, "cpmm", "X", 100.into())
            .unwrap_err()
            .contains("HookRequiresExecution")
    );
}
#[test]
fn all_resource_boundaries_undo_hook_state_transfers_and_native_effects_correctly() {
    for price in [0u64, 1] {
        for limit in 100u64..=195 {
            let mut s = clean();
            s.blocks[0][0].price = price.into();
            s.blocks[0][0].limit = limit.into();
            let r = finish(s);
            let q = first(&r);
            assert_eq!(q.used, limit.into());
            assert_eq!(q.fee, (limit * price).into());
            assert_eq!(r.state.accounts["alice"].nonce, 1.into());
            if limit < 195 {
                assert_eq!(q.status, "ResourceExhausted");
                assert_eq!(r.state.hooks["guard"].counter, 0.into());
                assert_eq!(r.state.balance("hook", "Q"), 20.into());
                assert_eq!(r.state.balance("pool", "Q"), 1000.into());
                assert_eq!(r.state.balance("alice", "Q"), (1000 - limit * price).into());
            } else {
                assert_eq!(q.status, "Success");
            }
        }
    }
}
#[test]
fn capabilities_accounts_points_and_unknown_effects_are_rejected_before_execution() {
    for case in 0..10 {
        let mut s = clean();
        let h = s.hooks.get_mut("guard").unwrap();
        match case {
            0 => h.capabilities.swap_fee = false,
            1 => h.capabilities.reject = false,
            2 => h.capabilities.write_local = false,
            3 => {
                h.capabilities.spend_own.insert("Q".into(), 2.into());
            }
            4 => h
                .programs
                .get_mut(&P::AfterSwap)
                .unwrap()
                .push(I::PaySender {
                    asset: "Q".into(),
                    amount: 1.into(),
                }),
            5 => {
                h.programs
                    .insert(P::BeforeLiquidity, h.programs[&P::BeforeSwap].clone());
            }
            6 => h.account = "alice".into(),
            7 => h.account = "pool".into(),
            8 => {
                if let I::FeeByInput {
                    at_or_above_bps, ..
                } = &mut h.programs.get_mut(&P::BeforeSwap).unwrap()[0]
                {
                    *at_or_above_bps = 10000;
                }
            }
            _ => {
                s.model_versions.retain(|m| m != hooks::MODEL);
            }
        }
        assert!(s.validate().is_err(), "case {case}");
    }
    let mut value = serde_json::to_value(clean()).unwrap();
    value["hooks"]["guard"]["programs"]["AfterSwap"][1]["from"] = serde_json::json!("alice");
    assert!(Scenario::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    let mut s = clean();
    let h = s.hooks["guard"].clone();
    s.hooks.insert("duplicate".into(), h);
    assert!(s.validate().is_err());
}
#[test]
fn config_boundaries_16_17_instructions_and_64_65_bindings() {
    let mut s = clean();
    s.hooks
        .get_mut("guard")
        .unwrap()
        .programs
        .insert(P::BeforeSwap, vec![I::IncrementCounter; 16]);
    assert!(s.validate().is_ok());
    let r = finish(s.clone());
    assert_eq!(r.state.hooks["guard"].counter, 17.into());
    s.hooks
        .get_mut("guard")
        .unwrap()
        .programs
        .get_mut(&P::BeforeSwap)
        .unwrap()
        .push(I::IncrementCounter);
    assert!(s.validate().is_err());
    let mut s = clean();
    for i in 1..=64 {
        let id = format!("hook-{i}");
        let account = format!("hook-account-{i}");
        let pool = format!("pool-{i}");
        let mut h = s.hooks["guard"].clone();
        h.account = account.clone();
        h.pool = pool.clone();
        s.accounts.insert(account, s.accounts["hook"].clone());
        s.accounts.insert(pool.clone(), s.accounts["pool"].clone());
        let mut p = s.protocols["cpmm"].clone();
        p.account = pool.clone();
        s.protocols.insert(pool, p);
        s.hooks.insert(id, h);
        assert_eq!(s.validate().is_ok(), i < 64);
    }
}
#[test]
fn generated_hook_children_count_in_node_and_depth_caps_for_routes_and_flash() {
    let s = clean();
    let swap = s.blocks[0][0].program[0].clone();
    let mut tx = s.blocks[0][0].clone();
    tx.program = vec![swap.clone(); 128];
    assert!(s.validate_tx(&tx).is_ok());
    tx.program.push(Call::AssertCondition {
        condition: true,
        message: "one extra".into(),
    });
    assert!(s.validate_tx(&tx).is_err());
    for (call, wrappers) in [
        (swap.clone(), 29),
        (
            Call::ExecuteRoute {
                asset_in: "X".into(),
                amount: 100.into(),
                hops: vec![VenueHop::Amm {
                    venue: "cpmm".into(),
                }],
                min_out: 1.into(),
                residual_policy: ResidualPolicy::RequireFull,
            },
            28,
        ),
    ] {
        let mut c = call;
        for _ in 0..wrappers {
            c = Call::Sequence { calls: vec![c] };
        }
        tx.program = vec![c.clone()];
        assert!(s.validate_tx(&tx).is_ok());
        tx.program = vec![Call::Sequence { calls: vec![c] }];
        assert!(s.validate_tx(&tx).is_err());
    }
    let mut s = s;
    s.model_versions.push("strict-flash-provider-v1".into());
    s.accounts.insert(
        "flash".into(),
        Account {
            nonce: 0.into(),
            balances: [("X".into(), 10000.into())].into(),
        },
    );
    s.flash_providers.insert(
        "flash".into(),
        FlashProvider {
            account: "flash".into(),
            asset: "X".into(),
            fee_bps: 5,
        },
    );
    let mut call = Call::FlashLoan {
        provider: "flash".into(),
        receiver: "alice".into(),
        amount: 100.into(),
        callback: vec![swap],
    };
    for _ in 0..27 {
        call = Call::Sequence { calls: vec![call] };
    }
    tx.program = vec![call.clone()];
    assert!(s.validate_tx(&tx).is_ok());
    tx.program = vec![Call::Sequence { calls: vec![call] }];
    assert!(s.validate_tx(&tx).is_err());
}
#[test]
fn counter_max_is_a_paid_protocol_revert_and_never_wraps() {
    let s = clean();
    let mut w = World::genesis(&s).unwrap();
    w.hooks.get_mut("guard").unwrap().counter = Amount::MAX;
    let b = engine::build_block(&s, &w, &s.blocks[0]).unwrap();
    assert_eq!(b.receipts[0].error.as_deref(), Some("HookStateExhausted"));
    assert_eq!(b.receipts[0].used, 150.into());
    assert_eq!(b.state.hooks["guard"].counter, Amount::MAX);
    assert_eq!(b.state.balance("hook", "Q"), 20.into());
}
#[test]
fn maximum_valid_fee_and_empty_hook_programs_use_declared_semantics() {
    let mut s = clean();
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("X".into(), 10000000.into());
    mutate_swap(&mut s, 10000000, 500);
    let p = s
        .hooks
        .get_mut("guard")
        .unwrap()
        .programs
        .get_mut(&P::BeforeSwap)
        .unwrap();
    if let I::FeeByInput {
        at_or_above_bps, ..
    } = &mut p[0]
    {
        *at_or_above_bps = 9999;
    }
    if let I::MaxInput { maximum, .. } = &mut p[1] {
        *maximum = 10000000.into();
    }
    let r = finish(s);
    assert_eq!(first(&r).status, "Success");
    assert_eq!(r.state.balance("pool", "Q"), 500.into());
    let mut s = clean();
    s.hooks.get_mut("guard").unwrap().programs.clear();
    let r = finish(s);
    assert_eq!(first(&r).used, 120.into());
    assert_eq!(r.state.balance("hook", "Q"), 20.into());
    assert_eq!(r.state.hooks["guard"].counter, 0.into());
}
#[test]
fn two_calls_share_counter_and_a_late_error_rolls_both_back() {
    for late in [false, true] {
        let mut s = clean();
        let call = s.blocks[0][0].program[0].clone();
        s.blocks[0][0].program.push(call);
        if late {
            s.blocks[0][0].program.push(Call::AssertCondition {
                condition: false,
                message: "late".into(),
            });
        }
        let r = finish(s);
        assert_eq!(first(&r).used, if late { 300.into() } else { 290.into() });
        assert_eq!(
            r.state.hooks["guard"].counter,
            if late { 0.into() } else { 4.into() }
        );
        assert_eq!(
            r.state.balance("hook", "Q"),
            if late { 20.into() } else { 14.into() }
        );
        assert_eq!(
            r.state.balance("alice", "Q"),
            if late { 700.into() } else { 867.into() }
        );
    }
}
#[test]
fn swap_output_and_flash_settlement_never_absorb_hook_reward_into_swap_budget() {
    for capital in [0u64, 11] {
        let mut s = clean();
        s.model_versions
            .extend(["strict-flash-provider-v1".into(), "bounded-route-v1".into()]);
        s.accounts
            .get_mut("alice")
            .unwrap()
            .balances
            .insert("X".into(), capital.into());
        s.accounts.insert(
            "flash".into(),
            Account {
                nonce: 0.into(),
                balances: [("X".into(), 10000.into())].into(),
            },
        );
        s.flash_providers.insert(
            "flash".into(),
            FlashProvider {
                account: "flash".into(),
                asset: "X".into(),
                fee_bps: 5,
            },
        );
        let first = s.blocks[0][0].program[0].clone();
        s.blocks[0][0].program = vec![Call::FlashLoan {
            provider: "flash".into(),
            receiver: "alice".into(),
            amount: 100.into(),
            callback: vec![
                first,
                Call::SwapOutput {
                    pool: "cpmm".into(),
                    asset_in: "Q".into(),
                    output_of: "0.callback.0".into(),
                    min_out: 1.into(),
                },
            ],
        }];
        let r = finish(s);
        let q = &r.blocks[0].receipts[0];
        assert_eq!(q.used, 320.into());
        assert_eq!(
            q.status,
            if capital == 0 {
                "ProtocolRevert"
            } else {
                "Success"
            }
        );
        assert_eq!(
            r.state.hooks["guard"].counter,
            if capital == 0 { 0.into() } else { 4.into() }
        );
        assert_eq!(
            r.state.balance("flash", "X"),
            if capital == 0 {
                10000.into()
            } else {
                10001.into()
            }
        );
    }
}
#[test]
fn hook_account_cannot_be_spent_or_funded_by_ordinary_calls() {
    for call in [
        Call::Transfer {
            to: "hook".into(),
            asset: "Q".into(),
            amount: 1.into(),
        },
        Call::TransferFrom {
            from: "hook".into(),
            to: "alice".into(),
            asset: "Q".into(),
            amount: 1.into(),
        },
        Call::TransferFrom {
            from: "alice".into(),
            to: "hook".into(),
            asset: "Q".into(),
            amount: 1.into(),
        },
    ] {
        let mut s = clean();
        s.blocks[0][0].program = vec![call];
        let r = finish(s);
        assert_eq!(first(&r).status, "ProtocolRevert");
        assert_eq!(r.state.balance("hook", "Q"), 20.into());
    }
}
#[test]
fn greedy_probes_do_not_double_commit_counter_or_hook_cash() {
    let mut s = clean();
    let first = s.blocks[0][0].clone();
    s.blocks.clear();
    s.model_versions.retain(|m| m != "scheduler-m1-v1");
    s.model_versions.extend(
        [
            "event-scheduler-v1",
            "bounded-mempool-v1",
            "basic-builders-v1",
            "native-bundles-v1",
            "bounded-greedy-v1",
        ]
        .map(str::to_owned),
    );
    let template = Scenario::parse(include_bytes!(
        "../../../scenarios/demos/sc-liquidator-own.json"
    ))
    .unwrap();
    let mut c = template.scheduler.unwrap();
    c.rounds = 1;
    c.liquidator = None;
    c.builder = scheduler::Builder::Greedy;
    c.bundles = Some(scheduler::bundles::Config {
        limits: Default::default(),
        emissions: vec![],
    });
    c.agents.retain(|id, _| s.agents.contains_key(id));
    c.emissions.clear();
    let mut second = first.clone();
    second.id = "second-hook".into();
    second.agent = "bob".into();
    second.sender = "bob".into();
    second.price = 2.into();
    s.accounts.get_mut("bob").unwrap().balances =
        [("X".into(), 1000.into()), ("Q".into(), 2000.into())].into();
    for (i, t) in [first, second].into_iter().enumerate() {
        c.emissions.push(scheduler::Emission {
            at: (i as u64 + 1).into(),
            producer_id: "environment".into(),
            producer_sequence: (i as u64).into(),
            transaction: t,
            delivery: scheduler::Delivery {
                channel: scheduler::Channel::Public,
                builder_delay: 0.into(),
            },
        });
    }
    s.scheduler = Some(c);
    let r = finish(s);
    assert_eq!(r.blocks[0].receipts.len(), 2);
    assert_eq!(r.blocks[0].receipts[0].sender, "bob");
    assert!(r.blocks[0].scheduler.as_ref().unwrap().metrics.probes >= 3);
    assert_eq!(r.state.hooks["guard"].counter, 4.into());
    assert_eq!(r.state.balance("hook", "Q"), 14.into());
    assert_eq!(r.state.balance("builder", "Q"), 585.into());
}
#[test]
fn common_liquidator_executes_hook_and_late_hook_rejection_reverts_writeoff() {
    for reject in [false, true] {
        let mut s = Scenario::parse(include_bytes!(
            "../../../scenarios/demos/sc-liquidator-own.json"
        ))
        .unwrap();
        s.scheduler.as_mut().unwrap().rounds = 3;
        s.model_versions.push(hooks::MODEL.into());
        s.accounts.insert(
            "hook".into(),
            Account {
                nonce: 0.into(),
                balances: [("Q".into(), 20.into())].into(),
            },
        );
        let mut h = clean().hooks["guard"].clone();
        h.pool = "sale".into();
        h.programs.remove(&P::BeforeSwap);
        if reject {
            h.programs.get_mut(&P::AfterSwap).unwrap().push(I::Reject {
                reason: "reject after sale and loss".into(),
            });
        }
        s.hooks.insert("guard".into(), h);
        let r = finish(s);
        let d = &r.blocks[2]
            .scheduler
            .as_ref()
            .unwrap()
            .liquidators
            .as_ref()
            .unwrap()
            .decisions[0];
        assert_eq!(d.transaction.is_some(), !reject);
        if reject {
            assert!(r.blocks[2].receipts.is_empty());
            assert_eq!(r.state.hooks["guard"].counter, 0.into());
            assert_eq!(r.state.lending["credit"].debt, 80.into());
            assert!(r.state.lending["credit"].losses.is_empty());
            assert!(
                d.candidates
                    .iter()
                    .all(|c| c.receipt.as_ref().unwrap().liquidations.is_empty())
            );
        } else {
            assert_eq!(r.state.hooks["guard"].counter, 1.into());
            assert_eq!(r.state.balance("carol", "Q"), 1027.into());
            let q = &r.blocks[2].receipts[0];
            assert_eq!(
                q.liquidations["0"].route.as_ref().unwrap().output,
                84.into()
            );
            assert_eq!(d.candidates[d.selected.unwrap()].net, Some(27.into()));
        }
    }
}
#[test]
fn arbitrage_uses_full_balance_delta_even_when_swap_cycle_loses_but_hook_pays() {
    let mut s =
        Scenario::parse(include_bytes!("../../../scenarios/demos/sc-03-route.json")).unwrap();
    s.model_versions.push(hooks::MODEL.into());
    if !s
        .model_versions
        .iter()
        .any(|m| m == routing::heterogeneous::MODEL)
    {
        s.model_versions.push(routing::heterogeneous::MODEL.into());
    }
    s.blocks = vec![vec![]];
    s.accounts.insert(
        "hook".into(),
        Account {
            nonce: 0.into(),
            balances: [("Q".into(), 1000.into())].into(),
        },
    );
    // Equal-price pools produce a losing round trip; the funded reward is the only gain.
    for p in s.protocols.values() {
        s.accounts
            .get_mut(&p.account)
            .unwrap()
            .balances
            .insert(p.x.clone(), 1000.into());
        s.accounts
            .get_mut(&p.account)
            .unwrap()
            .balances
            .insert(p.y.clone(), 1000.into());
    }
    let venue = s.protocols.keys().next().unwrap().clone();
    let mut h = clean().hooks["guard"].clone();
    h.pool = venue;
    h.capabilities.spend_own.insert("Q".into(), 100.into());
    h.programs = [(
        P::AfterSwap,
        vec![I::PaySender {
            asset: "Q".into(),
            amount: 100.into(),
        }],
    )]
    .into();
    s.hooks.insert("guard".into(), h);
    assert!(s.validate().is_err());
    for p in s.policies.values_mut() {
        p.route_limits = Some(vec![]);
    }
    let r = finish(s);
    let d = &r.blocks[0].decisions[0];
    assert!(d.transaction.is_some());
    let route = d.executed_route.as_ref().unwrap();
    assert!(route.output < 100.into());
    let initial = r.initial.balance("alice", "Q");
    assert_eq!(
        d.expected_net_gain,
        Some(r.state.balance("alice", "Q").checked_sub(initial).unwrap())
    );
    assert!(d.expected_net_gain.unwrap() > Amount::ZERO);
}
#[test]
fn backrun_prefix_and_builder_replay_only_commit_selected_hook_effects_once() {
    let mut s = Scenario::parse(include_bytes!(
        "../../../scenarios/demos/sc-backrun-public.json"
    ))
    .unwrap();
    s.model_versions.push(hooks::MODEL.into());
    s.accounts.insert(
        "hook".into(),
        Account {
            nonce: 0.into(),
            balances: [("Q".into(), 1000.into())].into(),
        },
    );
    let mut h = clean().hooks["guard"].clone();
    h.programs.remove(&P::BeforeSwap);
    s.hooks.insert("guard".into(), h);
    let r = finish(s);
    let b = &r.blocks[0];
    let search = &b.scheduler.as_ref().unwrap().search.as_ref().unwrap();
    let d = &search.decisions[0];
    assert_eq!(d.candidates[d.selected.unwrap()].net, Some(14.into()));
    assert_eq!(r.state.balance("alice", "Q"), 10014.into());
    assert_eq!(r.state.hooks["guard"].counter, 2.into());
    assert_eq!(r.state.balance("hook", "Q"), 994.into());
}
#[test]
fn execution_quotes_preserve_hook_cash_counter_and_rng_and_keep_exact_output() {
    let r = Run::new(clean()).unwrap();
    let before = r.checkpoint().unwrap();
    let q = routing::heterogeneous::Query {
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
    for _ in 0..3 {
        let answer = routing::heterogeneous::quote(&r.scenario, &r.state, &q).unwrap();
        let receipt = answer.search.candidates[0].receipt.as_ref().unwrap();
        assert_eq!(receipt.status, "Success");
        assert_eq!(receipt.used, 205.into());
        assert_eq!(receipt.route_results["0"].output, 82.into());
        assert_eq!(before, r.checkpoint().unwrap());
    }
}
#[test]
fn hook_observation_excludes_other_wallets_and_runtime_capability_checks_undo_effects() {
    let a = finish(clean());
    let mut s = clean();
    s.accounts
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("Q".into(), 99999.into());
    let b = finish(s);
    assert_eq!(first(&a), first(&b));
    for doubled in [false, true] {
        let mut s = clean();
        let w = World::genesis(&s).unwrap();
        if doubled {
            s.hooks
                .get_mut("guard")
                .unwrap()
                .programs
                .get_mut(&P::AfterSwap)
                .unwrap()
                .push(I::PaySender {
                    asset: "Q".into(),
                    amount: 3.into(),
                });
        } else {
            s.hooks.get_mut("guard").unwrap().capabilities.swap_fee = false;
        }
        let b = engine::build_block(&s, &w, &s.blocks[0]).unwrap();
        assert_eq!(b.receipts[0].status, "ProtocolRevert");
        assert_eq!(b.state.hooks["guard"].counter, 0.into());
        assert_eq!(b.state.balance("hook", "Q"), 20.into());
        assert_eq!(b.state.balance("pool", "Q"), 1000.into());
    }
}
