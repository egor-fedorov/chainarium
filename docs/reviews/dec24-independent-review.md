# Независимый review DEC-24 / M4d

Дата: 2026-11-06. Проверка выполнена отдельным read-only агентом по `docs/spec.md` §23.4. Объяснения разработчика не использовались как доказательство корректности.

## Ревизия и scope

- Exact reviewed revision: `976a5cb78997990aabdf227ddc6c0e5df354537f`.
- База diff: `72590bbaca8c15122a72ec0dad24df2f07c43f2a`.
- Scope: `72590bb..976a5cb`, 27 файлов, новые модели `liquidate-and-sell-v1` и `bounded-liquidator-v1`, интеграция scheduler/executor, receipts/exports/UI и новые fixtures/tests.
- Требования: accepted DEC-24 (§23.2.12), §23.4, native execution/authority (§5–6), lending/rounding/oracle DEC-14/15 (§8/10, §23.2.3), strict flash (§9), routes DEC-20 (§23.2.8), scheduler/builders DEC-21/22 (§23.2.9–10), связанные ограничения DEC-23.
- Численные эталоны: `docs/reviews/liquidator-reference.py` и `.json`, а также соответствующие demo scenarios и существующие protocol tests.
- Изменения после snapshot, включая docs-only `4046145` и Proposed DEC-25/hooks, вне scope. Проверка diff `976a5cb..HEAD` при завершении показала только изменения docs.

Рабочий репозиторий `/workspace/chainarium` не изменялся этим агентом. Использован отдельный `git archive 976a5cb` в `/tmp/dec24-review-976a5cb` и отдельный `/tmp/dec24-review-976a5cb-target`. Сервер пользователя на loopback:8080 не изменялся. Другие агенты не запускались.

## Результат

Воспроизводимых дефектов в проверенном scope не найдено. Findings уровней Critical/High/Medium/Low отсутствуют; блокирующих замечаний по DEC-24 нет.

Это результат ограниченного независимого review, а не доказательство отсутствия любых дефектов. Новые независимые regression tests следует перенести в обычную suite: они добавляют экономически значимые границы, не зафиксированные одним эталонным примером.

## Проверенные существенные свойства

- Wrapper фиксирует debt/collateral balances непосредственно вокруг вложенного `Liquidate`; продажа получает actual seizure, а старый collateral inventory сохраняется. Actual repayment ограничивается lending close/debt/collateral caps.
- Own funding проверяется общим executor после native reserve и для actual paid. Flash principal равен request, неиспользованный principal остаётся для settlement, ceil fee взимается со всего principal. Guard исполняется после flash scope и проецирует actual native settlement относительно входного баланса транзакции.
- Для всех проверенных отказов liquidation, collateral sale, write-off и protocol transfers откатываются совместно. Canonical typed results удаляются; diagnostic attempts остаются. Включённые revert/exhaustion сохраняют actual native fee и nonce.
- FIFO/FeePriority fixtures соответствуют reference. Дополнительные Fixed/Greedy проверки подтверждают, что проигравший native candidate с NoDebt включается и платит 120 units; winner платит 170 units, капитал и builder balance совпадают с integer oracle.
- Preview строится общим begin_block на parent и переиспользуется как неизменяемая исходная точка trials. Нет доступа к текущим/future emissions в policy observation. Target view ограничен перечисленными позициями; public venues/feeds, own balances и выбранный flash provider проверены по структуре и существующим noninterference tests.
- Freshness проверяется на preview time, equality разрешена; NoDebt/HF equality/missing/stale имеют явные target statuses. Пробы не умножают начисление и не расходуют канонические fees.
- Targets/sizes/DFS сортируются детерминированно. Проверены равный net, приоритет меньшего actual used, затем market, request и path; перестановка target/sizes не меняет report и конечное состояние.
- Проверены accounting generated children в точных caps 1024 nodes и 32 depth, в том числе внутри flash callback; следующий node/depth отвергается.
- Проверены hard trial/venue caps, общий round budget, authority/model/path validation, native reserve shortage, delayed/busy/expired intent и ModelError rollback через существующие tests и чтение executor/runtime.
- Все шесть DEC-24 demos восстанавливаются на каждой committed boundary с byte-identical checkpoints. Read-only observation/credit views не меняют checkpoint, включая RNG state.

## Выполненные проверки

Toolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.

1. Полный проектный suite на неизменённом archive snapshot:

   `CARGO_TARGET_DIR=/tmp/dec24-review-976a5cb-target ./scripts/cargo.sh test --locked --offline`

   Результат: PASS, exit 0. Включает обе новые suites `liquidation_sale`/`liquidator_policy`, fault-injection tests и прежние independent DEC-20/21/22/23 suites.

2. Независимый reference script:

   `python3 docs/reviews/liquidator-reference.py > /tmp/dec24-review-976a5cb/reference-recomputed.json`

   `cmp docs/reviews/liquidator-reference.json /tmp/dec24-review-976a5cb/reference-recomputed.json`

   Результат: PASS, точное совпадение.

3. Дополнительная независимая suite, добавленная только во временный archive:

   `CARGO_TARGET_DIR=/tmp/dec24-review-976a5cb-target ./scripts/cargo.sh test --locked --offline --test independent_dec24 -- --nocapture`

   Результат: 7 passed / 0 failed. Лог: `/tmp/dec24-review-976a5cb/independent-tests.log`.

   - `integer_oracle_768_own_flash_capital_close_fee_cases`: 768 комбинаций Own/Flash, капитала, native price, close factor, dust/request boundaries и CPMM fee; ожидаемые paid/seized/output/net/claims/cash рассчитаны обычной независимой целочисленной арифметикой, без quote/math helpers симулятора.
   - `every_resource_unit_boundary_restores_sale_writeoff_and_flash`: 344 исполнения; каждый limit от 100 до 170/200, Own/Flash, native price 0/1; проверены rollback, реальные fees, nonce и отсутствие canonical typed result при exhaustion.
   - `exact_generated_node_and_depth_caps_cover_flash_children`: точные границы node/depth и отказы на следующем значении, с учётом отдельного callback и ширины Sequence.
   - `fixed_and_greedy_preserve_paid_native_losing_attempts`: конкуренция при Fixed/Greedy, exact balances/fees/NoDebt.
   - `u256_request_and_minimum_overflow_are_protocol_failures_or_capped_actual_payment`: U256::MAX request/minimum; Own clamp успешно платит 60, flash principal+fee overflow и baseline+minimum overflow остаются обычными ProtocolRevert trials, не ModelError раунда.
   - `all_six_demos_restore_every_boundary_and_read_only_inspection_preserves_rng`: checkpoint/replay и неизменность после read-only views.
   - `deterministic_dfs_and_selection_prefer_net_then_work_market_request_path`: несколько markets, sizes и 1/2-hop маршрутов с одинаковым net; приоритет actual used и последующие tie-breaks, перестановка input config.

## Артефакты для переноса

- Отчёт: `/tmp/dec24-independent-review.md`.
- Готовый отформатированный Rust test: `/tmp/dec24-review-976a5cb/crates/chainarium-core/tests/independent_dec24.rs`.
- Лог семи независимых tests: `/tmp/dec24-review-976a5cb/independent-tests.log`.
- Пересчитанный reference JSON: `/tmp/dec24-review-976a5cb/reference-recomputed.json`.

Тестовый файл использует стандартные публичные API и относительные paths через CARGO_MANIFEST_DIR; его можно перенести в `crates/chainarium-core/tests/independent_dec24.rs` без зависимости от `/tmp`.

## Ограничения

- Новая независимая численная модель охватывает liquidation + CPMM. StableSwap/CLMM/book проверены чтением интеграции и существующими analytical/rollback/partial-fill suites; их математика заново независимо не реализовывалась в этом review.
- UI/CLI diff и использование сохранённых receipts/reports проверены статически. Отдельный браузерный smoke и визуальная приёмка этим агентом не выполнялись; сервер 8080 оставлен основному агенту.
- Performance/M5/MTM/hooks и внешняя EVM-совместимость не оценивались и не считаются закрытыми этим заключением.

## Сохранение в основной suite

Автор перенёс отчёт и `tests/independent_dec24.rs` в основной репозиторий. При
интеграции две стандартные целочисленные ceil-формулы заменены эквивалентным
`u64::div_ceil` для Clippy `-D warnings`; эталон не использует math helpers ядра.
Семь тестов адресно перепроверены после этой правки. Код реализации M4d не менялся.
