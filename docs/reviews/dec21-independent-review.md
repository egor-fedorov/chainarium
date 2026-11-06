# Independent DEC-21 / M4a review

Reviewed snapshot: `215ffcfa8ac24a71341528e6e727b7adfbbe4175` (`215ffcf`). Baseline: `d66959cadf904bd2e1ce8ea1cb0b91643b6202b8`. Review date: 2026-11-06.

**Result: no confirmed actionable findings.** No economic-contract change is proposed or authorized by this review. This conclusion is scoped to the reviewed implementation and experiments below; it is not a proof that all inputs are correct or that the full M4 roadmap is complete.

The review ran in a separate agent context. Source was extracted with `git archive 215ffcf` to `/tmp/dec21-independent-snapshot`. Production source, repository state, commits, and external services were not changed. The only added Rust source was the independent integration test in that temporary snapshot. The reviewer did not write to the main checkout; concurrent parent-agent work there is outside this snapshot review. The reviewer did not use `docs/progress.md`, the implementation agent's explanations, or claimed validations as evidence. Requirements came from `docs/spec.md`, especially Accepted DEC-21 §23.2.9 and §§5, 6, 11, 12, 13, 17, 23.4. The existing numerical reference was regenerated and its results checked against actual execution.

## Evidence and coverage

All source locations below refer to snapshot `215ffcf`, under `crates/chainarium-core/src` unless stated otherwise.

| Area | Independent inspection / observed result |
| --- | --- |
| Event generation and cutoff | `scheduler/runtime.rs:543` orders emissions by time/producer/sequence and creates their event IDs before periodic submissions. `:132` visits builder first, then sorted agent IDs. `:579` dispatches by clamped time/event ID and includes cutoff equality. Independent reversed-input/equal-time tests confirm FIFO and fee ties follow producer/event order rather than lexical TxId. A separate deferred-event case confirms event ID wins once two different original due times clamp to the same parent time. |
| Delayed/clamped response | Independent test defers emission, builder, observer, and reaction across separate rounds; the reaction uses actual time 30 and deadline 60. A checkpoint taken with the pending reaction reproduces the same response after visible-message retention has expired. The earlier authorized delivery remains the trigger; the policy receives parent state. |
| Visibility and authorization | `scheduler.rs:360` checks the agent's account authority and exposes only its delivered messages and own outbox. Public/Private dispatch is at `runtime.rs:132`; trigger creation is at `:240`. Independent private test confirms Public subscription does not grant Private delivery, while the sender can inspect its own in-flight submission. A reactive `TransferFrom` without allowance reverts, charges its own sender, and cannot spend the victim's funds. |
| Admission / inclusion | `runtime.rs:152` checks envelopes and parent nonce without canonical reservation. `:354` prunes deadline/stale/TTL entries; `:404` repeatedly chooses currently eligible candidates. The independent accounting oracle checks each round's receipt order, success/revert, fee, balances, nonces, used resource, and pending set against plain integer accounting. |
| Builders, funding, fees | The oracle exercises 360 generated scenarios across FIFO, FeePriority, and Fixed, with 24 emissions and 6 rounds each: 8,640 emissions / 2,160 rounds. It varies sender, nonce, amount, fee price, declared limit, send/delivery times, deadlines, balances, retention, pending cap, inclusion-attempt cap, and eligibility-check cap. It includes funding changes and same-nonce conflicts. All cases passed. FeePriority uses price per unit; declared limit controls fit/reserve, actual use controls charged resource and fees. |
| Exhaustion and fee recipient | An additional 42 cases sweep declared limits 100–120 at price 2, with both an independent fee recipient and fee paid to the sender. Exhaustion consumes the specified resource and nonce, rolls back the transfer, and makes the competing nonce stale. All balances and fees match the separate arithmetic expectations. |
| Dedup / terminal state / limits | `runtime.rs:76` deduplicates before delivery, preserving first submission; known entries are retained for terminal memory. Independent test confirms a MempoolLimit-rejected ID remains terminal when repeated after capacity becomes available. Existing scheduler tests were actually executed for duplicate/conflicting payloads, known/queue/pending/visible caps, exact TTL age, empty Fixed, depth/reaction/intent caps, and EventId overflow. Validation explicitly rejects zero and above-cap limits. |
| Periodic policies | `policy.rs:214` suppresses a policy with its current nonce already unfinished; `runtime.rs:559` uses the prior committed scheduler state. Existing periodic-policy and DEC-20 tests passed, including the scheduled LP behavior. Policies remain invoked once per round in sorted AgentId order; skipped LP schedule events are not replayed later by the scheduler. |
| Atomicity / single accrual | `runtime.rs:532` works on a clone; `run.rs:100` installs the result only after success. `runtime.rs:405` enters `engine::begin_block` once, and `engine.rs:950` applies the shared system transitions. Existing injected post-execution/invariant-failure tests and the scheduler-versus-legacy accrual comparison passed. A new overflow test fails after earlier queue processing and verifies the entire Run is unchanged by structural equality. Browser/API testing independently verifies an actual ModelError leaves the checkpoint unchanged. |
| Model selection / inherited execution | `scheduler.rs:112` rejects mixed M1/M4 configuration and requires the three scheduler/mempool/builder tags; reactive rules require their own version tag. Independent tests remove each required tag and reject every new tag on a legacy profile. The shared executor retains its reserve/execute/revert/refund semantics; the full workspace suite, including inherited executor, authorization, credit, flash, route, book, CLMM, LP/MM, and replay tests, passed on the snapshot. |
| Checkpoint / replay | `run.rs:142` reconstructs execution and compares scheduler state as well as ledger, blocks, initial state, and RNG. Actual tests cover pending/in-flight events, terminal IDs, trigger memory, read-only observations, tampering with a freshly recomputed checksum, fork, restore, and continuation. Browser viewing of older blocks leaves both current Run and checkpoint unchanged. |
| UI / CLI diagnostics | `web/scheduler.js:6`, `web/app.js:52`, and `run.rs:213` display/export real trace, queue, status, receipts, and fees. Browser/API checks for all six scheduler demos match separately exported CLI blocks and trace counts. Private visibility is clearly labeled as a researcher view. Delayed arrival 11 is shown as in-flight after cutoff 10. The mobile layout check passes at width 390; browser page-error count is zero. Invalid model configuration cannot replace the active API run. |

## Actual commands and retained artifacts

Baseline suite, before adding independent tests:

```sh
cd /tmp/dec21-independent-snapshot
scripts/cargo.sh test --locked --offline --workspace
```

Result: all workspace tests passed, including the 18 scheduler integration tests and the two scheduler unit tests. The pinned toolchain is Rust 1.99.1 using `/workspace/.chainarium-tools`; tests used the offline dependency cache.

Independent tests:

```sh
scripts/cargo.sh test --locked --offline -p chainarium-core --test independent_dec21 -- --nocapture
```

Result: **10 passed, 0 failed**. Source: `/tmp/dec21-independent-snapshot/crates/chainarium-core/tests/independent_dec21.rs`; convenient retention copy: `/tmp/dec21-independent-tests.rs`; captured output: `/tmp/dec21-independent-tests.log`. The transfer oracle uses its own small integer ledger and priority calculation and does not call scheduler/executor helpers to derive expected answers.

Six CLI exports are retained under `/tmp/dec21-cli-exports/{public,private,delayed,nonce,fifo,fee}`. They were produced by the snapshot binary with `run scenarios/demos/sc-scheduler-NAME.json OUTPUT_DIR`. Public and delayed finish with Alice 10016 Q; private with 10000 Q; nonce with Alice 740 Q / builder 230 Q; FIFO with Alice 875 Q / builder 115 Q; FeePriority with Alice 760 Q / builder 230 Q. Public receipts occur in block 1; delayed response occurs in block 2.

Independent UI/API test: `/tmp/dec21-ui-review.py`; output: `/tmp/dec21-ui-review.log`; screenshot: `/tmp/dec21-review-delayed.png`. Reproduce by starting `target/debug/chainarium serve 8081` from the temporary snapshot, then running `python /tmp/dec21-ui-review.py`. The script also compares the UI/API run with the retained CLI exports. Local listener/browser execution required the environment's loopback sandbox escalation, which was approved; no external network service was used.

The reference was independently executed as `python docs/reviews/scheduler-reference.py` and parsed against `docs/reviews/scheduler-reference.json`; they agree exactly. The reference's economic values agree with actual CLI/core output.

## Limits of the conclusion

The generated differential test is limited to native Q transfers and bounded small inputs. It is deliberately independent of the complex protocol implementations; it does not replace protocol-specific mathematical proofs or exhaustive combined-protocol exploration. Complex protocols were covered by source review, the actually executed inherited suite, the shared-executor/accrual comparison, and the route-based scheduler demos.

The review exercises reduced numerical caps and their boundaries, rather than proving acceptable memory/time behavior at every simultaneous maximum (65,536 known IDs, 10,000 rounds, large payloads). It does not establish cross-build/platform replay portability. Current reactive policies are deterministic literal programs and do not advance agent RNG or implement search; passing rollback/replay checks is not evidence for future stateful learning/search policies. Greedy builders, bundles, topology propagation, hooks, and the remaining full M4/v1.0 requirements are outside DEC-21 and outside this result.

The ten independent Rust tests are now retained in the main repository as
`crates/chainarium-core/tests/independent_dec21.rs`. Source line references above
remain those of the reviewed snapshot `215ffcf`.
