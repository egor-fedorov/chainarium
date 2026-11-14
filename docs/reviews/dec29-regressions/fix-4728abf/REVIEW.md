# Independent fix verification — 4728abfabc6d15240bf4ceefd5c396fe7a7cc51d

R1 is closed at this snapshot. All three unchanged original inconsistent reports
are rejected with HTTP400. Five valid original reports, including real ModelError
and ArithmeticLimit series, import read-only. Corrupting the ModelError prefix's
Success or rejection selector also rejects with HTTP400. New builder and mixed
ModelError exports reproduce the original analysis, and every selected native
checkpoint matches and replays.

The main R2 correction passes: the original 33 MiB hypothesis Plan is rejected
before output-directory creation. A separate reviewer-generated case with a 2 MiB
MetricId, 12 cases and 24 runs passes metadata validation, but later exceeds the report
budget; all 24 rows and native checkpoints remain present, the manifest is Incomplete,
and no summary or complete manifest is published. The snapshot's 32 focused tests pass.

One R2 residual remains at 4728abf: `web/app.js:19` always pretty-prints downloads.
The CLI accepts and exports a 67,106,816-byte compact Completed report, which the Rust
importer accepts with HTTP 200. The exact `JSON.stringify(value,null,2)` download is
67,138,488 bytes, above 67,108,864; its import returns HTTP 413. This loses the UI's
download/reimport path for a valid report near the cap.

Evidence: `results.json`, `late-growth-plan.json`, `late-growth-incomplete/`,
`near-limit/manifest.json`, `ui-downloaded-report.json`, `download-size-result.json`,
`download-api-result.json`. Reproducer source is preserved separately at
`/tmp/chainarium-dec29-evidence/download_size.py`.

The residual was reported immediately and is addressed separately by 1d4e3a2.
No main-repository files were changed by this reviewer.
