# Независимый review DEC-20

Независимый review DEC-20 / M3c–M3d, снимок `1bc17ee`, diff `3827f63..1bc17ee`.

Подтверждённых нарушений Accepted DEC-20 уровня P0/P1/P2/P3 в проверенном объёме не найдено. Блокирующих замечаний к денежному учёту, атомарности и baseline policies нет. Это вывод независимого чтения спецификации/кода и запусков; записи `docs/progress.md` и объяснения реализовавшего агента не использовались как доказательство.

Проверка выполнена в `git archive 1bc17ee`, распакованном в `/tmp/chainarium-dec20-review`. Основной checkout не редактировался; коммиты и публикации не выполнялись.

Подтверждённые свойства с адресами кода на снимке:

- `crates/chainarium-core/src/engine/route_calls.rs:44`: бюджеты следующей ноги равны фактическому output; spent/unspent/output берутся из ledger delta до/после native settlement. Envelope reserve/refund находятся за пределами этих измерений. Residuals суммируются по активам; RequireFull проверяется после каждой ноги; root result сохраняется только после конечного min_out.
- `crates/chainarium-core/src/routing/heterogeneous.rs:97`: buy lots используют limit reservation с округлённой вверх fee на lot; sell lots используют lot_atoms. Price improvement и fee-to-self не добавляют новые lots. Нулевая заявка и нулевой output маршрута отклоняются.
- `crates/chainarium-core/src/engine.rs:336`, `:884`: savepoint включает World, output register и route results; rollback очищает committed results/events, сохраняя диагностические попытки и native fee/nonce. Prefix fill + последующий SelfTrade, поздний min_out/assert, ресурсные границы и flash shortfall не оставляют протокольных изменений.
- `crates/chainarium-core/src/model.rs:649`: generated native children входят в 1024-node/32-depth admission limits; допускается ровно 1–3 hops без повторения venue; ссылки на скрытые generated children не принимаются. Root SwapOutput проверяет тип актива.
- `crates/chainarium-core/src/routing/heterogeneous.rs:307`, `:365`, `:386`, `:397`: DFS лексикографический; target завершает путь; исполняются максимум 64 кандидата; результат сортируется по output, фактическому used resource и path. Ошибки admission/revert/exhaustion не превращаются в успешные quotes. Исполнение stale intent использует текущее состояние.
- `crates/chainarium-core/src/policy.rs:95`, `crates/chainarium-core/src/engine.rs:715`, `crates/chainarium-core/src/model.rs:563`: heterogeneous arbitrage использует RequireFull и последний root AssertTransactionNetGain в fee asset; baseline снимается перед envelope reservation, fee-to-self учитывается. Поступления до начала самой транзакции не подменяют прибыль цикла.
- `crates/chainarium-core/src/policy/baselines.rs:261`: LP schedule сопоставляется с next_block, collect предшествует burn, zero-L не burn-ится; Target mint защищён caps. Ошибка пробы даёт Skip без переноса события. На границе 16 позиций реальный Exit закрывает все собственные позиции и оставляет чужую позицию и баланс владельца без изменений.
- `crates/chainarium-core/src/policy/baselines.rs:346`, `:365`, `:423`: MM cadence, observation age, точный rational floor, price bounds, cancel-before-replace и paired funding проверены. Missing/stale oracle даёт cancel-only; нехватка envelope fee диагностируется. На границе 16 заявок cancel-only освобождает всё собственное обеспечение и сохраняет чужую заявку.
- `crates/chainarium-core/src/routing.rs:34`: Observation фильтрует позиции/orders по владельцу и не содержит чужих balances, future blocks/private orderflow/reference valuation. Policy controller передаёт snapshot simulation; Run принимает решения до scripted candidates на предыдущем committed state. Штатные replay/checkpoint/RNG проверки прошли на трёх реальных сценариях.

Воспроизводимые проверки:

1. В `/tmp/chainarium-dec20-review`: `./scripts/cargo.sh test --locked --offline` — 148 штатных core tests прошли, включая 11 heterogeneous и 16 policies.
2. `./scripts/cargo.sh test --locked --offline -p chainarium-core --test independent_dec20` — 6 дополнительных независимых тестов прошли. Исходник: `/tmp/chainarium-dec20-review/crates/chainarium-core/tests/independent_dec20.rs`.
3. В независимых тестах — 3384 sell→CPMM случая и 1120 buy случаев, отдельно рассчитанных целыми числами без вызова Rust pricing/matching helper: частичные fills, dust, price improvement, per-match rounding, fee 0/1/99/100/3333/9999 bps, fee-to-self, RequireFull/KeepResiduals и реальные конечные balances после execution fee.
4. Отдельный двухстаканный маршрут: budget 35 X → spent 20 X → output 198 Q → spent 87 Q → output 20 R; residuals ровно 15 X + 111 Q, старые 100000 Q кошелька не расходуются ногой 2. Отдельно prefix fill с fee-to-self перед SelfTrade полностью откатывается.
5. Дополнительные граничные policy tests: LP Exit с 16 собственными + 1 чужой позицией после fee-generating swap; MM stale cancel с 16 собственными + 1 чужой заявкой.
6. `python3 docs/reviews/heterogeneous-route-reference.py` даёт byte-for-byte исходный `heterogeneous-route-reference.json`.
7. Реальный CLI `run scenarios/demos/<name>.json <export-dir>`: sc-03-heterogeneous — 4 блока, 6 Success/3 ProtocolRevert; sc-lp-policies — 5 блоков, 6 Success, 4 Submit/6 Hold; sc-mm-oracle — 6 блоков, 7 Success, 5 Submit/5 Skip/2 Hold. Артефакты `/tmp/dec20-export-{heterogeneous,lp,mm}`.
8. CLI routes на sc-03-heterogeneous после блока 1: 4 кандидата, truncated=false; book→deep выдаёт 194 R и residual 15 X, book→exit — 165 R и residual 15 X. Quote сохранён в `/tmp/dec20-route-quote.json`.

Границы вывода: browser/UI не проходили отдельный интерактивный review; проверены исходники wire/UI и CLI, но это не полноценная UI-приёмка. Не выполнен заново исчерпывающий аудит всей унаследованной CLMM/StableSwap/lending математики; их штатные тесты прошли, новые integration paths прочитаны и проверены. Дополнительный аналитический sweep покрывает малые/средние суммы, не весь U256-domain. Это review статически заданных M3 baseline policies, не гарантия безопасности произвольных будущих AgentPolicy implementations.

Отдельное ограничение поиска: Accepted DEC-20 ограничивает число полных кандидатов/пробных executions, и код этот предел выдерживает. Количество неполных DFS prefixes отдельно не ограничено (`heterogeneous.rs:307`); latency на очень больших топологиях этим review не доказана. Это отмечено как граница проверки, а не как подтверждённое нарушение принятой спецификации.

Дополнительные тесты сохранены в репозитории: `crates/chainarium-core/tests/independent_dec20.rs`. Они подключены к обычному Cargo test; строки исходного review выше относятся к снимку `1bc17ee`.
