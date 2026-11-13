use crate::{Amount, valuation::Exact};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", deny_unknown_fields)]
pub enum Value {
    Available { value: Exact },
    Unavailable { reasons: BTreeSet<String> },
}
impl Value {
    pub fn available(value: Exact) -> Self {
        Self::Available { value }
    }
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reasons: BTreeSet::from([reason.into()]),
        }
    }
    pub fn from_result(result: Result<Exact, String>) -> Self {
        match result {
            Ok(v) => Self::available(v),
            Err(e) => Self::unavailable(e),
        }
    }
    pub fn exact(&self) -> Option<&Exact> {
        match self {
            Self::Available { value } => Some(value),
            _ => None,
        }
    }
    pub fn reasons(&self) -> BTreeSet<String> {
        match self {
            Self::Available { .. } => BTreeSet::new(),
            Self::Unavailable { reasons } => reasons.clone(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Statistics {
    pub minimum: Value,
    pub maximum: Value,
    pub mean: Value,
    pub median: Value,
    pub p05: Value,
    pub p95: Value,
    pub sample_variance: Value,
    pub negative: Amount,
    pub zero: Amount,
    pub positive: Amount,
}
/// Input order is numeric seed, never value order: all bounded accumulation uses
/// this order. Only order statistics sort a copy, with exact cross-products.
pub fn statistics(values: &[Exact], bits: u64) -> Statistics {
    if values.is_empty() {
        let missing = Value::unavailable("NoPricedObservations");
        return Statistics {
            minimum: missing.clone(),
            maximum: missing.clone(),
            mean: missing.clone(),
            median: missing.clone(),
            p05: missing.clone(),
            p95: missing.clone(),
            sample_variance: missing,
            negative: 0.into(),
            zero: 0.into(),
            positive: 0.into(),
        };
    }
    let n = values.len();
    let mut sorted: Vec<_> = values.iter().collect();
    sorted.sort();
    let mean = values
        .iter()
        .try_fold(Exact::default(), |sum, x| sum.add(x, bits))
        .and_then(|sum| sum.div_count(n, bits));
    let median = if n % 2 == 1 {
        sorted[n / 2].bounded(bits)
    } else {
        sorted[n / 2 - 1]
            .add(sorted[n / 2], bits)
            .and_then(|v| v.div_count(2, bits))
    };
    let variance = if n == 1 {
        Err("InsufficientSample".into())
    } else {
        mean.clone()
            .and_then(|mean| {
                values.iter().try_fold(Exact::default(), |sum, x| {
                    let delta = x.sub(&mean, bits)?;
                    sum.add(&delta.mul(&delta, bits)?, bits)
                })
            })
            .and_then(|sum| sum.div_count(n - 1, bits))
    };
    let zero = Exact::default();
    Statistics {
        minimum: Value::from_result(sorted[0].bounded(bits)),
        maximum: Value::from_result(sorted[n - 1].bounded(bits)),
        mean: Value::from_result(mean),
        median: Value::from_result(median),
        p05: Value::from_result(sorted[(5 * n).div_ceil(100) - 1].bounded(bits)),
        p95: Value::from_result(sorted[(95 * n).div_ceil(100) - 1].bounded(bits)),
        sample_variance: Value::from_result(variance),
        negative: (values.iter().filter(|v| *v < &zero).count() as u64).into(),
        zero: (values.iter().filter(|v| v.is_zero()).count() as u64).into(),
        positive: (values.iter().filter(|v| *v > &zero).count() as u64).into(),
    }
}
