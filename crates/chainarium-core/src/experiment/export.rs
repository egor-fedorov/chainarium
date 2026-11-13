use super::*;
use std::{
    fs,
    io::{BufWriter, Write},
    path::Path,
};
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let mut file = BufWriter::new(fs::File::create_new(path).map_err(|e| e.to_string())?);
    serde_json::to_writer(&mut file, value).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    file.get_ref().sync_all().map_err(|e| e.to_string())
}
/// The final manifest is also the importable report and the single commit marker.
/// Only owned files inside a newly created directory can be replaced.
pub fn export(plan: &Plan, path: &Path) -> Result<Report, String> {
    export_using(plan, path, |run, path| run.export(path))
}
pub(super) fn export_using(
    plan: &Plan,
    path: &Path,
    mut export_run: impl FnMut(&Run, &Path) -> Result<(), String>,
) -> Result<Report, String> {
    let mut plan = plan.clone();
    plan.cases.sort_by_key(|c| c.seed);
    plan.validate()?;
    fs::create_dir(path).map_err(|e| format!("create new experiment directory: {e}"))?;
    write_json(
        &path.join("manifest.json"),
        &serde_json::json!({"schema_version":MODEL,"outcome":"Incomplete","plan_sha256":config::hash(&plan)?,"build_id":crate::BUILD_ID}),
    )?;
    write_json(&path.join("plan.json"), &plan)?;
    fs::create_dir(path.join("runs")).map_err(|e| e.to_string())?;
    let mut rows =
        BufWriter::new(fs::File::create_new(path.join("rows.jsonl")).map_err(|e| e.to_string())?);
    let report = execute(&plan, |run, row| {
        export_run(run, &path.join(&row.path))?;
        serde_json::to_writer(&mut rows, row).map_err(|e| e.to_string())?;
        rows.write_all(b"\n").map_err(|e| e.to_string())?;
        rows.flush().map_err(|e| e.to_string())
    })?;
    rows.flush().map_err(|e| e.to_string())?;
    rows.get_ref().sync_all().map_err(|e| e.to_string())?;
    let mut pairs =
        BufWriter::new(fs::File::create_new(path.join("pairs.jsonl")).map_err(|e| e.to_string())?);
    for pair in &report.analysis.pairs {
        serde_json::to_writer(&mut pairs, pair).map_err(|e| e.to_string())?;
        pairs.write_all(b"\n").map_err(|e| e.to_string())?;
    }
    pairs.flush().map_err(|e| e.to_string())?;
    pairs.get_ref().sync_all().map_err(|e| e.to_string())?;
    write_json(&path.join("summary.json"), &report.analysis)?;
    let mut description =
        fs::File::create_new(path.join("report.md")).map_err(|e| e.to_string())?;
    write!(description,"# {}\n\n{}\n\nOutcome: {}. {} requested native runs. Baseline: {}.\n\nImport `manifest.json` in the experiment viewer. `rows.jsonl` preserves every run; `pairs.jsonl` pairs by seed; `summary.json` includes exact statistics, eligible counts and all exclusions. Monetary values are reduced fractions; variance has squared units. Twenty seeds do not establish statistical sufficiency.\n\nEach `runs/NNNN` directory contains a native export and committed checkpoint. ModelError prefixes are inspectable but excluded from final distributions. Rejection attempts are separate from included failure fractions. No trial fee is charged again.\n",report.name,report.hypothesis,report.outcome,report.rows.len(),report.baseline_variant).map_err(|e|e.to_string())?;
    description.sync_all().map_err(|e| e.to_string())?;
    write_json(&path.join("manifest.complete.tmp"), &report)?;
    fs::rename(
        path.join("manifest.complete.tmp"),
        path.join("manifest.json"),
    )
    .map_err(|e| e.to_string())?;
    Ok(report)
}
