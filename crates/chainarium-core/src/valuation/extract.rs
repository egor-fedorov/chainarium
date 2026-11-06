use super::*;
use crate::Scenario;

struct Scan<'a> {
    p: &'a Policy,
    at: Amount,
    checks: usize,
    limited: bool,
    components: Vec<Component>,
}
impl Scan<'_> {
    fn check(&mut self) -> bool {
        if self.checks == self.p.limits.component_checks_per_portfolio {
            self.limited = true;
            return false;
        }
        self.checks += 1;
        true
    }
    fn push(
        &mut self,
        key: Key,
        quantity: Result<Exact, String>,
        deposit_cash: Option<DepositCash>,
    ) {
        let (quantity, mark, price) = match quantity {
            Ok(q) => {
                let (m, p) = marked(self.p, &key.asset, self.at, &q);
                (Some(q), m, p)
            }
            Err(e) => {
                let mut m = Mark::zero();
                m.fail(&e);
                (None, m, None)
            }
        };
        self.components.push(Component {
            key,
            quantity,
            mark,
            price,
            deposit_cash,
        });
    }
}
fn key(account: &str, kind: &str, protocol: &str, position: &str, asset: &str) -> Key {
    Key {
        account: account.into(),
        kind: kind.into(),
        protocol: protocol.into(),
        position: position.into(),
        asset: asset.into(),
    }
}
pub(super) fn snapshot(s: &Scenario, w: &World, p: &Policy, portfolio: &Portfolio) -> Snapshot {
    let mut scan = Scan {
        p,
        at: w.timestamp,
        checks: 0,
        limited: false,
        components: vec![],
    };
    let owners: BTreeSet<_> = portfolio.accounts.iter().collect();
    // Direct account lookup does not scan foreign wallet entries. Every examined
    // claim/position/order record does count, even when foreign or zero. Protocol
    // container lookups count too, so empty containers cannot evade the bound.
    'scan: {
        for account in &owners {
            if !scan.check() {
                break 'scan;
            }
            for (asset, n) in &w.accounts[*account].balances {
                if !scan.check() {
                    break 'scan;
                }
                scan.push(
                    key(account, "Wallet", "", "", asset),
                    Ok(Exact::integer(*n)),
                    None,
                );
            }
        }
        // Both pool kinds share the native LP ownership table; no pool cash is
        // also counted as a wallet. No solver or burn is invoked for a mark.
        for (id, balances) in w.lp_balances.iter() {
            if !scan.check() {
                break 'scan;
            }
            let Some(pool) = s.pool(id) else {
                continue;
            };
            for (owner, shares) in balances {
                if !scan.check() {
                    break 'scan;
                }
                if !owners.contains(owner) {
                    continue;
                }
                for asset in [&pool.x, &pool.y] {
                    let quantity = if shares.is_zero() {
                        Ok(Exact::default())
                    } else {
                        Exact::ratio(
                            (shares.big() * w.balance(&pool.account, asset).big()).into(),
                            w.shares[id].big(),
                            p.limits.rational_bits,
                        )
                    };
                    scan.push(
                        key(
                            owner,
                            if s.stable_pools.contains_key(id) {
                                "StableLP"
                            } else {
                                "CpmmLP"
                            },
                            id,
                            "",
                            asset,
                        ),
                        quantity,
                        None,
                    );
                }
            }
        }
        for (id, state) in w.clmm.iter() {
            if !scan.check() {
                break 'scan;
            }
            let cfg = &s.clmm_pools[id];
            for (position_id, position) in &state.positions {
                if !scan.check() {
                    break 'scan;
                }
                if !owners.contains(&position.owner) {
                    continue;
                }
                let quantities = (|| -> Result<[Amount; 2], String> {
                    let principal = crate::clmm::math::principal(
                        position.liquidity,
                        state.sqrt_price,
                        position.lower,
                        position.upper,
                        false,
                    )?;
                    let mut copy = position.clone();
                    state.accrue(&mut copy)?;
                    Ok([
                        principal[0].checked_add(copy.owed[0])?,
                        principal[1].checked_add(copy.owed[1])?,
                    ])
                })();
                for (i, asset) in [&cfg.x, &cfg.q].into_iter().enumerate() {
                    scan.push(
                        key(
                            &position.owner,
                            "ClmmClaim",
                            id,
                            &position_id.to_string(),
                            asset,
                        ),
                        quantities
                            .as_ref()
                            .map(|ns| Exact::integer(ns[i]))
                            .map_err(|_| "InvalidClaim".into()),
                        None,
                    );
                }
            }
        }
        for (id, state) in w.books.iter() {
            if !scan.check() {
                break 'scan;
            }
            let cfg = &s.books[id];
            for (order_id, order) in &state.orders {
                if !scan.check() {
                    break 'scan;
                }
                if !owners.contains(&order.owner) {
                    continue;
                }
                let quantity = cfg
                    .locked(order.side, order.price_ticks, order.remaining_lots)
                    .map(Exact::integer)
                    .map_err(|_| "InvalidClaim".into());
                scan.push(
                    key(
                        &order.owner,
                        "BookEscrow",
                        id,
                        &order_id.to_string(),
                        cfg.reserve_asset(order.side),
                    ),
                    quantity,
                    None,
                );
            }
        }
        for (id, state) in w.lending.iter() {
            if !scan.check() {
                break 'scan;
            }
            let cfg = &s.lending[id];
            for (owner, shares) in &state.deposit_shares {
                if !scan.check() {
                    break 'scan;
                }
                if !owners.contains(owner) {
                    continue;
                }
                let claim = if shares.is_zero() {
                    Ok(Amount::ZERO)
                } else {
                    shares.mul_div(state.claims, state.total_shares, false)
                };
                let cash = w.balance(&cfg.cash_account, &cfg.debt_asset);
                scan.push(
                    key(owner, "DepositClaim", id, "", &cfg.debt_asset),
                    claim
                        .as_ref()
                        .map(|n| Exact::integer(*n))
                        .map_err(|_| "InvalidClaim".into()),
                    claim.ok().map(|n| DepositCash {
                        claim: n,
                        reserve_cash: cash,
                        cash_bound: n.min(cash),
                    }),
                );
            }
            for (owner, position) in &state.positions {
                if !scan.check() {
                    break 'scan;
                }
                if !owners.contains(owner) {
                    continue;
                }
                scan.push(
                    key(owner, "Collateral", id, "", &cfg.collateral_asset),
                    Ok(Exact::integer(
                        w.balance(&cfg.positions[owner], &cfg.collateral_asset),
                    )),
                    None,
                );
                scan.push(
                    key(owner, "Debt", id, "", &cfg.debt_asset),
                    Ok(Exact::integer(position.debt).neg()),
                    None,
                );
            }
        }
    }
    scan.components.sort_by(|a, b| a.key.cmp(&b.key));
    let mut mark = Mark::zero();
    let mut by_asset: BTreeMap<String, Mark> = BTreeMap::new();
    for c in &scan.components {
        mark.add(&c.mark, p.limits.rational_bits);
        let asset = by_asset
            .entry(c.key.asset.clone())
            .or_insert_with(Mark::zero);
        asset.add(&c.mark, p.limits.rational_bits);
        if asset.truncated {
            mark.fail("ArithmeticLimit");
        }
    }
    if scan.limited {
        mark.fail("ComponentLimit");
    }
    Snapshot {
        mark,
        components: scan.components,
        by_asset,
        component_checks: scan.checks,
    }
}
