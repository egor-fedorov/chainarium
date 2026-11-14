use crate::Scenario;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const MODEL: &str = "flow-matched-hodl-v1";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub portfolios: usize,
    pub assets_per_portfolio: usize,
    pub flow_events_per_portfolio: usize,
    pub rational_bits: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            portfolios: 16,
            assets_per_portfolio: 64,
            flow_events_per_portfolio: 4096,
            rational_bits: 32768,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub version: String,
    pub portfolios: Vec<String>,
    pub limits: Limits,
}
pub fn validate(s: &Scenario) -> Result<(), String> {
    let has = s.model_versions.iter().any(|v| v == MODEL);
    let Some(p) = &s.hodl_policy else {
        return if has {
            Err("HODL model/config mismatch".into())
        } else {
            Ok(())
        };
    };
    let valuation = s
        .valuation_policy
        .as_ref()
        .ok_or("HODL requires reference MTM")?;
    if !has || p.version != MODEL {
        return Err("invalid HODL profile".into());
    }
    let values = serde_json::to_value(&p.limits).map_err(|e| e.to_string())?;
    let caps = serde_json::to_value(Limits::default()).map_err(|e| e.to_string())?;
    for (key, value) in values.as_object().ok_or("HODL limits")? {
        let n = value.as_u64().ok_or("HODL limit")?;
        if n == 0
            || n > caps[key].as_u64().ok_or("HODL cap")?
            || (key == "rational_bits" && n < 512)
        {
            return Err(format!("HODL limit outside cap: {key}"));
        }
    }
    if p.portfolios.is_empty()
        || p.portfolios.len() > p.limits.portfolios
        || p.portfolios.iter().collect::<BTreeSet<_>>().len() != p.portfolios.len()
        || p.portfolios
            .iter()
            .any(|id| !valuation.portfolios.contains_key(id))
    {
        return Err("invalid HODL portfolios".into());
    }
    Ok(())
}
