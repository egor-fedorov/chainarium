# Independent DEC-29/M5b review — final verdict

Final implementation snapshot reviewed:
`1d4e3a2aaadc0a754de11bc66f5b583178e5524d`.

**Verdict: R1 and R2 are closed. No other confirmed open findings in the reviewed
DEC-29/M5b scope.** This verdict does not constitute acceptance of unrelated future
M5 work or Proposed DEC-30.

## Findings and fix history

| Finding | Original reproduction | Correction | Independent result |
| --- | --- | --- | --- |
| R1 / P2: imported row horizons/count selectors contradict the saved Scenario/outcomes | `2bc4465`: three original corrupted reports accepted with HTTP 200 | `a05ec23`, verified as part of `4728abf` | Original immutable reproducers reject with HTTP 400. Correct reports still import read-only. Completed and ModelError prefix count selectors are checked. |
| R2 / P2: Completed export exceeds its own importer capacity | `2bc4465`: legal 34,609,582-byte Plan exports 69,232,220-byte report that UI rejects | `4728abf` | Original oversized metadata rejects before any execution/output directory. Separate 2 MiB MetricId / 24-run case exceeds the final report budget and preserves all 24 rows/checkpoints under Incomplete. |
| R2 residual: pretty download crosses the cap | `4728abf`: compact 67,106,816 bytes imports with HTTP 200; UI formatter produces 67,138,488 bytes, rejected with HTTP 413 | `1d4e3a2` | Actual Chromium import → download → reimport passes for the 67,106,816-byte report. Download is byte-identical, SHA256 `a32ce47e409ccf4601deb9440dced2313b073a6923a00a978546fc978499f9d6`; both imports return HTTP 200 and singleton remains unchanged. Normal 55,135-byte report also passes. |

Original finding locations, reproduction details and broad independent numerical
coverage remain in `REVIEW-2bc4465.md`. The intermediate fix history is recorded in
`/tmp/chainarium-dec29-fix-evidence-4728abf/REVIEW.md`.

## Verification performed

The original snapshot was archived to `/tmp/chainarium-dec29-independent` and
built offline. Its 28 experiment tests passed. Independent checks additionally
covered all 48 demo runs against standalone execution and checkpoint resume, every
exported file byte-for-byte; 64 new fractional-price cases and every exact statistic;
real asymmetric Unpriced cohorts and zero outcomes; a per-step 512-bit Fraction
oracle with prime denominators; native paid-revert/resource-exhaustion fees and
included failure fractions; foreign RNG streams under different target draw counts;
variant-specific ModelError followed by a Completed run; real EFBIG after a streamed
export; independent input hashes/diffs; 27 invalid Plan mutations; and real Chromium
exact values, exclusions, narrow layout, read-only imports and checkpoint matching.

The combined Rust corrections were archived and built independently at
`/tmp/chainarium-dec29-fix-4728abf`; all 32 focused tests passed. Preserved original
negative controls and five valid reports were submitted to its isolated HTTP server.
Fresh builder and mixed-ModelError exports reproduced the original analysis and
every selected checkpoint replayed successfully. New large-input and late-result
growth cases checked both pre-execution rejection and Incomplete preservation.

The final frontend correction was archived at `/tmp/chainarium-dec29-fix-1d4e3a2`.
The final Chromium test supplied the exact archived `web/app.js` and
`web/experiments.js` via Playwright request interception to the isolated 4728abf
HTTP server. The test first compared **every file under `crates/` byte-for-byte**
between both snapshots and confirmed they are identical. Thus the final JS handlers
ran against the unchanged, independently tested Rust importer. No claim is made
that a second full native build was performed solely for this JS-only correction.
For the 32 MiB hypothesis label, the test hid page layout with CSS while retaining
the actual DOM updates, handlers, file upload, HTTP validation, download and reimport;
this avoids turning a report-size regression into an unrelated layout benchmark.

## Evidence

Original immutable finding reproducers and broad checks:

* `/tmp/chainarium-dec29-evidence/import_invariants.py`
* `/tmp/chainarium-dec29-evidence/report_size.py`
* `/tmp/chainarium-dec29-evidence/download_size.py`
* `/tmp/chainarium-dec29-evidence/independent_checks.py`
* `/tmp/chainarium-dec29-evidence/runtime_checks.py`
* `/tmp/chainarium-dec29-evidence/validation_checks.py`
* `/tmp/chainarium-dec29-evidence/ui_review.py`

Separate fix verification:

* `/tmp/chainarium-dec29-evidence/verify_fixes.py`
* `/tmp/chainarium-dec29-evidence/verify_download_fix.py`
* `/tmp/chainarium-dec29-fix-evidence-4728abf/results.json`
* `/tmp/chainarium-dec29-fix-evidence-4728abf/download-api-result.json`
* `/tmp/chainarium-dec29-fix-evidence-1d4e3a2/download-fix-browser-result.json`

Large plans, complete original/fixed exports, native checkpoints, corrupted report
files, downloaded files, JSON result logs and screenshots remain alongside those
paths. They are sufficient to reproduce the financial checks and all findings.

The reviewer made no changes to the main repository, commits, pushes or environment
publication, and spawned no additional agents. Only isolated temporary servers on
port 8765 were used; the main singleton on 8080 was never accessed. The temporary
review server was stopped after verification.

Limitations: bounded deterministic/adversarial coverage, not exhaustive proof or
performance benchmarking. Power-loss durability and all native policy combinations
were not exhaustively exercised. Unrelated Proposed DEC-30 changes were excluded.
