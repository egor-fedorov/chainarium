# DEC-26 / M4f — независимая перепроверка исправлений F1/F2

**Вывод:** на `ce78187b364de1aeb7bada27abbe3538fea36852` исходные **F1 (P1) и F2 (P2) закрыты** по воспроизведениям и дополнительным граничным проверкам ниже. Новых подтверждённых P0–P3 в рассмотренном fix diff не найдено. Это адресная перепроверка, а не доказательство полной корректности v1.0.

## Scope и происхождение проверки

- Проверенный diff: `fc364d9c093d4af8837b342854eff3a64bd302ef..ce78187b364de1aeb7bada27abbe3538fea36852`.
- Исходный review остаётся отдельным: `/tmp/dec26-independent-review.md`; его findings и первоначальные failing logs не переписаны.
- Источники: точный git diff, исходники, §13/Accepted DEC-26 с уточнением enforcement, действующие scheduler/cost/authority/RNG/replay контракты, исходные репродукторы, новые собственные тесты. Авторские утверждения о checks не использовались как доказательство.
- Изолированный `git archive ce78187` в `/tmp/chainarium-dec26-fix-review`. Основной checkout не менялся; commits/push/publication/другие агенты отсутствуют. 8080 не использовался; отдельный сервер во время этой перепроверки не запускался. Новый DEC-27 и последующий docs commit не входят в scope.
- Проверены локальные инструкции; AGENTS.md не обнаружен. Применён ранее прочитанный cloud-environment-runtime; readiness проверена заново. Rust/Cargo 1.99.1, locked/offline, `/workspace/.chainarium-tools`. Для ускорения повторно использован только compilation target под `/tmp/chainarium-dec26-review/target`.

## F1 — закрыт: capability enforcement до полной simulation

**Проверенный механизм:** `policy/agents.rs:296` вводит исчерпывающий match всех native Call variants. AssertBalance/AssertBalanceDelta, TransferFrom, Repay/Liquidate/LiquidateAndSell допускают только policy account в соответствующем поле. Sequence и Flash callback проверяются рекурсивно. Общая validation сначала проверяет обычные node/depth/authority/envelope ограничения, затем capability. В `scheduler/runtime/baseline_calls.rs:70` тот же capability check повторён после inclusion validation, **до** `engine::execute`; AnyReceipt не обходит этот check.

**Исходные CLI fixtures:** `hidden-wallet-0.json` и `hidden-wallet-1.json` из первоначального review теперь оба отвергаются с `BaselinePrivateRead`; output directory не создаётся. Отличие чужого Q-баланса больше не меняет отправку/плату. Исходный regression и отдельный положительный контроль own assertion проходят в полном suite.

**Новая независимая матрица:** 6 запрещённых calls × 5 форм вложенности × ScriptedUser/RandomUser × SuccessfulOnly/AnyReceipt × два скрытых баланса = **240 случаев**. Формы: root, Sequence, Flash callback, Sequence→Flash→Sequence и глубокая цепочка Sequence. Для обычных calls —31 wrapper (native depth32); для LiquidateAndSell —28, чтобы сохранить предел для его собственных native children. Это валидные по структурным caps программы: отказ происходит именно по `BaselinePrivateRead`, а не из-за случайного другого validation error.

Для каждого случая проверены:

1. Статический validation rejection с `BaselinePrivateRead` при чужом account; соответствующая версия с own account проходит validation.
2. Защитный runtime путь: сначала создан валидный Run, затем template заменён на запрещённый. Step даёт Skip; trial, transaction и submission отсутствуют; нет canonical receipt, платы или изменения nonce.
3. Полные Decisions для двух вариантов скрытого баланса одинаковы. У RandomUser остаётся ровно4 использованных words; ScriptedUser выборок не создаёт.

Отдельный unit probe настроен на panic при достижении executor callback: nested forbidden template в обоих режимах не достигает probe. Это подтверждает, что receipt не просто удаляется из диагностики после уже выполненной запрещённой simulation.

**Положительный execution control:** nested own AssertBalance остаётся допустимым в обоих preflight modes. Успешный own assertion стоит120 units и оставляет alice880/nonce1. Неуспешный own assertion с SuccessfulOnly даёт Skip/alice1000/nonce0; тот же call с AnyReceipt включается как ProtocolRevert и реально платит120, alice880/nonce1. Обычные native calls и paid-failure semantics не заменены blanket rejection.

**Соседняя поверхность ownership diagnostics:** новые правила не меняют native Burn/Collect/Cancel errors. В дополнительном сценарии CancelOrder(id1) при отсутствующем и чужом order приводит к одинаковому restricted Observation, одинаковому action/transaction и trial ProtocolRevert/used110. В SuccessfulOnly обе ветви Skip и alice1000; в AnyReceipt обе Submit, actual fee110 и alice890. Текст `unknown order id` / `order owner required` остаётся разным в диагностике. Для этого случая поведенческий обход не воспроизведён; различие исследовательских сообщений не выдано за новый finding.

## F2 — закрыт: immediate submission outcome сохраняется в Decision

**Проверенный механизм:** `scheduler.rs:227` определяет typed `SubmissionRejection` и `SubmissionOutcome`. `Round::submit` возвращает Queued только после успешного enqueue builder delivery и возвращает Rejected для Duplicate/ConflictingTxId/KnownTransactionsLimit/QueueLimit. В `baseline_calls.rs:101` outcome сохраняется; при Rejected Decision становится Skip с причиной `Submission: ...`. Attempted transaction и trial остаются, а Baseline trace записывается уже после исхода отправки и содержит TxId. Admission/expiry/inclusion сохраняют отдельный committed status.

**Независимые наблюдаемые результаты:**

| Сценарий | Результат |
|---|---|
| `known=1`, Transfer10 каждый eligible round | Round1 Queued/Success; rounds2–6 Skip/Rejected KnownTransactionsLimit, попытки отсутствуют в registry. Во всех round сохраняются новые samples, позиции4/8/12/16/20/24. После round1: alice875, bob10, builder115, nonce1; последующие trials не меняют средства |
| `queue=1`, очередь занята отложенной внешней delivery | Все6 baseline attempts Skip/Rejected QueueLimit; каждый attempted TxId имеет terminal Rejected в known, существующая очередь сохраняется. Alice1000, builder0, nonce0; RNG positions4…24; trial Success/fee115 остаётся только диагностикой |
| `queue=1`, builder delivery заняла единственное место; сообщения observers не поместились | Submission остаётся Queued, actual inclusion Success. Шесть переводов: alice250, bob60, builder690, nonce6. Observer QueueLimit не ошибочно превращён в отказ builder submission |
| Builder delay2, lifetime1 | Rounds1/3/5 отправляют Queued; rounds2/4/6 имеют BusyNonce, затем actual admission истекает с ExpiredDeadline. Ранний Queued не переписывается в submission rejection; fees/nonce отсутствуют |

Для Known/Queue сценариев checkpoint восстановлен после **каждой** границы с точным равенством Run. Trial fee115 не попадает в canonical charges при refusal. Отдельный unit test доказывает, что Duplicate и ConflictingTxId сохраняют первый payload, known registry, queue, pending и event counter; Known cap не добавляет registry entry, Queue cap добавляет только terminal rejection нового ID. Повтор уже отвергнутого по QueueLimit payload даёт Duplicate без скрытого retry.

**Atomic rollback:** введён ModelError после построенного раунда с отказом QueueLimit. Parent World, previous State и RNG точно сохраняются; повтор без fault воспроизводит тот же отказ и position4. Таким образом новый typed outcome не создаёт частичный commit при исключении.

**UI/export:** два новых CLI exports (`known.json`, `queue.json`) содержат одинаковые Decisions в `run.json` и `baseline-decisions.jsonl`, с phase `BaselineDecisionDiagnostic` и typed Rejected. Неизменённый фактический `web/agents.js` исполнен в Node с минимальным DOM sink на этих реальных Rust exports. Проверены `Skip`, `Submission: KnownTransactionsLimit/QueueLimit`, отдельная строка «Отправка: отклонена», trial Success отдельно от текста об отсутствии canonical fees. В refused panel больше нет `PreflightSuccess`; успешный round1 отдельно показывает «поставлена в очередь». Это проверка source renderer, а не полный browser/layout test.

## Выполненные проверки и переносимые материалы

**До добавления новых временных тестов:** полный точный head suite:

```text
CARGO_TARGET_DIR=/tmp/chainarium-dec26-review/target scripts/cargo.sh test --workspace --locked --offline
381 passed, 0 failed
```

Лог: `/tmp/dec26-fix-workspace-tests.log`. В этот suite уже входят первоначальные8 independent integration и2 unit tests, перенесённые автором без использования его результатов как доказательства.

**Новые независимые integration tests:** `/tmp/dec26-fix-regressions/dec26_fix_adversarial.rs` — **6 passed**; лог `/tmp/dec26-fix-adversarial-final.log`. Покрывают матрицу240, Known/Queue/replay/RNG, observer-only overflow, позднюю expiry, own execution/AnyReceipt и bounded ownership diagnostic comparison.

**Новая test-only unit instrumentation:** `/tmp/dec26-fix-regressions/enable-unit-tests.patch` и `fix_independent.rs` — **3 passed**; лог `/tmp/dec26-fix-unit.log`. Patch добавляет только `#[cfg(test)]` module declaration и три unit tests; production runtime behavior не изменяется.

**CLI/source renderer:** `/tmp/dec26-fix-export-known`, `/tmp/dec26-fix-export-queue`; `/tmp/dec26-fix-ui-source.log`; `/tmp/dec26-fix-known-ui.html`, `/tmp/dec26-fix-queue-ui.html`. Переносимые fixtures и `ui-source-check.mjs` находятся рядом с тестами. Инструкция по переносу: `/tmp/dec26-fix-regressions/README.md`. Добавленные Rust files отформатированы rustfmt.

## Ограничения

Это повторная независимая проверка ограниченного fix diff. Полный browser/release smoke автор выполняет отдельно; я не присваиваю ему статус независимо проверенного результата. Во время этой перепроверки не запускались UI server/Chromium, полная M5 UI приёмка, performance/MTM/RL/DEC-27 проверки.

Дополнительные тесты и cfg(test) modules находились только в /tmp. Build fingerprint включает тестовые файлы, поэтому replay assertions относятся к одному соответствующему build; совместимость checkpoint между разными source fingerprints не утверждается. Полный381-test прогон выполнен на нетронутом archived head до дополнительной инструментации.

Матрица и source inspection подтверждают исправление установленных foreign-wallet/borrower probe путей и submission outcomes; они не являются исчерпывающей моделью всех допустимых native программ, public protocol state, возможных future calls и interleavings. Исчерпывающий match требует явного решения при добавлении нового Call variant, но сам по себе не доказывает будущую корректность такого решения. Для ownership IDs выполнен конкретный CancelOrder comparison, а не полный differential review всех CLMM/book программ.

С указанными границами **F1/F2 можно считать устранёнными на ce78187**. Исходный отчёт сохраняет историческую оценку fc364d9. Этот вывод не означает принятия полной v1.0 и не расширяет согласованный scope на следующий срез.
