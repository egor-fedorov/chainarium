# Independent DEC-29/M5b review — snapshot 2bc4465

Scope: read-only review of `/workspace/chainarium`, implementation diff
`1a4e5af..2bc4465`, Accepted DEC-29 (§23.2.17), applicable §14/15 and §23.4.
All execution used a `git archive 2bc4465` snapshot at
`/tmp/chainarium-dec29-independent`. No main-repository edits were made.

## Findings

### R1 — P2: report import accepts internally contradictory row metadata/counts

Locations: `crates/chainarium-core/src/experiment/mod.rs:416-435` and
`:481-496` in 2bc4465.

`Report::validate` checks row horizons against caps and one another, but not
against the corresponding saved Scenario. It validates the included-failure
fraction but never compares `IncludedCount` or `InclusionRejectionAttempts`
selectors with the already-present outcomes.

Reproduction from the ordinary builder export:

* Set every row's `horizon` and `committed_blocks` to `"0"`. Keep the canonical
  Plan's three-block scenarios, Completed statuses, metrics and summary.
* Alternatively change row0 `outcomes.included.Success` from `"1"` to `"999"`
  while the `IncludedCount{Success}` metric remains one.
* Alternatively change row0 rejection attempts to 999 while its selector remains one.

All three POSTs to `/api/experiment-report` return HTTP200. The UI shows Completed
rows with `0 / 0` blocks, nonzero financial results and the retained three-block
Plan. This is a detectable internal contradiction, distinct from the intentionally
deferred checkpoint verification of the truth of otherwise coherent observations.
The current singleton is unchanged by those imports.

Evidence: `import_invariants.py`, `import-invariants-result.json`, immutable
`accepted-inconsistent-{horizon,included,rejections}.json`,
`accepted-zero-horizon.png`. Root acknowledged and supplied fix a05ec23 for separate review.

### R2 — P2: valid Plan exports Completed report that its own importer rejects

Locations: `crates/chainarium-core/src/experiment/export.rs:56-61`,
`experiment/mod.rs:339-341`, `web/experiments.js:42-44` in 2bc4465.

A legal one-case/two-variant/six-block Plan with a 33MiB hypothesis occupies
34,609,582 bytes, below the 64MiB Plan cap. Export succeeds as Completed and tells
the user to import manifest.json. The report repeats the hypothesis in its top
level and embedded Plan and occupies 69,232,220 bytes. Report::parse and the UI
both cap input at 67,108,864 bytes. The UI rejects this actual CLI output with
`Отчёт больше 64 MiB`.

Thus input validation and successful export do not guarantee an importable report;
the failure is determined by metadata before any execution. README mentions the
browser limit, but the accepted bounded CLI-to-viewer flow still produces this
non-roundtrippable Completed artifact. Root accepted a common report budget and
pre-execution metadata-size check as the correction.

Evidence: `report_size.py`, `report-size-result.json`, `large-legal-plan.json`,
`large-legal-series/manifest.json`, `large-report-rejected.png`.

## Independent checks that passed

* Offline build of exact snapshot, 28 focused experiment tests.
* All 48 demo native exports equal standalone execution and checkpoint resume,
  every exported file byte-for-byte; scalar reference cases and own Fraction
  statistics checked.
* 64 new fractional-price cases/128 native runs: independently calculated end
  portfolio values, fees, paired effects, min/max, mean, median, nearest-rank
  p05/p95, sample variance and negative/zero/positive counts.
* Native asymmetric missing cohorts from zero versus nonzero final X inventory;
  seed-based pairing, both side-specific exclusions, zero no-op and no-receipt case.
* Separate per-step 512-bit Fraction oracle using 64 prime denominators: native
  observations retained, affected statistics ArithmeticLimit, unaffected exact
  order statistics retained.
* Actual 2 Success + 1 ProtocolRevert + 1 ResourceExhausted native receipts:
  failure fraction 1/2, fees455 and PnL−457; selector counts match native records.
* Four seeds with target draw cadence12 versus24: foreign agent24 draws and
  complete scalar selections remain identical.
* Actual variant-specific reactive timestamp overflow after one committed block:
  PnL−125 prefix remains inspectable, all its final metrics excluded, next variant
  completes, both selected native checkpoints replay correctly.
* Real EFBIG using a child-only RLIMIT_FSIZE of23435 bytes: first native export
  and row preserved, second export stops, manifest remains Incomplete, no summary.
* Independent Plan/Scenario/common/external SHA256 and structural diff reproduction
  on four series, including every new fractional and missing-cohort case.
* 27 independent invalid-plan mutations rejected before any output directory:
  all limit zero/upper cases, lowered aggregate caps, wrong metrics, missing target,
  hidden environment/account/model-order change, seed-dependent treatment.
* Chromium on isolated port8765: exact fractions and counts, exclusions, ArithmeticLimit,
  narrow390px layout, read-only imports, selected-checkpoint mismatch rejection,
  matching native replay, and report download. Both findings observed in UI.

Scripts: `independent_checks.py`, `runtime_checks.py`, `validation_checks.py`,
`ui_review.py`; corresponding `*-result.json` files and screenshots record results.
The original inconsistent report artifacts and finding reproducers remain intact.

Limitations: this is bounded deterministic and adversarial coverage, not exhaustive
proof or performance benchmarking. Power-loss durability and every native policy
combination were not independently exhaustively exercised. The main singleton on
port8080 was never used. Fix commits are reviewed separately from this original report.
