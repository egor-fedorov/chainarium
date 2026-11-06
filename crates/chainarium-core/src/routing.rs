use crate::{Amount, Call, model::*};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VenueView {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable: Option<crate::stable::Parameters>,
    pub id: String,
    pub x: String,
    pub y: String,
    pub reserve_x: Amount,
    pub reserve_y: Amount,
    pub fee_bps: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub account: String,
    pub block: Amount,
    pub timestamp: Amount,
    pub nonce: Amount,
    pub balances: BTreeMap<String, Amount>,
    pub venues: Vec<VenueView>,
}
impl Observation {
    /// Only own balances and public pool state; no other wallets, pending orderflow or future inputs.
    pub fn capture(s: &Scenario, w: &World, account: &str) -> Result<Self, String> {
        let a = w.accounts.get(account).ok_or("unknown observer")?;
        if s.protected(account) {
            return Err("protocol account cannot observe as an agent".into());
        }
        Ok(Self {
            account: account.into(),
            block: w.block,
            timestamp: w.timestamp,
            nonce: a.nonce,
            balances: a.balances.clone(),
            venues: s
                .pools()
                .map(|(id, p)| {
                    Ok(VenueView {
                        stable: s
                            .stable_pools
                            .get(id)
                            .map(|p| p.parameters(s))
                            .transpose()?,
                        id: id.clone(),
                        x: p.x.clone(),
                        y: p.y.clone(),
                        reserve_x: w.balance(&p.account, &p.x),
                        reserve_y: w.balance(&p.account, &p.y),
                        fee_bps: p.fee_bps,
                    })
                })
                .collect::<Result<_, String>>()?,
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Hop {
    pub venue: String,
    pub asset_in: String,
    pub asset_out: String,
    pub input: Amount,
    pub output: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub hops: Vec<Hop>,
    pub output: Amount,
}
impl Route {
    pub fn calls(&self, input: Amount, min_out: Amount, prefix: &str) -> Vec<Call> {
        self.hops
            .iter()
            .enumerate()
            .map(|(i, hop)| {
                let minimum = if i + 1 == self.hops.len() {
                    min_out
                } else {
                    1.into()
                };
                if i == 0 {
                    Call::SwapExactIn {
                        pool: hop.venue.clone(),
                        asset_in: hop.asset_in.clone(),
                        amount: input,
                        min_out: minimum,
                    }
                } else {
                    Call::SwapOutput {
                        pool: hop.venue.clone(),
                        asset_in: hop.asset_in.clone(),
                        output_of: if prefix.is_empty() {
                            (i - 1).to_string()
                        } else {
                            format!("{prefix}.{}", i - 1)
                        },
                        min_out: minimum,
                    }
                }
            })
            .collect()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Search {
    pub routes: Vec<Route>,
    pub examined: Amount,
    pub truncated: bool,
}
pub fn search(observation: &Observation, asset_in: &str, asset_out: &str, input: Amount) -> Search {
    struct Searcher<'a> {
        venues: Vec<&'a VenueView>,
        target: &'a str,
        visited: BTreeSet<String>,
        path: Vec<Hop>,
        result: Search,
        count: u64,
    }
    impl Searcher<'_> {
        fn visit(&mut self, asset: &str, input: Amount) {
            if self.result.truncated || self.path.len() >= 3 {
                return;
            }
            for index in 0..self.venues.len() {
                let v = self.venues[index];
                if self.visited.contains(&v.id) {
                    continue;
                }
                let (other, x, y) = if asset == v.x {
                    (&v.y, v.reserve_x, v.reserve_y)
                } else if asset == v.y {
                    (&v.x, v.reserve_y, v.reserve_x)
                } else {
                    continue;
                };
                let quoted = if let Some(params) = &v.stable {
                    crate::stable::output(
                        [v.reserve_x, v.reserve_y],
                        asset == v.x,
                        input,
                        params,
                        &mut || Ok::<(), String>(()),
                        &mut vec![],
                    )
                } else {
                    crate::engine::cpmm_output(x, y, input, v.fee_bps)
                };
                let Ok(output) = quoted else {
                    continue;
                };
                self.path.push(Hop {
                    venue: v.id.clone(),
                    asset_in: asset.into(),
                    asset_out: other.clone(),
                    input,
                    output,
                });
                self.visited.insert(v.id.clone());
                if other == self.target {
                    if self.count == 64 {
                        self.result.truncated = true;
                    } else {
                        self.count += 1;
                        self.result.routes.push(Route {
                            hops: self.path.clone(),
                            output,
                        });
                    }
                } else {
                    self.visit(other, output);
                }
                self.path.pop();
                self.visited.remove(&v.id);
                if self.result.truncated {
                    return;
                }
            }
        }
    }
    let mut venues: Vec<_> = observation.venues.iter().collect();
    venues.sort_by(|a, b| a.id.cmp(&b.id));
    let mut worker = Searcher {
        venues,
        target: asset_out,
        visited: BTreeSet::new(),
        path: vec![],
        result: Search {
            routes: vec![],
            examined: Amount::ZERO,
            truncated: false,
        },
        count: 0,
    };
    worker.visit(asset_in, input);
    worker.result.examined = worker.count.into();
    worker.result.routes.sort_by(|a, b| {
        b.output.cmp(&a.output).then_with(|| {
            a.hops
                .iter()
                .map(|h| &h.venue)
                .cmp(b.hops.iter().map(|h| &h.venue))
        })
    });
    worker.result
}
