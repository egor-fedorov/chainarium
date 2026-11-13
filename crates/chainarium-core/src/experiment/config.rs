use crate::{Amount, Scenario, World, model::MAX_SCENARIO, run::digest};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
pub const MODEL: &str = "paired-experiments-v1";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub cases: usize,
    pub variants: usize,
    pub runs: usize,
    pub blocks_per_run: usize,
    pub total_blocks: usize,
    pub metrics: usize,
    pub policy_targets: usize,
    pub rational_bits: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            cases: 64,
            variants: 8,
            runs: 128,
            blocks_per_run: 10000,
            total_blocks: 100000,
            metrics: 16,
            policy_targets: 8,
            rational_bits: 32768,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<(), String> {
        let values = serde_json::to_value(self).map_err(|e| e.to_string())?;
        let caps = serde_json::to_value(Self::default()).map_err(|e| e.to_string())?;
        for (key, value) in values.as_object().ok_or("experiment limits")? {
            let n = value.as_u64().ok_or("experiment limit")?;
            if n == 0
                || n > caps[key].as_u64().ok_or("experiment cap")?
                || (key == "rational_bits" && n < 512)
            {
                return Err(format!("experiment limit outside cap: {key}"));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Family {
    Arbitrage,
    LP,
    MM,
    Baseline,
    Reactive,
    Backrun,
    Liquidator,
    FlashArbitrage,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub family: Family,
    pub agent: String,
}
fn escape(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}
impl Target {
    fn path(&self) -> String {
        let parent = match self.family {
            Family::Arbitrage => "/policies",
            Family::LP => "/lp_policies",
            Family::MM => "/mm_policies",
            Family::Baseline => "/baseline_policies",
            Family::Reactive => "/scheduler/reactive",
            Family::Backrun => "/scheduler/backrun/policies",
            Family::Liquidator => "/scheduler/liquidator/policies",
            Family::FlashArbitrage => "/scheduler/flash_arbitrage/policies",
        };
        format!("{parent}/{}", escape(&self.agent))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Comparison {
    BuilderOrder,
    AgentPolicy { targets: Vec<Target> },
}
impl Comparison {
    pub(super) fn paths(&self) -> Vec<String> {
        match self {
            Self::BuilderOrder => vec!["/scheduler/builder".into()],
            Self::AgentPolicy { targets } => targets.iter().map(Target::path).collect(),
        }
    }
    pub fn validate(&self, limits: &Limits) -> Result<(), String> {
        if let Self::AgentPolicy { targets } = self
            && (targets.is_empty()
                || targets.len() > limits.policy_targets
                || targets.iter().collect::<BTreeSet<_>>().len() != targets.len()
                || targets.iter().any(|t| t.agent.is_empty()))
        {
            return Err("invalid experiment policy targets".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum IncludedStatus {
    Success,
    ProtocolRevert,
    ResourceExhausted,
}
impl IncludedStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "Success",
            Self::ProtocolRevert => "ProtocolRevert",
            Self::ResourceExhausted => "ResourceExhausted",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Metric {
    PortfolioPnl { portfolio: String },
    PaidNativeFees { accounts: Vec<String> },
    IncludedCount { status: IncludedStatus },
    InclusionRejectionAttempts,
}
impl Metric {
    pub fn unit(&self) -> &'static str {
        match self {
            Self::PortfolioPnl { .. } | Self::PaidNativeFees { .. } => "atoms Q",
            _ => "count",
        }
    }
    fn validate(&self, s: &Scenario) -> Result<(), String> {
        match self {
            Self::PortfolioPnl { portfolio } => {
                if !s
                    .valuation_policy
                    .as_ref()
                    .is_some_and(|p| p.portfolios.contains_key(portfolio))
                {
                    return Err("unknown experiment portfolio".into());
                }
            }
            Self::PaidNativeFees { accounts }
                if accounts.is_empty()
                    || accounts.iter().collect::<BTreeSet<_>>().len() != accounts.len()
                    || accounts.iter().any(|a| !s.accounts.contains_key(a)) =>
            {
                return Err("invalid experiment fee accounts".into());
            }
            _ => {}
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub seed: Amount,
    pub variants: BTreeMap<String, Scenario>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema_version: String,
    pub name: String,
    pub hypothesis: String,
    pub baseline_variant: String,
    pub comparison: Comparison,
    pub primary_metric: String,
    pub metrics: BTreeMap<String, Metric>,
    pub limits: Limits,
    pub cases: Vec<Case>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Difference {
    pub pointer: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub old: Option<Value>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub new: Option<Value>,
}
fn present_value<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}
pub fn differences(a: &Value, b: &Value) -> Vec<Difference> {
    fn walk(a: Option<&Value>, b: Option<&Value>, path: String, out: &mut Vec<Difference>) {
        if a == b {
            return;
        }
        if let (Some(Value::Object(a)), Some(Value::Object(b))) = (a, b) {
            for key in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
                walk(
                    a.get(key),
                    b.get(key),
                    format!("{path}/{}", escape(key)),
                    out,
                );
            }
        } else if let (Some(Value::Array(a)), Some(Value::Array(b))) = (a, b) {
            for i in 0..a.len().max(b.len()) {
                walk(a.get(i), b.get(i), format!("{path}/{i}"), out);
            }
        } else {
            out.push(Difference {
                pointer: path,
                old: a.cloned(),
                new: b.cloned(),
            });
        }
    }
    let mut out = vec![];
    walk(Some(a), Some(b), String::new(), &mut out);
    out
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VariantInfo {
    pub scenario_sha256: String,
    pub differences: Vec<Difference>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CaseInfo {
    pub seed: Amount,
    pub common_input_sha256: String,
    pub external_inputs_sha256: String,
    pub variants: BTreeMap<String, VariantInfo>,
}
pub(super) fn hash<T: Serialize>(value: &T) -> Result<String, String> {
    Ok(digest(
        &serde_json::to_vec(value).map_err(|e| e.to_string())?,
    ))
}
fn remove(v: &mut Value, path: &str) -> Result<Value, String> {
    let (parent, key) = path.rsplit_once('/').ok_or("invalid comparison pointer")?;
    v.pointer_mut(parent)
        .and_then(Value::as_object_mut)
        .and_then(|o| o.remove(&key.replace("~1", "/").replace("~0", "~")))
        .ok_or_else(|| format!("missing intervention target: {path}"))
}
fn common(mut raw: Value, paths: &[String]) -> Result<Value, String> {
    remove(&mut raw, "/name")?;
    for path in paths {
        remove(&mut raw, path)?;
    }
    Ok(raw)
}
fn across_cases(mut raw: Value) -> Result<Value, String> {
    remove(&mut raw, "/name")?;
    remove(&mut raw, "/seed")?;
    raw["blocks"] = serde_json::json!([]);
    if raw.get("scheduler").is_some() {
        raw["scheduler"]["emissions"] = serde_json::json!([]);
        if raw["scheduler"].get("bundles").is_some() {
            raw["scheduler"]["bundles"]["emissions"] = serde_json::json!([]);
        }
    }
    if let Some(feeds) = raw
        .pointer_mut("/valuation_policy/reference_feeds")
        .and_then(Value::as_object_mut)
    {
        for feed in feeds.values_mut() {
            feed["records"] = serde_json::json!([]);
        }
    }
    Ok(raw)
}
impl Plan {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_SCENARIO {
            return Err("experiment plan exceeds 64 MiB".into());
        }
        let mut plan: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("experiment input: {e}"))?;
        plan.cases.sort_by_key(|c| c.seed);
        plan.validate()?;
        Ok(plan)
    }
    pub fn validate(&self) -> Result<Vec<CaseInfo>, String> {
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_SCENARIO {
            return Err("experiment plan exceeds 64 MiB".into());
        }
        self.limits.validate()?;
        self.comparison.validate(&self.limits)?;
        if self.schema_version != MODEL
            || self.name.is_empty()
            || self.hypothesis.is_empty()
            || self.cases.is_empty()
            || self.cases.len() > self.limits.cases
            || self.metrics.is_empty()
            || self.metrics.len() > self.limits.metrics
            || self.metrics.keys().any(String::is_empty)
            || !matches!(
                self.metrics.get(&self.primary_metric),
                Some(Metric::PortfolioPnl { .. })
            )
        {
            return Err("invalid experiment header/metrics/cases".into());
        }
        let mut sorted: Vec<_> = self.cases.iter().collect();
        sorted.sort_by_key(|c| c.seed);
        if sorted.windows(2).any(|w| w[0].seed == w[1].seed) {
            return Err("duplicate experiment seed".into());
        }
        let variants: Vec<_> = sorted[0].variants.keys().cloned().collect();
        if variants.len() < 2
            || variants.len() > self.limits.variants
            || variants.iter().any(String::is_empty)
            || !variants.contains(&self.baseline_variant)
            || variants
                .len()
                .checked_mul(sorted.len())
                .ok_or("experiment run overflow")?
                > self.limits.runs
        {
            return Err("invalid experiment variants/run cap".into());
        }
        let paths = self.comparison.paths();
        let mut cross_case = BTreeMap::new();
        let mut horizon = None;
        let mut total_blocks = 0usize;
        let mut infos = vec![];
        for case in sorted {
            if case.variants.keys().ne(variants.iter()) {
                return Err("experiment variant sets differ".into());
            }
            let base = serde_json::to_value(&case.variants[&self.baseline_variant])
                .map_err(|e| e.to_string())?;
            let expected = common(base.clone(), &paths)?;
            let mut info = CaseInfo {
                seed: case.seed,
                common_input_sha256: hash(&expected)?,
                external_inputs_sha256: hash(
                    &serde_json::json!({"blocks":base["blocks"],"emissions":base.pointer("/scheduler/emissions"),"bundle_emissions":base.pointer("/scheduler/bundles/emissions"),"reference_feeds":base.pointer("/valuation_policy/reference_feeds")}),
                )?,
                variants: BTreeMap::new(),
            };
            for (id, s) in &case.variants {
                s.validate()?;
                World::genesis(s)?;
                if s.seed != case.seed
                    || s.chain.fee_asset != "Q"
                    || s.valuation_policy.is_none()
                    || s.rounds() > self.limits.blocks_per_run
                {
                    return Err("invalid experiment seed/fee/valuation/horizon".into());
                }
                if let Some(n) = horizon {
                    if n != s.rounds() {
                        return Err("experiment horizons differ".into());
                    }
                } else {
                    horizon = Some(s.rounds());
                }
                total_blocks = total_blocks
                    .checked_add(s.rounds())
                    .ok_or("experiment block overflow")?;
                if total_blocks > self.limits.total_blocks {
                    return Err("experiment total block cap".into());
                }
                if matches!(self.comparison, Comparison::BuilderOrder) {
                    let c = s
                        .scheduler
                        .as_ref()
                        .ok_or("BuilderOrder requires scheduler")?;
                    if !s.policies.is_empty()
                        || !s.lp_policies.is_empty()
                        || !s.mm_policies.is_empty()
                        || !s.baseline_policies.is_empty()
                        || !c.reactive.is_empty()
                        || c.backrun.is_some()
                        || c.liquidator.is_some()
                        || c.flash_arbitrage.is_some()
                    {
                        return Err("BuilderOrder forbids autonomous policies".into());
                    }
                }
                for m in self.metrics.values() {
                    m.validate(s)?;
                }
                let raw = serde_json::to_value(s).map_err(|e| e.to_string())?;
                for path in &paths {
                    if raw.pointer(path).and_then(|v| v.get("account"))
                        != base.pointer(path).and_then(|v| v.get("account"))
                    {
                        return Err("experiment cannot change policy account".into());
                    }
                }
                if common(raw.clone(), &paths)? != expected {
                    return Err(format!(
                        "undeclared experiment difference: seed {}, variant {id}",
                        case.seed
                    ));
                }
                let masked = across_cases(raw.clone())?;
                if let Some(prior) = cross_case.get(id) {
                    if prior != &masked {
                        return Err(format!(
                            "treatment or fixed environment changes across seeds: {id}"
                        ));
                    }
                } else {
                    cross_case.insert(id.clone(), masked);
                }
                info.variants.insert(
                    id.clone(),
                    VariantInfo {
                        scenario_sha256: hash(s)?,
                        differences: differences(&base, &raw),
                    },
                );
            }
            infos.push(info);
        }
        Ok(infos)
    }
}
