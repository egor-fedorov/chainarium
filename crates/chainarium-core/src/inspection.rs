//! Read-only chart data uses the same venue quotes as execution; no floating-point money.
use crate::{Amount, Scenario, World, engine};
use serde::Serialize;
#[derive(Serialize)]
pub struct QuotePoint {
    pub input: Amount,
    pub output: Option<Amount>,
    pub error: Option<String>,
}
#[derive(Serialize)]
pub struct Curve {
    pub venue: String,
    pub model: String,
    pub asset_in: String,
    pub asset_out: String,
    pub reserve_in: Amount,
    pub reserve_out: Amount,
    pub points: Vec<QuotePoint>,
}
#[derive(Serialize)]
pub struct MarketView {
    pub venue: String,
    pub model: String,
    pub parameters: Option<crate::stable::Parameters>,
    pub normalized: Option<[Amount; 2]>,
    pub invariant: Option<Amount>,
    pub invariant_error: Option<String>,
    pub curves: Vec<Curve>,
}
pub fn market(s: &Scenario, w: &World, id: &str) -> Result<MarketView, String> {
    let pool = s.pool(id).ok_or("unknown venue")?;
    let parameters = s
        .stable_pools
        .get(id)
        .map(|p| p.parameters(s))
        .transpose()?;
    let normalized = parameters
        .as_ref()
        .map(|p| {
            p.normalized([
                w.balance(&pool.account, &pool.x),
                w.balance(&pool.account, &pool.y),
            ])
        })
        .transpose()?;
    let (invariant, invariant_error) = if let (Some(p), Some(xp)) = (&parameters, normalized) {
        match crate::stable::invariant(xp, p, &mut || Ok::<(), String>(()), &mut vec![]) {
            Ok(d) => (Some(d), None),
            Err(e) => (None, Some(e)),
        }
    } else {
        (None, None)
    };
    let model = |id: &str| {
        if s.stable_pools.contains_key(id) {
            crate::stable::MODEL
        } else {
            "cpmm-v1"
        }
        .to_string()
    };
    let inputs = [1, 5, 10, 20, 50]
        .map(|n| {
            w.balance(&pool.account, &pool.x)
                .mul_div(Amount::from(n), 100.into(), false)
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let mut curves = vec![];
    for (other, p) in s.pools() {
        if !((p.x == pool.x && p.y == pool.y) || (p.x == pool.y && p.y == pool.x)) {
            continue;
        }
        let points = inputs
            .iter()
            .map(|input| {
                let (output, error) = match engine::quote(s, w, other, &pool.x, *input) {
                    Ok((_, out)) => (Some(out), None),
                    Err(e) => (None, Some(e)),
                };
                QuotePoint {
                    input: *input,
                    output,
                    error,
                }
            })
            .collect();
        curves.push(Curve {
            venue: other.clone(),
            model: model(other),
            asset_in: pool.x.clone(),
            asset_out: pool.y.clone(),
            reserve_in: w.balance(&p.account, &pool.x),
            reserve_out: w.balance(&p.account, &pool.y),
            points,
        });
    }
    curves.sort_by(|a, b| a.venue.cmp(&b.venue));
    Ok(MarketView {
        venue: id.into(),
        model: model(id),
        parameters,
        normalized,
        invariant,
        invariant_error,
        curves,
    })
}
