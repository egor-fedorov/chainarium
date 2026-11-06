use crate::{Amount, Call, Chain, Receipt, Scenario, Transaction, World};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MODEL: &str = "heterogeneous-route-v1";
pub const MAX_HOPS: usize = 3;
pub const MAX_CANDIDATES: usize = 64;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResidualPolicy {
    RequireFull,
    KeepResiduals,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum VenueHop {
    Amm {
        venue: String,
    },
    Clmm {
        venue: String,
        limit_tick: i32,
    },
    Book {
        venue: String,
        limit_price_ticks: Amount,
    },
}
impl VenueHop {
    pub fn venue(&self) -> &str {
        match self {
            Self::Amm { venue } | Self::Clmm { venue, .. } | Self::Book { venue, .. } => venue,
        }
    }
    pub fn other(&self, s: &Scenario, input: &str) -> Result<String, String> {
        let (x, y) = match self {
            Self::Amm { venue } => {
                let p = s.pool(venue).ok_or("unknown AMM route venue")?;
                (&p.x, &p.y)
            }
            Self::Clmm { venue, limit_tick } => {
                use crate::clmm::math::{MAX_TICK, MIN_TICK};
                if !(MIN_TICK..=MAX_TICK).contains(limit_tick) {
                    return Err("route limit tick outside bounds".into());
                }
                let p = s.clmm_pools.get(venue).ok_or("unknown CLMM route venue")?;
                (&p.x, &p.q)
            }
            Self::Book {
                venue,
                limit_price_ticks,
            } => {
                let p = s.books.get(venue).ok_or("unknown book route venue")?;
                if limit_price_ticks.is_zero() {
                    return Err("zero route book limit".into());
                }
                p.quote_per_lot(*limit_price_ticks)?;
                (&p.base_asset, &p.quote_asset)
            }
        };
        if input == x {
            Ok(y.clone())
        } else if input == y {
            Ok(x.clone())
        } else {
            Err("route input asset not in venue".into())
        }
    }
    pub(crate) fn native_call(
        &self,
        s: &Scenario,
        input: &str,
        budget: Amount,
    ) -> Result<Call, String> {
        self.other(s, input)?;
        Ok(match self {
            Self::Amm { venue } => Call::SwapExactIn {
                pool: venue.clone(),
                asset_in: input.into(),
                amount: budget,
                min_out: 1.into(),
            },
            Self::Clmm { venue, limit_tick } => Call::ClmmSwap {
                pool: venue.clone(),
                asset_in: input.into(),
                amount: budget,
                sqrt_price_limit: crate::clmm::math::tick_sqrt(*limit_tick)?,
                min_out: 1.into(),
                allow_partial: true,
            },
            Self::Book {
                venue,
                limit_price_ticks,
            } => {
                use crate::book::{Side, TimeInForce};
                let cfg = &s.books[venue];
                let side = if input == cfg.base_asset {
                    Side::Sell
                } else {
                    Side::Buy
                };
                let per_lot = cfg.reservation(side, *limit_price_ticks, 1.into())?;
                let lots = Amount::from_big(budget.big() / per_lot.big())?;
                if lots.is_zero() {
                    return Err("route budget below one book lot".into());
                }
                Call::PlaceOrder {
                    book: venue.clone(),
                    side,
                    price_ticks: *limit_price_ticks,
                    lots,
                    tif: TimeInForce::IOC,
                }
            }
        })
    }
}
pub fn validate_hops(s: &Scenario, asset_in: &str, hops: &[VenueHop]) -> Result<String, String> {
    if !s.model_versions.iter().any(|v| v == MODEL) {
        return Err("missing heterogeneous route model version".into());
    }
    if hops.is_empty() || hops.len() > MAX_HOPS {
        return Err("route requires 1..3 hops".into());
    }
    let mut seen = BTreeSet::new();
    let mut asset = asset_in.to_owned();
    for hop in hops {
        if !seen.insert(hop.venue()) {
            return Err("repeated route venue".into());
        }
        asset = hop.other(s, &asset)?;
    }
    Ok(asset)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HopResult {
    pub hop: VenueHop,
    pub asset_in: String,
    pub asset_out: String,
    pub budget: Amount,
    pub spent: Amount,
    pub unspent: Amount,
    pub output: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RouteResult {
    pub asset_in: String,
    pub asset_out: String,
    pub input_budget: Amount,
    pub input_spent: Amount,
    pub input_unspent: Amount,
    pub output: Amount,
    pub residual_policy: ResidualPolicy,
    pub residuals: BTreeMap<String, Amount>,
    pub hops: Vec<HopResult>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Family {
    Cpmm,
    StableSwap,
    Clmm,
    Book,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Venue {
    pub id: String,
    pub x: String,
    pub y: String,
    pub family: Family,
}
pub fn venues(s: &Scenario) -> Vec<Venue> {
    let mut venues: Vec<_> = s
        .pools()
        .map(|(id, p)| Venue {
            id: id.clone(),
            x: p.x.clone(),
            y: p.y.clone(),
            family: if s.stable_pools.contains_key(id) {
                Family::StableSwap
            } else {
                Family::Cpmm
            },
        })
        .chain(s.clmm_pools.iter().map(|(id, p)| Venue {
            id: id.clone(),
            x: p.x.clone(),
            y: p.q.clone(),
            family: Family::Clmm,
        }))
        .chain(s.books.iter().map(|(id, p)| Venue {
            id: id.clone(),
            x: p.base_asset.clone(),
            y: p.quote_asset.clone(),
            family: Family::Book,
        }))
        .collect();
    venues.sort_by(|a, b| a.id.cmp(&b.id));
    venues
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DirectionalLimit {
    pub asset_in: String,
    pub hop: VenueHop,
}
pub fn validate_limits(venues: &[Venue], limits: &[DirectionalLimit]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    // Only CLMM/book require explicit limits, with at most two directions per venue.
    if limits.len() > 2 * venues.len() {
        return Err("too many directional limits".into());
    }
    for limit in limits {
        let v = venues
            .iter()
            .find(|v| v.id == limit.hop.venue())
            .ok_or("unknown limit venue")?;
        if (limit.asset_in != v.x && limit.asset_in != v.y)
            || !seen.insert((limit.hop.venue(), &limit.asset_in))
        {
            return Err("duplicate or invalid directional limit".into());
        }
        match (&limit.hop, v.family) {
            (VenueHop::Clmm { limit_tick, .. }, Family::Clmm)
                if (crate::clmm::math::MIN_TICK..=crate::clmm::math::MAX_TICK)
                    .contains(limit_tick) => {}
            (
                VenueHop::Book {
                    limit_price_ticks, ..
                },
                Family::Book,
            ) if !limit_price_ticks.is_zero() => {}
            _ => return Err("invalid directional limit type/value".into()),
        }
    }
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub agent: String,
    pub sender: String,
    pub asset_in: String,
    pub asset_out: String,
    pub amount: Amount,
    pub min_out: Amount,
    pub residual_policy: ResidualPolicy,
    pub resource_limit: Amount,
    pub resource_price: Amount,
    pub limits: Vec<DirectionalLimit>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub hops: Vec<VenueHop>,
    pub transaction: Transaction,
    pub receipt: Option<Receipt>,
    pub admission_error: Option<String>,
}
impl Candidate {
    pub fn result(&self) -> Option<&RouteResult> {
        self.receipt
            .as_ref()
            .filter(|r| r.status == "Success")?
            .route_results
            .get("0")
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Search {
    pub candidates: Vec<Candidate>,
    pub examined: Amount,
    pub truncated: bool,
}
pub enum Trial {
    Executed(Box<Receipt>),
    Rejected(String),
}
/// Enumeration uses public topology only, never future inputs or other wallets.
/// The controller supplies a read-only execution capability, bounded to 64 calls.
pub fn search_with(
    obs: &super::Observation,
    chain: &Chain,
    query: &Query,
    simulate: &dyn Fn(&Transaction) -> Result<Trial, String>,
) -> Result<Search, String> {
    search_with_tail(obs, chain, query, &[], simulate)
}
pub(crate) fn search_with_tail(
    obs: &super::Observation,
    chain: &Chain,
    query: &Query,
    tail: &[Call],
    simulate: &dyn Fn(&Transaction) -> Result<Trial, String>,
) -> Result<Search, String> {
    validate_limits(&obs.route_venues, &query.limits)?;
    if query.sender != obs.account || query.amount.is_zero() || query.resource_limit < 100.into() {
        return Err("invalid route query".into());
    }
    query.resource_limit.checked_mul(query.resource_price)?;
    let mut paths = vec![];
    fn visit(
        venues: &[Venue],
        q: &Query,
        asset: &str,
        path: &mut Vec<VenueHop>,
        paths: &mut Vec<Vec<VenueHop>>,
    ) {
        if path.len() == MAX_HOPS || paths.len() > MAX_CANDIDATES {
            return;
        }
        for v in venues {
            if path.iter().any(|hop| hop.venue() == v.id) {
                continue;
            }
            let other = if asset == v.x {
                &v.y
            } else if asset == v.y {
                &v.x
            } else {
                continue;
            };
            let hop = match v.family {
                Family::Cpmm | Family::StableSwap => VenueHop::Amm {
                    venue: v.id.clone(),
                },
                _ => {
                    let Some(limit) = q
                        .limits
                        .iter()
                        .find(|l| l.hop.venue() == v.id && l.asset_in == asset)
                    else {
                        continue;
                    };
                    limit.hop.clone()
                }
            };
            path.push(hop);
            if other == &q.asset_out {
                paths.push(path.clone());
            } else {
                visit(venues, q, other, path, paths);
            }
            path.pop();
            if paths.len() > MAX_CANDIDATES {
                return;
            }
        }
    }
    let mut public_venues = obs.route_venues.clone();
    public_venues.sort_by(|a, b| a.id.cmp(&b.id));
    visit(
        &public_venues,
        query,
        &query.asset_in,
        &mut vec![],
        &mut paths,
    );
    let truncated = paths.len() > MAX_CANDIDATES;
    paths.truncate(MAX_CANDIDATES);
    let mut candidates = Vec::with_capacity(paths.len());
    for (i, hops) in paths.into_iter().enumerate() {
        let mut transaction = Transaction {
            id: format!("route-quote:{}:{i}", obs.block),
            agent: query.agent.clone(),
            sender: query.sender.clone(),
            chain: chain.id.clone(),
            nonce: obs.nonce,
            limit: query.resource_limit,
            price: query.resource_price,
            deadline: obs.timestamp.checked_add(chain.interval)?,
            program: vec![Call::ExecuteRoute {
                asset_in: query.asset_in.clone(),
                amount: query.amount,
                hops: hops.clone(),
                min_out: query.min_out,
                residual_policy: query.residual_policy,
            }],
        };
        transaction.program.extend_from_slice(tail);
        let (receipt, admission_error) = match simulate(&transaction)? {
            Trial::Executed(receipt) => (Some(*receipt), None),
            Trial::Rejected(error) => (None, Some(error)),
        };
        candidates.push(Candidate {
            hops,
            transaction,
            receipt,
            admission_error,
        });
    }
    candidates.sort_by(|a, b| {
        b.result()
            .map(|r| r.output)
            .cmp(&a.result().map(|r| r.output))
            .then_with(|| {
                a.receipt
                    .as_ref()
                    .map(|r| r.used)
                    .cmp(&b.receipt.as_ref().map(|r| r.used))
            })
            .then_with(|| {
                a.hops
                    .iter()
                    .map(VenueHop::venue)
                    .cmp(b.hops.iter().map(VenueHop::venue))
            })
    });
    Ok(Search {
        examined: (candidates.len() as u64).into(),
        candidates,
        truncated,
    })
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Quote {
    pub state_ref: String,
    pub block: Amount,
    pub search: Search,
}
pub fn quote(s: &Scenario, w: &World, query: &Query) -> Result<Quote, String> {
    if !s.assets.contains_key(&query.asset_in) || !s.assets.contains_key(&query.asset_out) {
        return Err("unknown route query asset".into());
    }
    let obs = super::Observation::capture(s, w, &query.sender)?;
    let search = search_with(&obs, &s.chain, query, &|tx| {
        crate::engine::probe_transaction(s, w, tx)
    })?;
    Ok(Quote {
        state_ref: crate::run::digest(&serde_json::to_vec(&(s, w)).map_err(|e| e.to_string())?),
        block: w.block,
        search,
    })
}
