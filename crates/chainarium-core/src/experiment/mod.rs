//! Accepted DEC-29: materialized paired inputs, sequential native execution and
//! exact statistics. The runner never chooses or consumes an RNG stream.
mod config;
mod export;
mod statistics;
#[cfg(test)]
mod tests;
use crate::{
    Amount, Run,
    valuation::{self, Exact},
};
pub use config::*;
pub use export::export;
use serde::{Deserialize, Serialize};
pub use statistics::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum RunStatus {
    Completed,
    ModelError { error: String },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Outcomes {
    pub included: BTreeMap<String, Amount>,
    pub included_failure_fraction: Value,
    pub inclusion_rejection_attempts: Amount,
    pub scheduler_actions: BTreeMap<String, Amount>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub seed: Amount,
    pub variant: String,
    pub run_id: String,
    pub path: String,
    pub status: RunStatus,
    pub committed_blocks: Amount,
    pub horizon: Amount,
    pub metrics: BTreeMap<String, Value>,
    pub prefix_metrics: Option<BTreeMap<String, Value>>,
    pub outcomes: Outcomes,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    pub seed: Amount,
    pub variant: String,
    pub metrics: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Excluded {
    pub seed: Amount,
    pub reasons: BTreeMap<String, BTreeSet<String>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Cohort {
    pub requested_n: Amount,
    pub completed_n: Amount,
    pub eligible_n: Amount,
    pub eligible_seeds: Vec<Amount>,
    pub excluded: Vec<Excluded>,
    pub statistics: Statistics,
    pub unit: String,
    pub variance_unit: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Analysis {
    pub pairs: Vec<Pair>,
    pub variants: BTreeMap<String, BTreeMap<String, Cohort>>,
    pub paired: BTreeMap<String, BTreeMap<String, Cohort>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub plan: Plan,
    pub schema_version: String,
    pub build_id: String,
    pub plan_sha256: String,
    pub name: String,
    pub hypothesis: String,
    pub baseline_variant: String,
    pub comparison: Comparison,
    pub primary_metric: String,
    pub metrics: BTreeMap<String, Metric>,
    pub limits: Limits,
    pub cases: Vec<CaseInfo>,
    pub outcome: String,
    pub rows: Vec<Row>,
    pub analysis: Analysis,
}
fn count(n: usize) -> Amount {
    (n as u64).into()
}
fn outcomes(run: &Run, bits: u64) -> Result<Outcomes, String> {
    let mut included = BTreeMap::from([
        ("Success".into(), Amount::ZERO),
        ("ProtocolRevert".into(), Amount::ZERO),
        ("ResourceExhausted".into(), Amount::ZERO),
    ]);
    let mut rejected = Amount::ZERO;
    let mut actions = BTreeMap::<String, Amount>::new();
    for b in &run.blocks {
        rejected = rejected.checked_add(count(b.rejected.len()))?;
        for r in &b.receipts {
            let n = included
                .get_mut(&r.status)
                .ok_or("unknown native receipt status")?;
            *n = n.checked_add(1.into())?;
        }
        if let Some(s) = &b.scheduler {
            for trace in &s.trace {
                let n = actions.entry(trace.action.clone()).or_default();
                *n = n.checked_add(1.into())?;
            }
        }
    }
    let failures = included["ProtocolRevert"].checked_add(included["ResourceExhausted"])?;
    let total = included["Success"].checked_add(failures)?;
    let fraction = if total.is_zero() {
        Value::unavailable("NoIncludedReceipts")
    } else {
        Value::from_result(Exact::ratio(failures.big().into(), total.big(), bits))
    };
    Ok(Outcomes {
        included,
        included_failure_fraction: fraction,
        inclusion_rejection_attempts: rejected,
        scheduler_actions: actions,
    })
}
fn metrics(
    run: &Run,
    selectors: &BTreeMap<String, Metric>,
    outcomes: &Outcomes,
    bits: u64,
) -> Result<BTreeMap<String, Value>, String> {
    let boundary = valuation::at(run, run.blocks.len())?;
    let mut result = BTreeMap::new();
    for (id, m) in selectors {
        let value = match m {
            Metric::PortfolioPnl { portfolio } => {
                let mark = &boundary
                    .portfolios
                    .iter()
                    .find(|p| &p.portfolio == portfolio)
                    .ok_or("missing portfolio")?
                    .pnl;
                match &mark.value {
                    Some(v) => Value::available(v.clone()),
                    None => Value::Unavailable {
                        reasons: mark.reasons.clone(),
                    },
                }
            }
            Metric::PaidNativeFees { accounts } => Value::from_result(
                run.blocks
                    .iter()
                    .flat_map(|b| &b.receipts)
                    .filter(|r| accounts.contains(&r.sender))
                    .try_fold(Exact::default(), |sum, r| {
                        sum.add(&Exact::integer(r.fee), bits)
                    }),
            ),
            Metric::IncludedCount { status } => {
                Value::available(Exact::integer(outcomes.included[status.as_str()]))
            }
            Metric::InclusionRejectionAttempts => {
                Value::available(Exact::integer(outcomes.inclusion_rejection_attempts))
            }
        };
        result.insert(id.clone(), value);
    }
    Ok(result)
}
fn row(
    plan: &Plan,
    run: &Run,
    seed: Amount,
    variant: &str,
    index: usize,
    status: RunStatus,
) -> Result<Row, String> {
    let outcomes = outcomes(run, plan.limits.rational_bits)?;
    let mut metrics = metrics(run, &plan.metrics, &outcomes, plan.limits.rational_bits)?;
    let prefix_metrics = if matches!(status, RunStatus::ModelError { .. }) {
        let prefix = metrics.clone();
        for v in metrics.values_mut() {
            *v = Value::unavailable("ModelError");
        }
        Some(prefix)
    } else {
        None
    };
    Ok(Row {
        seed,
        variant: variant.into(),
        run_id: run.run_id.clone(),
        path: format!("runs/{index:04}"),
        status,
        committed_blocks: count(run.blocks.len()),
        horizon: count(run.scenario.rounds()),
        metrics,
        prefix_metrics,
        outcomes,
    })
}
type Observation = (Amount, bool, Value, BTreeMap<String, BTreeSet<String>>);
fn cohort(observations: Vec<Observation>, unit: &str, bits: u64) -> Cohort {
    let requested_n = count(observations.len());
    let completed_n = count(observations.iter().filter(|(_, c, _, _)| *c).count());
    let mut eligible_seeds = vec![];
    let mut values = vec![];
    let mut excluded = vec![];
    for (seed, _, value, mut reasons) in observations {
        if let Some(v) = value.exact() {
            eligible_seeds.push(seed);
            values.push(v.clone());
        } else {
            if reasons.is_empty() {
                reasons.insert("metric".into(), value.reasons());
            }
            excluded.push(Excluded { seed, reasons });
        }
    }
    Cohort {
        requested_n,
        completed_n,
        eligible_n: count(values.len()),
        eligible_seeds,
        excluded,
        statistics: statistics(&values, bits),
        unit: unit.into(),
        variance_unit: format!("({unit})²"),
    }
}
fn analyze(
    rows: &[Row],
    metric_defs: &BTreeMap<String, Metric>,
    baseline: &str,
    bits: u64,
) -> Result<Analysis, String> {
    let ids: BTreeSet<_> = rows.iter().map(|r| r.variant.clone()).collect();
    let seeds: BTreeSet<_> = rows.iter().map(|r| r.seed).collect();
    let lookup: BTreeMap<_, _> = rows
        .iter()
        .map(|r| ((r.seed, r.variant.as_str()), r))
        .collect();
    let mut variants = BTreeMap::new();
    let mut paired = BTreeMap::new();
    let mut pairs = vec![];
    for id in &ids {
        let mut summaries = BTreeMap::new();
        let mut pair_summaries = BTreeMap::new();
        for (metric, selector) in metric_defs {
            let mut observations = vec![];
            let mut pair_observations = vec![];
            for seed in &seeds {
                let row = lookup
                    .get(&(*seed, id.as_str()))
                    .ok_or("missing experiment variant")?;
                let completed = matches!(row.status, RunStatus::Completed);
                let value = row
                    .metrics
                    .get(metric)
                    .ok_or("missing experiment metric")?
                    .clone();
                observations.push((*seed, completed, value.clone(), BTreeMap::new()));
                if id != baseline {
                    let base = lookup
                        .get(&(*seed, baseline))
                        .ok_or("missing experiment baseline")?;
                    let a = base.metrics.get(metric).ok_or("missing baseline metric")?;
                    let mut reasons = BTreeMap::new();
                    if a.exact().is_none() {
                        reasons.insert("baseline".into(), a.reasons());
                    }
                    if value.exact().is_none() {
                        reasons.insert("variant".into(), value.reasons());
                    }
                    let delta = match (value.exact(), a.exact()) {
                        (Some(v), Some(a)) => Value::from_result(v.sub(a, bits)),
                        _ => Value::unavailable("UnavailablePair"),
                    };
                    pair_observations.push((
                        *seed,
                        completed && matches!(base.status, RunStatus::Completed),
                        delta,
                        reasons,
                    ));
                }
            }
            summaries.insert(metric.clone(), cohort(observations, selector.unit(), bits));
            if id != baseline {
                pair_summaries.insert(
                    metric.clone(),
                    cohort(pair_observations, selector.unit(), bits),
                );
            }
        }
        variants.insert(id.clone(), summaries);
        if id != baseline {
            paired.insert(id.clone(), pair_summaries);
        }
    }
    for seed in seeds {
        for id in ids.iter().filter(|id| id.as_str() != baseline) {
            let r = lookup[&(seed, id.as_str())];
            let b = lookup[&(seed, baseline)];
            let metrics = metric_defs
                .keys()
                .map(|m| {
                    let value = match (r.metrics[m].exact(), b.metrics[m].exact()) {
                        (Some(v), Some(a)) => Value::from_result(v.sub(a, bits)),
                        _ => Value::unavailable("UnavailablePair"),
                    };
                    (m.clone(), value)
                })
                .collect();
            pairs.push(Pair {
                seed,
                variant: id.clone(),
                metrics,
            });
        }
    }
    Ok(Analysis {
        pairs,
        variants,
        paired,
    })
}
impl Report {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > crate::model::MAX_SCENARIO {
            return Err("browser experiment report exceeds 64 MiB".into());
        }
        let report: Self =
            serde_json::from_slice(bytes).map_err(|e| format!("experiment report: {e}"))?;
        report.validate()?;
        Ok(report)
    }
    pub fn validate(&self) -> Result<(), String> {
        self.limits.validate()?;
        self.comparison.validate(&self.limits)?;
        if self.plan.cases.windows(2).any(|w| w[0].seed >= w[1].seed)
            || config::hash(&self.plan)? != self.plan_sha256
            || self.plan.validate()? != self.cases
            || self.name != self.plan.name
            || self.hypothesis != self.plan.hypothesis
            || self.baseline_variant != self.plan.baseline_variant
            || self.comparison != self.plan.comparison
            || self.primary_metric != self.plan.primary_metric
            || self.metrics != self.plan.metrics
            || self.limits != self.plan.limits
        {
            return Err("report differs from canonical plan".into());
        }
        let hash_ok = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if self.schema_version != MODEL
            || self.name.is_empty()
            || self.hypothesis.is_empty()
            || !hash_ok(&self.build_id)
            || !hash_ok(&self.plan_sha256)
            || self.cases.is_empty()
            || self.cases.len() > self.limits.cases
            || self.rows.len() > self.limits.runs
            || self.metrics.is_empty()
            || self.metrics.len() > self.limits.metrics
            || self.metrics.keys().any(String::is_empty)
            || !matches!(
                self.metrics.get(&self.primary_metric),
                Some(Metric::PortfolioPnl { .. })
            )
            || !matches!(
                self.outcome.as_str(),
                "Completed" | "CompletedWithModelErrors"
            )
        {
            return Err("invalid or incomplete experiment report".into());
        }
        if self.cases.windows(2).any(|w| w[0].seed >= w[1].seed) {
            return Err("report seeds must be strictly ordered".into());
        }
        let variants: Vec<_> = self.cases[0].variants.keys().collect();
        if variants.len() < 2
            || variants.len() > self.limits.variants
            || variants.iter().any(|v| v.is_empty())
            || !self.cases[0].variants.contains_key(&self.baseline_variant)
            || self.rows.len() != self.cases.len() * variants.len()
        {
            return Err("report variant/run shape".into());
        }
        let mut total = Amount::ZERO;
        let mut index = 0;
        let mut model_error = false;
        let mut horizon = None;
        for (case_index, c) in self.cases.iter().enumerate() {
            if c.variants.keys().ne(variants.iter().copied())
                || !hash_ok(&c.common_input_sha256)
                || !hash_ok(&c.external_inputs_sha256)
            {
                return Err("report common inputs shape".into());
            }
            for (variant, info) in &c.variants {
                let r = &self.rows[index];
                if r.seed != c.seed
                    || &r.variant != variant
                    || r.path != format!("runs/{index:04}")
                    || !hash_ok(&info.scenario_sha256)
                    || r.run_id != info.scenario_sha256
                    || r.committed_blocks > r.horizon
                    || r.horizon != count(self.plan.cases[case_index].variants[variant].rounds())
                    || r.horizon > count(self.limits.blocks_per_run)
                    || r.metrics.keys().ne(self.metrics.keys())
                {
                    return Err("report row metadata mismatch".into());
                }
                if let Some(n) = horizon {
                    if r.horizon != n {
                        return Err("report horizon mismatch".into());
                    }
                } else {
                    horizon = Some(r.horizon);
                }
                total = total.checked_add(r.horizon)?;
                if total > count(self.limits.total_blocks) {
                    return Err("report total horizon cap".into());
                }
                let valid_values = |ms: &BTreeMap<String, Value>| {
                    ms.values().all(|v| match v {
                        Value::Available { .. } => true,
                        Value::Unavailable { reasons } => {
                            !reasons.is_empty() && reasons.iter().all(|r| !r.is_empty())
                        }
                    })
                };
                if !valid_values(&r.metrics) {
                    return Err("empty unavailability reason".into());
                }
                match &r.status {
                    RunStatus::Completed => {
                        if r.committed_blocks != r.horizon || r.prefix_metrics.is_some() {
                            return Err("incomplete Completed row".into());
                        }
                    }
                    RunStatus::ModelError { error } => {
                        model_error = true;
                        if error.is_empty()
                            || r.committed_blocks >= r.horizon
                            || r.metrics
                                .values()
                                .any(|v| v != &Value::unavailable("ModelError"))
                            || !r.prefix_metrics.as_ref().is_some_and(|m| {
                                m.keys().eq(self.metrics.keys()) && valid_values(m)
                            })
                        {
                            return Err("invalid ModelError prefix".into());
                        }
                    }
                }
                let expected_keys =
                    BTreeSet::from(["Success", "ProtocolRevert", "ResourceExhausted"]);
                if r.outcomes
                    .included
                    .keys()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>()
                    != expected_keys
                {
                    return Err("invalid included statuses".into());
                }
                let failed = r.outcomes.included["ProtocolRevert"]
                    .checked_add(r.outcomes.included["ResourceExhausted"])?;
                let included = failed.checked_add(r.outcomes.included["Success"])?;
                let expected = if included.is_zero() {
                    Value::unavailable("NoIncludedReceipts")
                } else {
                    Value::from_result(Exact::ratio(
                        failed.big().into(),
                        included.big(),
                        self.limits.rational_bits,
                    ))
                };
                if expected != r.outcomes.included_failure_fraction {
                    return Err("included failure fraction mismatch".into());
                }
                // Count selectors duplicate diagnostic fields. Validate both
                // representations even when final metrics exclude a ModelError
                // and its committed observations live in prefix_metrics.
                let observed_metrics = r.prefix_metrics.as_ref().unwrap_or(&r.metrics);
                for (id, selector) in &self.metrics {
                    let expected = match selector {
                        Metric::IncludedCount { status } => {
                            Some(r.outcomes.included[status.as_str()])
                        }
                        Metric::InclusionRejectionAttempts => {
                            Some(r.outcomes.inclusion_rejection_attempts)
                        }
                        _ => None,
                    };
                    if let Some(n) = expected
                        && observed_metrics[id] != Value::available(Exact::integer(n))
                    {
                        return Err("report count metric differs from outcomes".into());
                    }
                }
                index += 1;
            }
        }
        if (self.outcome == "CompletedWithModelErrors") != model_error {
            return Err("series outcome mismatch".into());
        }
        if analyze(
            &self.rows,
            &self.metrics,
            &self.baseline_variant,
            self.limits.rational_bits,
        )? != self.analysis
        {
            return Err("experiment summary/cohort mismatch".into());
        }
        Ok(())
    }
    /// A report never supplies a filesystem path to the controller. The user
    /// selects a checkpoint, which must replay and match this exact row/Scenario.
    pub fn restore_row(&self, index: usize, bytes: &[u8]) -> Result<Run, String> {
        self.validate()?;
        let expected = self.rows.get(index).ok_or("unknown experiment row")?;
        let run = Run::restore(bytes)?;
        if run.build_id != self.build_id
            || run.run_id != expected.run_id
            || config::hash(&run.scenario)? != expected.run_id
            || count(run.blocks.len()) != expected.committed_blocks
            || count(run.scenario.rounds()) != expected.horizon
        {
            return Err("checkpoint does not match selected experiment row".into());
        }
        let actual_outcomes = outcomes(&run, self.limits.rational_bits)?;
        let actual_metrics = metrics(
            &run,
            &self.metrics,
            &actual_outcomes,
            self.limits.rational_bits,
        )?;
        let expected_metrics = expected
            .prefix_metrics
            .as_ref()
            .unwrap_or(&expected.metrics);
        if actual_outcomes != expected.outcomes || &actual_metrics != expected_metrics {
            return Err("checkpoint results differ from report row".into());
        }
        if let RunStatus::ModelError { error } = &expected.status {
            let mut replay = run.clone();
            if replay.step().err().as_ref() != Some(error) {
                return Err("checkpoint does not reproduce reported ModelError".into());
            }
        }
        Ok(run)
    }
    pub(super) fn new(plan: &Plan, cases: Vec<CaseInfo>, rows: Vec<Row>) -> Result<Self, String> {
        let analysis = analyze(
            &rows,
            &plan.metrics,
            &plan.baseline_variant,
            plan.limits.rational_bits,
        )?;
        let outcome = if rows
            .iter()
            .any(|r| matches!(r.status, RunStatus::ModelError { .. }))
        {
            "CompletedWithModelErrors"
        } else {
            "Completed"
        }
        .into();
        Ok(Self {
            plan: plan.clone(),
            schema_version: MODEL.into(),
            build_id: crate::BUILD_ID.into(),
            plan_sha256: config::hash(plan)?,
            name: plan.name.clone(),
            hypothesis: plan.hypothesis.clone(),
            baseline_variant: plan.baseline_variant.clone(),
            comparison: plan.comparison.clone(),
            primary_metric: plan.primary_metric.clone(),
            metrics: plan.metrics.clone(),
            limits: plan.limits.clone(),
            cases,
            outcome,
            rows,
            analysis,
        })
    }
}
/// Fully validate before the first execution. The callback must export or consume
/// each run immediately; the runner retains only bounded rows, not prior Worlds.
pub fn execute(
    plan: &Plan,
    mut consume: impl FnMut(&Run, &Row) -> Result<(), String>,
) -> Result<Report, String> {
    let mut plan = plan.clone();
    plan.cases.sort_by_key(|c| c.seed);
    let infos = plan.validate()?;
    let mut rows = vec![];
    for case in &plan.cases {
        for (variant, s) in &case.variants {
            let mut run = Run::new(s.clone())?;
            let status = match run.finish() {
                Ok(()) => RunStatus::Completed,
                Err(error) => RunStatus::ModelError { error },
            };
            let result = row(&plan, &run, case.seed, variant, rows.len(), status)?;
            consume(&run, &result)?;
            rows.push(result);
        }
    }
    Report::new(&plan, infos, rows)
}
