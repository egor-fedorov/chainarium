# Независимый review DEC-27–DEC-28

Дата: 2026-11-10. Проверка выполнена в отдельном контексте по `docs/spec.md §23.4`.

Основной проверенный commit: `e32849c870b3e22a8885bb5e62b0d4819aa4ce3f`; исходный diff: `d4d5726..e32849c`. Отдельно проверено исправление `22b1be27f02ab80320ba1d587ff997ea74d74c5b`.

Результат: подтверждён один P2 в проверке capital binding. Он исправлен в `22b1be2` и независимо перепроверен. Других подтверждённых findings по указанному scope не обнаружено. Открытых findings этого review после fix verification нет; это не утверждение абсолютной корректности и не закрытие следующих gates M5.

## Метод и границы работы

Прочитаны Accepted DEC-27 (§23.2.15), DEC-28 (§23.2.16), §23.4 и связанные execution, financial, scheduler и valuation контракты. Использованы pre-implementation `flash-arbitrage-reference.py/.json` и `valuation-reference.py/.json`; оба JSON повторно сгенерированы Python-скриптами и совпали с сохранёнными fixtures.

Основной репозиторий использовался read-only. Проверяющий не изменял его файлы, не делал commit/push/publish, не запускал других агентов и не трогал сервер 8080. Сборка, дополнительные проверки и воспроизведения выполнялись в двух `git archive` snapshots точных commits внутри `/tmp/dec27-dec28-independent-review`. Применён skill cloud-environment-runtime; зависимости не устанавливались. Для HTTP/browser проверок использован отдельный localhost:8091.

Доказательства не ограничены тестами автора: дополнительно написаны отдельные Python integer/Fraction модели flash-циклов, DFS поиска, извлечения компонентов и оценки committed exports. Они не импортируют Rust evaluator/executor. Существующие тесты использованы как дополнительная регрессионная проверка.

## F-01 — P2: незарегистрированный sender допускается в capital binding

Статус: **исправлено и независимо перепроверено в `22b1be27f02ab80320ba1d587ff997ea74d74c5b`**.

Место на исходном commit: `crates/chainarium-core/src/valuation/config.rs:193–197`, условие в `validate` для единственного `Transfer`.

DEC-28 допускает binding только к literal sole Transfer с обычными endpoints. Валидатор проверял регистрацию `to`, но для `sender` вызывал только `protected()`. Неизвестный AccountId не считается protected. Поэтому при известном `to`, находящемся внутри portfolio, неизвестный sender проходил обе проверки: ordinary boundary и XOR принадлежности портфелю. Scheduler намеренно допускает невалидные literal emissions до admission, поэтому его общая валидация этот пробел не закрывала.

Минимальное изменение валидного scheduler scenario:

1. Взять `sc-scheduler-public.json`, удалить reactive policies, оставить одну emission `victim` и один round.
2. Поставить `victim.sender = "unregistered-sender"`; программа — ровно `[Transfer { to: "alice", asset: "X", amount: "1" }]`.
3. Включить `reference-mtm-v1`, portfolio `owner = ["alice"]` и capital binding `{portfolio: "owner", tx_id: "victim"}`.

Готовый самодостаточный reproducer: `/tmp/dec27-dec28-independent-review/unregistered-capital-sender.json`.

Воспроизведение исходного поведения из snapshot:

```sh
/tmp/dec27-dec28-independent-review/source/target/debug/chainarium run \
  /tmp/dec27-dec28-independent-review/unregistered-capital-sender.json \
  /tmp/dec27-dec28-independent-review/repro-original-fresh
```

Ожидалось: `Scenario::parse` отклоняет capital configuration с незарегистрированным endpoint.

Фактически на `e32849c`: exit code 0, `1 committed blocks exported`; emission позже получает `Rejected: invalid envelope or unauthorized sender`. Binding уже принят и сохранён в export.

Влияние ограничено обязательной валидацией конфигурации. Ложный capital flow или ошибочное включение Transfer не подтверждены: у reproducer нет committed receipts перевода и `capital-flows.jsonl` пуст. Приоритет P2 относится к нарушению принятого input contract, а не к потере средств.

Fix добавляет `!s.accounts.contains_key(&tx.sender)` в тот же predicate. Независимая проверка отдельной сборки `22b1be2`:

- Исходный неизменённый reproducer теперь получает ошибку `capital Transfer must cross ordinary portfolio boundary` до исполнения.
- Контроль без capital label по-прежнему допускается как обычная невалидная emission и наблюдаемо отклоняется на admission.
- Контроль с зарегистрированным `bob`, sole Transfer `1 Q` в `alice` успешно исполняется и создаёт ровно один фактический inflow.
- Добавленный автором targeted integration regression также прошёл.

Отдельный скрипт перепроверки: `/tmp/dec27-dec28-independent-review/verify_fix.py`. Этот finding сохраняется в истории review, хотя после fix verification закрыт.

## Проверенные экономические и программные границы

DEC-27: вручную проверены restricted observation и фиксированная simulation capability, собственный fee reserve до займа, provider asset/cash и ceil repayment, точная форма closed RequireFull программы, root net guard после flash settlement, направление/наличие CLMM/book limits, actual output каждой ноги, отсутствие канонических эффектов probes, tie-breaking, общие caps и AgentId ordering. Проверены runtime prechecks nonce/time, типизированный submit outcome и поведение reexecution на actual prefix. Изучены копирование preview/round/RNG и commit граница scheduler. Отдельных reference valuation inputs в policy observation нет.

DEC-28: вручную проверены reduced signed rational arithmetic, floor к −∞, per-operation bit cap и Unpriced partial marks; quantity/value aggregation и gross exposure до netting; wallet, CPMM/StableSwap LP, native CLMM principal/collectable fees, book remaining escrow, collateral, post-loss deposit claim/cash_bound и active debt. Проверены отсутствие двойного счёта reserves/escrows/fees, latest available record, FutureObservation/Stale без fallback, actual block timestamp для capital flows, sole literal Transfer bindings, Own collisions, payment restrictions и commit всей bundle группы. Проверены read-only evaluator, export/manifest/API/UI и reconstruction из checkpoint.

## Реально выполненные проверки

На основном snapshot `e32849c`:

- `./scripts/cargo.sh test --locked --offline -p chainarium-core --test valuation --test flash_arbitrage`: **36/36** tests passed (16 valuation, 20 flash).
- `./scripts/cargo.sh test --locked --offline -p chainarium-core --lib flash`: **2/2** tests passed; fixed-program capability и injected probe/commit ModelError с rollback всех families/RNG.
- Независимый differential: **160** двух-CPMM flash scenarios. Варьировались reserves, fees, principal, resource price/limit, own balance, provider cash и minimum. Python integer расчёт сверял выбранное направление/net, отсутствие canonical effects при Skip, actual 190 units, provider fee и конечный own balance.
- Независимый DFS differential: **60** multigraph scenarios, 2/3 hops, уменьшенные venue/trial caps, лексикографический порядок, best completed candidate, truncation и shared budgets нескольких agents. Сверены trials, venue_checks, decisions и выбранные paths/net/cost.
- Независимый Fraction differential: **100** portfolios с 180-bit balances и рациональными ценами; сверены exact aggregation, canonical reduction и floor, overlapping views и нулевой genesis PnL.
- Независимое извлечение **129** компонентов во всех boundaries шести MTM demos, плюс отдельный StableSwap scenario с тремя portfolio views. Количества вычислены из canonical snapshots отдельной Python моделью; CLMM principal использует integer `isqrt` и точную Q96/Q128 арифметику. Дополнительно сверены полные значения, partial values, flow prices и PnL через Python Fraction.
- Live lending scenario с `V = -1/2`, `floor_atoms = -1`; отдельный живой `+80 R / -80 R` offset без цены остаётся Unpriced при priced_part `1/2`.
- Отдельный latest-record scenario: доступный future-observed record не заменяется старым валидным; проверены момент observed_at и точная max_age граница/следующая секунда.
- Отдельные VisibleTxId bundle scenarios: success создаёт ровно один flow; поздняя невалидность payment nonce откатывает всю группу и создаёт **ноль** flows; Own alias того же literal TxId отвергается при validation.
- Изменение только valuation prices/limits на flash и random-agent scenarios не изменило `initial`, `state`, `blocks`, scheduler или RNG; run_id изменился ожидаемо.
- CLI checkpoint/resume на genesis/mid/final boundaries **10** flash/MTM demos: полные run exports и valuations совпали с непрерывным исполнением.
- HTTP/Chromium smoke шести MTM demos на отдельном 8091: selected history, readonly повторные чтения, restore/replay, точный fractional wire и JSON download, capital/PnL/Unpriced, mobile layout, opt-in старых scenarios. Использован имеющийся browser regression script с изменением только BASE в временной копии. Desktop screenshot valuation панели дополнительно просмотрен вручную. Скрипт прошёл, page errors отсутствовали.

На fix snapshot `22b1be2`: заново собран CLI; выполнены три независимых reproducer/control проверки F-01 и targeted integration regression (1/1 passed). Полный исходный differential/browser набор на fix не повторялся: production diff состоит только из проверки регистрации sender; широкие результаты выше относятся к `e32849c`.

Основные собственные артефакты в `/tmp/dec27-dec28-independent-review`:

- `differential.py` — flash arithmetic, wide Fraction wallets, component extraction;
- `search_differential.py` — bounded DFS/shared budgets;
- `valuation_adversarial.py` — полная оценка/PnL, StableSwap, references, record boundaries, bundles;
- `negative_fraction.py` — live signed fraction и gross missing-price offset;
- `noninterference_replay.py` — execution/RNG independence и CLI replay;
- `unregistered-capital-sender.json`, `verify_fix.py` — F-01 и fix verification;
- `differential/` — входные scenarios и CLI exports;
- `ui-smoke.py`, `output/` — browser smoke и screenshots.

## Ограничения и остаточные риски

Review не является exhaustive proof всех U256 состояний, всех mixed-venue/hook комбинаций или всех scheduler interleavings. Независимая генеративная арифметика flash-поиска покрывает CPMM; mixed CLMM/book/StableSwap routes, hook rollback/payout и ряд nonce/time/U256 boundaries проверены source inspection плюс имеющимися targeted tests, без отдельной новой реализации этих executors. Инъекция ModelError использует существующий unit-test seam; самостоятельный production ModelError scenario не создан.

Не выполнены новые worst-case performance measurements для максимальных 16 portfolios × 4096 scans × 10000 blocks или максимального rational_bits, многопользовательская HTTP concurrency проверка и полный browser набор предыдущих срезов. Арифметические/component caps проверены по реализации и имеющимся tests; их исчерпывающая комбинация с каждым extractor не перебиралась.

Следующий Proposed experiment contract и работа над ним вне scope. Результат этого review позволяет считать собственный independent-review gate DEC-27–DEC-28 выполненным после указанного исправления, но не заменяет пользовательскую приёмку, comparative A/B, HODL/explorer и performance gates следующих срезов M5.
