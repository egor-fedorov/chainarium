# Независимый review DEC-26 / M4f

**Результат для проверенного head:** подтверждены два findings: **P1 — обход ограничения на чужие wallets через preflight; P2 — потеря причины раннего отказа отправки в baseline Decision/UI/export**. Подтверждённых P0/P3 нет. Исправления, обсуждаемые автором после сообщений review, в этот результат не включены.

## Зафиксированная область

- Diff: `f65c8b3cc7a0511417a30cad2ae0dd3c45a8f7d0..fc364d9c093d4af8837b342854eff3a64bd302ef`.
- Каноническая спецификация из архива **fc364d9**: §13, §13.1, §23.2.14 Accepted DEC-26, §23.3–23.4; применимые ранее принятые native authority/cost/lending/oracle/RNG и DEC-20/21/22/24 контракты. Новый proposed DEC-27 и M5 не рассматриваются.
- Проверены изменённые core policy/runtime/run/model/report, интеграция scheduler, CLI exports, шесть fixtures, UI `agents.js` и его подключение; необходимые существующие executor/inclusion/accrual/TWAP/authority paths прочитаны как зависимости diff.
- Источники доказательств: спецификация, git diff, исходники, pre-implementation `docs/reviews/baseline-agents-reference.py/.json`, собственные воспроизведения. Записи прогресса и утверждения автора не приняты как доказательство.
- Один отдельный reviewer, без дочерних агентов. AGENTS.md не обнаружен в /workspace и репозитории. Прочитан cloud-environment-runtime skill. Исходники получены `git archive fc364d9` в `/tmp/chainarium-dec26-review`. Основной checkout не редактировался; commits/push/publication не выполнялись. Временные тесты/инструментация/экспорты находятся только в /tmp. Сервис 8080 не использовался и не менялся; браузер проверял отдельную сборку на 18026.
- Toolchain: Rust 1.99.1 через `scripts/cargo.sh`, offline/locked, cache `/workspace/.chainarium-tools`.

## F1 — P1: SuccessfulOnly превращает полный preflight в бесплатную проверку чужого wallet

**Место в diff:** `crates/chainarium-core/src/scheduler/runtime/baseline_calls.rs:70–93`, особенно выполнение на полном preview (73–74) и зависимость Submit/Skip от receipt (76–91). Политики валидируют шаблон только через общий `validate_tx` в `policy/agents.rs:460–474`. Общий executor разрешает `AssertBalance` любого зарегистрированного account (`model.rs:644–656`, `engine.rs:854–870`); это допустимый native call, но ему не соответствует ограничение доступа новой роли.

**Нарушенный контракт:** §13/DEC-26 разрешают policy только собственные balances/nonce, публичные venues/feeds и собственную borrower position. В §23.2.14: проекция «не открывает ... чужие wallets». Изоляция только полей `Observation` недостаточна, если решение отправки зависит от скрытого wallet через симуляцию.

**Минимальное воспроизведение:** исходный `sc-agent-random.json`; заменить роль на ScriptedUser, `SuccessfulOnly`, событие round 1:

```json
[
  {"type":"AssertBalance","account":"bob","asset":"Q","minimum":"1"},
  {"type":"Transfer","to":"bob","asset":"Q","amount":"10"}
]
```

Сравнить два валидных сценария, отличающихся только `accounts.bob.balances.Q = "0"` / `"1"`. `bob` не является account этой policy. Остальные параметры и доступное наблюдение одинаковы. Тест сравнивает целиком `Decision.observation` и подтверждает равенство.

| Значение | bob.Q = 0 | bob.Q = 1 |
|---|---:|---:|
| Разрешённое Observation | одинаковое | одинаковое |
| Decision | Skip, `Preflight: balance assertion` | Submit, `PreflightSuccess` |
| Canonical alice.Q | 1000 | 865 |
| Canonical alice.nonce | 0 | 1 |
| Canonical bob.Q | 0 | 11 |
| Canonical builder.Q | 0 | 125 |

В правой ветке 125 units = 100 envelope + 10 AssertBalance + 10 Transfer + 5 ledger transfer; плюс перевод 10 Q даёт `alice=1000−125−10=865`. Левая ветка получает результат скрытой проверки без канонической платы. Это изменение поведения агента, а не только исследовательский просмотр скрытого поля. При последовательных шаблонах такая возможность позволяет условно отправлять действия на основе недоступного состояния, обходя контракт общей границы policy.

**Артефакты:** `/tmp/dec26-regressions/hidden-wallet-0.json`, `hidden-wallet-1.json`; integration regression `other_wallet_assertion_cannot_gate_submission`; лог `/tmp/dec26-independent-final-tests.log`; CLI exports `/tmp/dec26-export-hidden-wallet-0` и `...-1`.

**Ожидаемое исправление:** enforce доступные чтения/действия preflight для данной роли, включая вложенные Sequence/Flash callback, либо отклонять программы, использующие недоступные wallets/positions. Обычный native executor не обязан менять семантику assertions. Ошибка не устраняется одним удалением trial из UI: Submit/Skip уже раскрывает результат. Regression допускает одинаковый validation rejection обоих сценариев как допустимое исправление; отдельный `own_wallet_assertion_remains_supported` проверяет сохранение собственного assertion.

## F2 — P2: KnownTransactionsLimit теряется в baseline Decision

**Место в diff:** `crates/chainarium-core/src/scheduler/runtime/baseline_calls.rs:99–117`. Decision/trace `Baseline` уже закрепляют `Submit`/`PreflightSuccess`, затем вызывается `submit`, возвращающий лишь `Result<(), String>`. На штатном отказе по known cap `runtime.rs:111–123` записывает `KnownTransactionsLimit` и возвращает `Ok(())`, не регистрируя TxId. Новый baseline path не обновляет Decision по этому исходу. `web/agents.js:17,24` показывает старую причину и `не зарегистрирована`; `run.rs:289–300` экспортирует тот же Decision.

**Нарушенный контракт:** DEC-26 требует явного результата/причины решения и связи с фактическим TxId/status; отказ отправки является пропуском события, а общие caps DEC-21 сохраняются. Правильное соблюдение лимита ledger не компенсирует отсутствие причины в новом контракте baseline diagnostics.

**Воспроизведение:** `sc-agent-random.json`, `limits.known=1`, `probability_ppm=1000000`, один шаблон Transfer 10 Q, price 1. Round 1 исполняется. Round 2 выбирает тот же шаблон, успешно проверяет preflight, но новый TxId не помещается в known registry.

Фактический round 2:

- `Decision.action=Submit`, `reason=PreflightSuccess`, есть attempted transaction и trial Success/fee115.
- В scheduler trace — `KnownTransactionsLimit` для `policy:baseline:trader:2`.
- Этого TxId нет в `state.known`; canonical receipt отсутствует. Статус, на который ссылается UI, не существует.
- Canonical средства после round 1 и round 2 одинаковы: alice875, bob10, builder115, nonce1; проба round 2 ничего не списала.
- Проверенный DOM baseline panel: `Submit · PreflightSuccess`, затем «не зарегистрирована»; `KnownTransactionsLimit` в этом panel отсутствует. Полный scheduler trace содержит правильную причину, поэтому это диагностический/контрактный дефект, не потеря денег или нарушение самого cap.

**Артефакты:** `/tmp/dec26-regressions/known-cap.json`; regression `known_cap_refusal_has_explicit_skip_decision`; `/tmp/dec26-known-cap-ui.txt`, `.png`; CLI export `/tmp/dec26-export-known-cap`.

**Ожидаемое исправление:** возвращать/сохранять структурированный исход попытки отправки и обновлять baseline Decision при раннем refusal, сохраняя attempted TxId и diagnostic trial для объяснения. Аналогичный путь QueueLimit и dedup также заслуживает проверки. Следующее явно запланированное событие должно оставаться отдельным решением, без скрытого retry.

## Проверенные инварианты и численные результаты

**Исходный suite без добавленной инструментации:** `scripts/cargo.sh test --workspace --locked --offline` — **367 passed, 0 failed**. Полный лог `/tmp/dec26-baseline-test.log`. Это существующие тесты; их прохождение рассматривается совместно с независимыми проверками, а не как независимое доказательство.

**Независимые integration tests:** 8 тестов, **6 passed / 2 failed** на fc364d9; оба отказа соответствуют F1/F2. Файл `/tmp/dec26-regressions/dec26_independent.rs`, vectors рядом; полный лог `/tmp/dec26-independent-final-tests.log`.

- **RNG scalar matrix:** 5 seeds (0, 1, 42, u64::MAX, U256::MAX) × 3 AgentIds (включая UTF-8 и delimiters) × 5 количеств templates (1,2,3,7,16) × 5 probabilities (0,1,500000,999999,1000000) = **375 сценариев**, по 8 rounds = **3000 пар gate/template**. Сравнены gate, index, word position и итоговые переводы/nonce с Python scalar ChaCha12 из pre-implementation reference. Каждый итоговый checkpoint восстановлен replay с точным равенством Run. Seed42/trader даёт gate41542/769541/966069/475919/922990/769445, отправки20 в rounds1/4, alice730/bob40/builder230/nonce2/position24.
- Uniform rejection прочитан отдельно: bound в u128, принимает `x<floor(2^64/n)*n`, максимум32 draws; template не начинается при gate exhaustion. Исходный unit тест проверяет threshold−1/threshold,32 rejects, n1/2/16. По коду 64 raw u64 draws/eligible agent, при validated cap64 agents —4096. Статистическая частота не использована вместо доказательства границы.
- **Borrower priority matrix:** 7 debt targets (0,79,80,81,90,91,100) × 5 collateral targets (0,5,6,7,12) = **35 случаев** после реально исполненных Deposit6/Borrow80. Сравнен выбранный единственный call и точная сумма, включая недоступный collateral; приоритет repay → deposit → borrow → withdraw → hold и C+D=S+E сохранены. Core сохраняет authority/LTV/HF/cash/fee reserve; policy не уменьшает сумму по cash. Исходные тесты дополнительно проверяют LTV90/91 при collateral6×20×75%, stale/cash, ресурс114 против115, accrued90 и repayments10/80/11 при rate12.5%.
- **Publisher competition:** два native Spot publishers независимо получают trial Success/sequence1. Фактический первый receipt Success, второй ProtocolRevert «out of order»; каждый платит110, nonce каждого1, Q каждого890, record.sequence1. Никакой trial fee не удвоен. Для demo независимо пересчитаны SHA-256 canonical tuple `(TxId,FeedId)` update IDs.
- **Private/future noninterference:** шесть demo ролей/режимов повторены с разными величинами недоставленных private transfers и future transfers между посторонними wallets. Все baseline decisions равны. Это положительный контроль обычного пути; он не отменяет F1 при явном скрытом assertion.
- **Replay corruption:** baseline checkpoint с inflight intent; по отдельности изменены emission_cursor, due_at очереди и RNG position, после чего пересчитан корректный checksum. Все три restore отвергнуты с replay mismatch.
- **Собственное чтение:** own AssertBalance → Transfer10 остаётся Submit/Success с alice865; этот положительный контроль нужен при исправлении F1.

**Независимая unit-инструментация:** `/tmp/dec26-regressions/enable-unit-tests.patch` включает два теста через существующие private fault hooks, без изменения production поведения. Они прошли. Лог `/tmp/dec26-independent-unit.log` также содержит один ранее существовавший DEC-25 test, совпавший по имени фильтра; это не новая проверка DEC-25.

- Counter на probe hook: ровно1 preflight для selected ScriptedUser/RandomUser/Borrower/Spot/Twap (в том числе NotReady trial), ноль при RandomHold; Hold всё равно расходует две выборки/4 words.
- Injected ModelError после фактически построенного paid ProtocolRevert (fee125, alice875) не коммитит parent world, previous scheduler state или RNG. Повтор без fault получает тот же результат и position4. Дополнительно исходные fault tests покрыли ошибку в probe/после успешного intent и физическое истощение stream.

**Schedules/caps/overflow:** чтением и существующими успешно выполненными тестами проверены unique schedule0..1024, invalid1025, exact block/no retry, cadence first/last/every, probability boundaries, templates1/16/17, feeds1/16/17, одна policy на AgentId, M1 incompatibility, current nonce guard включая незавершённые bundles, nonce/deadline/delivery/sequence overflow, delayed/expired intents, reserved TxId prefix, общий declared block limit. F2 выделяет недостающий исход именно в новой baseline диагностике.

**Oracle/execution:** один read-only `begin_block(parent)` для shared baseline/liquidator preview; final block строится из parent, без повторного canonical accrual. Spot цена/observed_at берутся в actual execution; TWAP end фиксируется на cutoff. Штатные exact cases: Spot20→5 после swap10X→100Q; TWAP round1 NotReady/no fee, round2 price20/sequence1/observed_at2/published_at2/used111. Delay сохраняет TWAP end2 при published_at3. Multi-feed preflight failure не отправляет частично готовые publications. SuccessfulOnly preflight не гарантирует actual Success: исходный test рыночного изменения даёт trial Success120 → actual revert110 с платой110.

**CLI/export/UI:** отдельная сборка fc364d9 на loopback18026; six demos завершены через реальный browser/UI. Итоги: random730, scripted740, preflight865, borrower540, spot780, twap889 Q у соответствующего отправителя. Переключение genesis/блоков и HTTP reads не изменили checkpoint; restore дал тот же Run. JS ошибок не было. `/tmp/dec26-browser.log`, `dec26-browser-results.json`, переносимый `browser-check.py`. Шесть CLI exports: `baseline-decisions.jsonl` поблочно и целиком совпадает с `run.json` scheduler.baselines, имеет phase `BaselineDecisionDiagnostic`; actual receipts остаются отдельно. Лог `/tmp/dec26-export.log`. Browser также воспроизвёл F2.

## Ограничения и статус

Review проверяет указанный diff и его экономически существенные зависимости, а не полную корректность всех ранее реализованных протоколов. Финитные матрицы не являются исчерпывающим доказательством для всех U256 значений, всех программ, всех комбинаций очередей/семейств и всех interleavings. Редкая 32-reject ветка проверена детерминированной unit инъекцией существующего теста и чтением кода, а не ожиданием случайного совпадения в настоящем ChaCha stream. Глобальный worst-case4096 является выводом из bounds/caps; полный искусственный worst-case4096 прогон не выполнялся.

Прямой policy Observation не содержит seed/future draws, wallets других accounts, pending/private payloads или future emissions. Однако F1 показывает, что этого структурного свойства недостаточно для поведенческой изоляции. Протокольные receipts могут содержать исследовательскую диагностику; вывод F1 основан именно на изменении Submit/Skip и реальных выплат, поэтому не требует приравнивать любой diagnostic trace к доступному policy observation.

Не выполнялись полная UI приёмка M5, нагрузочные эксперименты, MTM/RL/strategy quality оценка, новый DEC-27, долгие переполнения живым количеством2^68 words. Отсутствие иных подтверждённых findings не означает доказанную корректность или принятие полной v1.0. Для fc364d9 устранение или явное принятие F1/F2 остаётся gate §23.4; исправленный hash требует отдельной перепроверки.
