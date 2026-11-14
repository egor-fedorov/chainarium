# Независимый read-only review DEC-30 / M5c HODL

Результат: подтверждённых дефектов P0/P1/P2/P3 не обнаружено. Исправления production code по результатам этой проверки не требуются. Это вывод в пределах указанного ниже покрытия, не доказательство отсутствия любых ошибок и не приёмка всего M5.

## Scope и независимость

- Репозиторий `/workspace/chainarium`, ветка `work`.
- Проверенный production diff: `490881b..315a2a0`, конечный commit `315a2a06408016156d053930800278f930505583`.
- Во время проверки root создал только документационный commit `03af65b3484df7aad8d5901e4d676646c15c2fe0`; `git diff --name-only 315a2a0..HEAD` содержит только `docs/spec.md`, `docs/progress.md`. Production sources, web, scenarios и build fingerprint не изменились.
- Release binary: `/workspace/chainarium/target/release/chainarium`; build ID `8479e213264f9f83a982cb15d1c23efab7f13cb89b3333a6182e7997ede24b61`; SHA-256 бинарника `a74a9fd0ca55a4278dbf692589f01acb57fd2d56106473c1b876480009430641`.
- Rust/Cargo 1.99.1. Собственный Rust harness связан с уже существующим debug core rlib `target/debug/deps/libchainarium_core-8cceeb5ec912b174.rlib`; debug build ID `d3d29ad925077cf0a571672cb52fc6adb621a4920c7d2a0169f0a74a9d57cdbb`.
- Применимых `AGENTS.md` в `/`, `/workspace`, `/workspace/chainarium` и поддереве workspace не найдено. Прочитаны `docs/spec.md` §23.2.16 DEC-28, §23.2.17 DEC-29, §23.2.18 DEC-30, §23.4 и preimplementation Fraction fixtures.
- Diff исследован самостоятельно. Объяснения автора о правильности реализации не использовались. Другие агенты не запускались. Файлы репозитория не редактировались; commits/push/publication не выполнялись. На выходе рабочее дерево чистое.
- Копии production source с модификациями не создавались. White-box проверки меняют только объекты `Run.initial` в памяти собственного `/tmp` harness; затем проверяют `World::invariant` и вызывают настоящий `hodl::at` из неизменённого debug core.

## Findings

Подтверждённых findings нет. Не обнаружены нарушения canonical order, feasibility, permanent failure, valuation/PnL, caps, noninterference, checkpoint, export/provenance или readonly/exact UI в выполненных проверках. Не выдаются severity или исправления для гипотетических проблем без воспроизводимого нарушения.

## Проверки и результаты

1. `./scripts/cargo.sh test --workspace --locked --offline` — PASS, весь workspace suite. Отдельный `-- --list` подтвердил 479 tests. Включены native execution/atomicity, scheduler/RNG, checkpoint/replay, valuation, HODL, paired experiments; HODL имеет 5 module tests и 13 integration tests. Исходники между выполнением и listing не менялись.
2. Preimplementation fixtures — PASS: Python `cases()` и проверки DEC-30 совпадают с сохранённым JSON (18 cases), DEC-28 — с JSON (24 cases). Модули импортированы без `__main__`, без перезаписи fixtures и без `__pycache__`.
3. Собственный Python Fraction sweep — PASS: 96 заново сформированных сценариев, 672 boundaries, три portfolio views (`alice`, `bob`, combined), 1344 фактических labelled capital records. Native receipts: 899 Success, 1405 ProtocolRevert. Независимо проверены holdings из genesis wallet quantities, canonical receipt/event order при обратном lexical TxId, before/after, per-portfolio flow count, capital marks, actual/HODL marks/PnL, excess, value_gap, wire reduction/floor, permanent failure и last_feasible. Unlabelled transfers и native fees остаются в actual performance.
4. Sweep включал положительные и нулевые holdings, неизвестные/просроченные/будущие/ещё недоступные цены, неоценённую историю, несколько портфелей и overlaps, lowered assets/flow caps. Наблюдавшиеся причины HODL mark: Stale 405, FutureObservation 270, NoAvailableRecord 131, MissingPrice 254, AssetLimit 446, InfeasibleOutflow 40, FlowLimit 96 (counts по portfolio-boundary, возможны несколько причин одновременно). Все результаты совпали с независимым Fraction oracle.
5. Отдельные native boundary cases — PASS: 0/1/64 активов дают Ready; 65 ненулевых активов при cap64 дают AssetLimit без частичного initial inventory. Полный вывод X освобождает slot для следующего Y в том же блоке, но не обнуляет cumulative flow budget. При flow cap1 второй flow даёт FlowLimit с пустой last_feasible корзиной и cursor первого вывода.
6. Собственные labelled Transfer refusal/failure cases — PASS: wrong nonce даёт rejection, flows0, excess0; Transfer900 Q из1000 Q при limit200/price1 даёт ProtocolRevert, flows0, actual885, HODL1000, excess−115; limit100 даёт ResourceExhausted, flows0, actual900, excess−100. Никакой второй HODL fee.
7. Native arithmetic recovery — PASS: три holdings по1, цена1 в genesis, цены с попарно взаимно простыми знаменателями `(2^193−1)`, `(2^197−1)`, `(2^199−1)` в t1, снова1 в t2. Точный mark в t1 имеет 589-bit denominator: actual остаётся известен при DEC-28 cap32768, HODL mark получает ArithmeticLimit при DEC-30 cap512, ledger остаётся Ready. В t2 HODL value3/excess0 восстанавливаются.
8. Собственный Rust white-box harness — PASS: три invariant-valid LP claims по1/3 X агрегируются ровно в1 X (и9 Q), initial/HODL value12 при цене3. Missing price не уничтожает holdings. ComponentLimit не выдаёт partial holdings. Gross initial debt1 Q при положительном wallet2 Q даёт InitialLiabilityUnsupported несмотря на положительную net Q; независимый depositor остаётся Ready. Initial quantity grouping с широкими взаимно простыми знаменателями >512 bits даёт ArithmeticLimit и отсутствующий initial inventory.
9. Native noninterference — PASS: `sc-agent-random`, `sc-hodl-infeasible`, `sc-hodl-credit`, `sc-hodl-ordered`, `sc-hodl-lp` выполнены с HODL, без HODL и с изменёнными HODL caps/reference feeds. `initial`, `state`, полные `blocks` (receipts/events/decisions), `scheduler`, `rng` совпали. У выключенного HODL отсутствует `hodl.jsonl`.
10. Checkpoint — PASS: для каждого prefix этих пяти сценариев checkpoint/resume до конца даёт byte-identical все native export files, включая HODL. Никакой повторный interest/fees/RNG не возник.
11. DEC-29 — PASS: собственная paired builder case с общим HODL даёт native HODL export в обеих строках; native checkpoint replay воспроизводит `hodl.jsonl`. Изменение только `hodl_policy.limits.flow_events_per_portfolio` одной variant отвергается как `undeclared experiment difference` до создания output directory. Existing tests дополнительно проверяют отсутствие новых HODL selectors/primary metric.
12. Provenance — PASS: build/run IDs, SHA-256 `hodl-policy.json`, `valuation-policy.json` и independently serialized reference inputs проверены для всех672 sweep boundaries. Canonical fractions проверены рекурсивно, включая floor к −∞.
13. API/UI — PASS: headless `/usr/bin/chromium` на текущем сервере `127.0.0.1:8080`, все семь HODL presets, все boundaries и portfolio options. Дополнительный сценарий содержит wallet Q=`9007199254740993`, fractional loss `−1/3` и HTML-like identifiers. Отображение совпало с Rust wire, download полностью совпал с policy/valuation policy/boundary, JS errors отсутствуют, identifiers экранируются. Чтение истории/download не изменили checkpoint bytes. На viewport390 нет горизонтального overflow; сохранены screenshots fractional/infeasible/unpriced. Fractional и infeasible screenshots также визуально просмотрены. Серверный singleton восстановлен из checkpoint, сохранённого до UI проверки.
14. Static review — обработаны HODL config/Shadow/mark/difference/ordered_flows, shared DEC-28 extraction/price/pnl/capital validation, Scenario validation, streaming Run export, API handler, UI exact formatting/race guard и DEC-29 common-input masking. Existing full suite покрывает rolled-back bundle probes, delayed delivery, ModelError rollback и claims других протоколов.

## Команды воспроизведения

Рабочий каталог `/workspace/chainarium`. Для повторных запусков указывайте новые output directories: native export/checkpoint/experiment намеренно не перезаписывают существующие артефакты.

```bash
./scripts/cargo.sh test --workspace --locked --offline
python /tmp/dec30-review-fixtures.py
DEC30_REVIEW_ROOT=/tmp/dec30-review-sweep-rerun python /tmp/dec30-review-sweep.py
DEC30_REVIEW_ROOT=/tmp/dec30-review-native-rerun python /tmp/dec30-review-native.py
DEC30_REVIEW_ROOT=/tmp/dec30-review-boundaries-rerun DEC30_SWEEP_ROOT=/tmp/dec30-review-sweep-rerun python /tmp/dec30-review-boundaries.py
DEC30_REVIEW_ROOT=/tmp/dec30-review-ui-rerun python /tmp/dec30-review-ui.py

/workspace/.chainarium-tools/rustup/toolchains/1.99.1-x86_64-unknown-linux-gnu/bin/rustc --edition=2024 /tmp/dec30-review-claims.rs -L dependency=target/debug/deps --extern chainarium_core=target/debug/deps/libchainarium_core-8cceeb5ec912b174.rlib --extern serde_json=target/debug/deps/libserde_json-b2cbb6aa9ac0dba9.rlib -o /tmp/dec30-review-claims
/tmp/dec30-review-claims
```

UI требует доступного сервера текущей release-сборки на8080 и Playwright. В данной среде сервер/Chromium находятся вне обычного sandbox loopback; команда UI выполнялась с `sandbox_permissions=require_escalated`. Это локальная проверка, без публикации среды. Применён навык `cloud-desktop` (`skill://plugin_connector_1p_c5b7d5df5d7081918f2c4be5a633ed5d/cloud-desktop/SKILL.md`) для выбора повторяемой headless проверки.

## Сохранённые артефакты

- Финальный отчёт: `/tmp/dec30-review-report.md`.
- Скрипты/исходник: `/tmp/dec30-review-sweep.py`, `/tmp/dec30-review-native.py`, `/tmp/dec30-review-boundaries.py`, `/tmp/dec30-review-ui.py`, `/tmp/dec30-review-fixtures.py`, `/tmp/dec30-review-claims.rs`.
- Logs: `/tmp/dec30-review-sweep.log`, `/tmp/dec30-review-native.log`, `/tmp/dec30-review-boundaries.log`, `/tmp/dec30-review-ui.log`, `/tmp/dec30-review-fixtures.log`, `/tmp/dec30-review-claims.log`, `/tmp/dec30-review-test-list.log`.
- Metadata: `/tmp/dec30-review-coverage.json`, `/tmp/dec30-review-script-sha256.txt`.
- Все96 sweep inputs, native exports и counters: `/tmp/dec30-review-sweep/`.
- Arithmetic recovery, on/off/changed scenarios, prefix checkpoints/resumed exports, paired plan/rejection input и exports: `/tmp/dec30-review-native/`.
- Финальные assets/caps/rejected/revert/exhausted scenarios и exports: `/tmp/dec30-review-boundaries-final/`.
- Screenshots: `/tmp/dec30-review-ui/fractional.png`, `/tmp/dec30-review-ui/infeasible.png`, `/tmp/dec30-review-ui/unpriced.png`.
- Исполняемый white-box harness `/tmp/dec30-review-claims` воспроизводится из сохранённого `.rs`; архивировать бинарник необязательно. Ранний каталог `/tmp/dec30-review-boundaries/` — подмножество проверок до добавления transfer refusal cases; для архива достаточно `boundaries-final`.

## Ограничения покрытия

- Native genesis сейчас задаёт sole ownership LP и нулевой opening debt. Fractional/gross-liability initial cases проверены на invariant-valid synthetic World snapshots настоящим extractor/HODL, но не объявляются native-reachable genesis/checkpoint inputs.
- Independently randomized sweep использует wallet portfolios. CLMM, StableLP, book escrow, deposits/collateral, bundle rollback и delayed scheduling дополнительно проверены source review и существующими integration/native invariant tests; новый случайный differential generator для всех этих протоколов не создавался.
- Проверены нижние арифметические caps512 и границы assets64/65, но не worst-case memory/time при одновременных hard caps32768 bits,4096 flows,10000 blocks,16 portfolios. Performance gates и полный M5 остаются вне этого review.
- UI проверен в headless Chromium на desktop/mobile viewport, не во всех браузерах и не в полноценном accessibility audit.
