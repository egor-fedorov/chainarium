# Прогресс Chainarium — 2026-11-06

Цель: весь v1.0 из §2.1. Текущий результат — **исполняемый прототип M1–M4g**.
Это не готовая v1.0. Независимый review M1–M3b завершён, три findings исправлены
и перепроверены. Отдельный review M3c–M3d завершён без подтверждённых findings;
область и ограничения сохранены в `docs/reviews/dec20-independent-review.md`.
DEC-05–DEC-27 приняты владельцем.
Общий partial-fill router, LP/MM-политики и heterogeneous arbitrage реализованы.
Контракт M4a принят как **DEC-21** (§23.2.9) 2026-11-06;
scheduler/mempool/basic builders реализованы и функционально проверены.
Независимый review M4a завершён без подтверждённых замечаний;
отчёт — `docs/reviews/dec21-independent-review.md`.
M4b / DEC-22 реализован и функционально проверен в `7fc0007`;
независимый read-only review этого snapshot завершён без подтверждённых findings.
Отчёт — `docs/reviews/dec22-independent-review.md`; сохранены 12 независимых
тестов, включая 640 differential cases. DEC-23 в область review не входит.
M4c — **Accepted DEC-23** (§23.2.11): поиск backrun,
ограниченное наблюдение и conditional net после bid. До реализации сохранены
аналитические `docs/reviews/backrun-reference.py`/`.json`. Реализация и Rust/CLI/browser
проверки завершены. Независимый review M4c на `99c26d1` выявил два P2:
исчерпание nonce и overflow lifetime/reaction delay. Исправления `467876f`,
`bcacbcf`, `f99114a` адресно перепроверены reviewer; оба findings закрыты.
Отчёт — `docs/reviews/dec23-independent-review.md`, сохранены16 независимых
тестов и отдельный boundary test. Итоговый suite reviewer —271 passed.
M4d: **Accepted DEC-24** (§23.2.12), LiquidateAndSell + bounded
liquidator, Own/Flash funding и SC-06 competition реализованы. До реализации
сохранены `docs/reviews/liquidator-reference.py`/`.json`. Разрешённый владельцем
отдельный read-only review diff `72590bb..976a5cb` завершён без подтверждённых
findings. Отчёт — `docs/reviews/dec24-independent-review.md`; сохранены7 независимых
тестов, включая768 integer cases и344 resource-boundary executions. **DEC-25** (§23.2.13), hooks CPMM с независимыми численными примерами,
согласован владельцем 2026-11-06. M4e реализован и функционально проверен;
отдельный read-only review hooks diff `145ea8a..d4a4709` завершён без подтверждённых
findings. Отчёт — `docs/reviews/dec25-independent-review.md`; сохранены13 независимых
integration tests и unit fault injection. Для M4f принят Accepted DEC-26 (§23.2.14): scripted/random
user, target borrower и native oracle publisher; владелец согласовал контракт
2026-11-06. Реализация и Rust/CLI/browser проверки завершены;
независимый review M4f выявил два findings; исправления `ce78187` независимо перепроверены, оба закрыты.

## Что работает

Одно Rust-ядро обслуживает CLI и локальный web inspector. Реализованы точные
U256/рациональные цены, ledger, copy-on-write savepoints, atomic block commit,
fees/nonce/resource accounting и deterministic replay. Канонические события
отделены от трассы отменённой попытки; просмотр UI не исполняет транзакции.

- M1: CPMM exact-input swap, Transfer/Approve/TransferFrom, вложенные Sequence,
  assertions, ресурс 100/10/5, late revert/exhaustion/ModelError.
- M2a: proportional LP mint/burn, последний выход/bootstrap, маршруты до
  3 hops / 64 полных кандидатов, реальные outputs, ограниченная arbitrage policy
  на собственных средствах с Observation и simulation тем же executor.
- M2b: отдельный flash reserve, ceil fee, callback authority, запрет reentry
  одного provider, nesting разных providers, атомарный settlement/rollback.
- M2c/M2d: изолированные collateral escrow, supply/withdraw shares, borrow,
  repay за себя/другого, права oracle publisher, age/sequence/source validity,
  block accrual с дробным переносом, constant/kink rates, liquidation, atomic
  write-off E→S, история убытков и последующий вывод вкладчика.
- M2e: stableswap-2-v1, нормализация, D/y solver, proportional LP, смешанный
  маршрут и котировки; стоимость каждой итерации учитывается до вычисления.
  Profit guard использует фактическую execution fee, включая solver work.
- M2f: native spot и TWAP, точный интеграл/ограниченная история, публикация,
  NotReady/gaps, freshness, read-only проекция и интеграция lending.
- M2g: SC-07 объединяет flash/lending/CPMM/StableSwap/TWAP, включая убыток
  и полный rollback недостаточной выручки. Реактивная liquidator policy — M4.
- M3a: on-chain стакан, price-time, целые lot/tick, комиссии на каждый match,
  GTC/IOC/FOK/cancel, защищённый escrow и read-only quote через executor.
- M3b: CLMM positions/ticks, partial swaps, fees/claims/surplus, полная trace
  и обе read-only формы; атомарность сохраняется на crossings и выводах LP.
- M3c: ExecuteRoute через четыре семейства, RequireFull/KeepResiduals,
  typed ledger results, до 3 hops / 64 полных simulations, read-only UI/CLI.
- M3d: passive/scheduled CLMM LP, Fixed/PublicOracle book MM, heterogeneous
  arbitrage с фактической native fee. Одно намерение на AgentId, Hold/Skip/Submit,
  старое committed observation и повторная проверка включения.
- M4a–M4d: scheduler, Public/Private delivery, mempool, builders, bundles с
  реальным bid, bounded backrun и liquidator с Own/Flash funding; конкуренция
  использует общий executor, включённые неудачи оплачиваются.
- M4e: ограниченные CPMM hooks вокруг swap/LP, dynamic fee/input limit,
  собственные counter/cash и выплаты sender; общие caps, resource accounting
  и rollback. Read-only наблюдение и execution quote не запускают effects
  в каноническом состоянии.
- M4f: scripted/random user, target borrower и native oracle publisher.
  Именованный RNG расходуется по расписанию независимо от Hold/BusyNonce,
  ModelError отменяет весь round вместе с RNG. Пользователь выбирает
  SuccessfulOnly/AnyReceipt; trial и фактические оплаченные receipts разделены.

- M4g: flash-funded arbitrage с фиксированным principal/provider, своим native
  fee reserve и root net guard после settlement. Bounded DFS и общий budget
  семейства; RequireFull, typed submit outcomes, diagnostic/actual separation.

UI содержит 58 presets, play/pause/step, просмотр block→tx→call, балансы, сравнение котировок и трасса solver,
LP shares, C/D/S/E, escrow, точный HF и запись oracle, фактически использованную
в конкретном вызове. Расчёт риска выполняет Rust; JS отображает готовые строки.
Есть export/checkpoint/restore/fork и A/B параметров SC-01.

## Фактическая проверка

- `scripts/cargo.sh fmt --check` и Clippy всех targets с `-D warnings` — прошли.
- `scripts/cargo.sh test --workspace --locked --offline` — **412 tests passed** в полном прогоне после M4g:
  23 unit tests (включая fault injection), 19 acceptance, 20 composition, 19 credit,
  10 stable, 10 twap, 7 flash_liquidation, 14 book, 17 clmm, 11 heterogeneous,
  16 policies, 6 independent_dec20, 10 independent_dec21, 12 independent_dec22,
  18 scheduler, 26 bundles, 30 backrun, 16 independent_dec23,
  1 independent_dec23_boundaries, 12 liquidation_sale, 20 liquidator_policy,
  7 independent_dec24, 21 hooks, 13 independent_dec25, 20 baseline_agents, 8 independent_dec26,
  6 independent_dec26_fix, 20 flash_arbitrage;
  ignored=0. Независимые тесты сохраняют аналитические sweeps предыдущих reviews.
  Полный прогон включает перенесённые DEC-25/DEC-26 reviewer tests и регрессии исправлений.
  Release пересобран; экономический reference DEC-26 повторно совпал побайтно.
- `node --test web/wire.test.mjs` — канонические строки, диапазоны и точный
  decimal conversion прошли; JS экономические команды не проходят через Number.
- `python3 scripts/check-ui.py` — пройден путь всех 58 presets, readonly
  просмотр, trace, checkpoint/resume, fork/A-B, точные Amount/Price через HTTP,
  desktop/mobile; ошибок JavaScript и горизонтального overflow нет.
- Release build и `git diff --check` — прошли.
- CLI: все 58 demos исполнены финальной release-сборкой. Для шести новых flash-arbitrage
  demos checkpoint/resume даёт идентичные run/events/probes/delivery/transactions exports. Для шести baseline-agent
  demos checkpoint/resume дал побайтно одинаковые run/events/baseline-decisions/delivery/transactions exports. Для шести hook demos
  checkpoint/resume дал побайтно одинаковые `run.json`, `events.jsonl`,
  `hook-attempts.jsonl` и `transactions.csv`. Оба StableSwap demos после checkpoint/resume
  дают побайтно идентичные `run.json`. SC-05 checkpoint после блока 1 → resume:
  `run.json` побайтно совпадает с непрерывным запуском. В events JSONL начисление
  10 Q присутствует, отменённая liquidation не создаёт канонического write-off.

Артефакты: `output/m2cd-final/`; снимки `output/m2-credit-loss.png`,
`output/m2-credit-rollback.png`, `output/m2-credit-mobile.png`. Desktop/mobile
снимки просмотрены; исправлены скрытие ненужного AMM selector и восстановление
выбранного preset после reload/restore. Export не перезаписывает папки.

### SC-05: точная цепочка lending

Начало: вклад Alice 100 Q, escrow Bob 6 X, borrow 80 Q при цене 20 Q/X,
LTV=75%, LT=80%. Интервал 1 s, rate=0.125M, reserve factor=20%.
Это аналитический fixture, не рыночные ставки.

| Переход | C | D | S | E | T |
| --- | ---: | ---: | ---: | ---: | ---: |
| Supply/collateral/borrow, блок 1 | 20 | 80 | 100 | 0 | 100 |
| Системное начисление +10, начало блока 2 | 20 | 90 | 108 | 2 | 100 |
| Oracle 11 Q/X, liquidate 60 Q за 6 X и atomic write-off 30 | 80 | 0 | 80 | 0 | 100 |
| Withdraw 25 shares → 20 Q, конец блока 2 | 60 | 0 | 60 | 0 | 75 |
| Последние 75 shares → 60 Q, блок 3 | 0 | 0 | 0 | 0 | 0 |

Промежуточное состояние `(80,30,108,2)` — проводка внутри атомарной liquidation,
не отдельная committed-граница. Loss 30 поглощается E=2 и S=28. До успешной
liquidation demo выполняет ту же операцию с поздним отказом: все transfers,
losses и claims возвращаются, уже начисленный системный процент остаётся.
После полного закрытия повторный Repay отклоняется без списания Q.

### Остальные демо

- SC-01: Success/ProtocolRevert/ResourceExhausted; units 120/130/120,
  fees 240/260/240 Q, refunds 160/140/0. Финальный pool 1100 X / 910 Q,
  Alice nonce=3, builder получает 740 Q.
- LP: mint 100 shares, burn 25, следующий mint отменён. Финальные reserves
  и total shares =1075; Alice 9925 Q / 925 X.
- SC-03: policy использует 100 Q, получает 90 X → 123 Q, сохраняет старые X.
  Ограниченный поиск и решение/ожидаемый gain записаны; лучшие маршруты вне
  бюджета не обещаются. Отдельные тесты учитывают ненулевую execution fee.
- SC-04: flash 100 Q, возврат 101 Q, остаток 22 Q; недоплата и поздний отказ
  полностью возвращают состояние. Fee price=0 указан в учебных M2 presets.

### M2e: StableSwap и смешанный маршрут

Принятый DEC-16 реализован без изменения ожидаемых чисел. Аналитический эталон
`docs/reviews/stableswap-reference.py` использует bisection continuous invariant
и quadratic y; его закреплённый JSON совпал с повторным расчётом. Rust выводит:

| Reserves Q/R | Decimals Q/R | Input Q | Output R atoms |
| --- | --- | ---: | ---: |
| 1000 / 1000 | 6 / 6 | 100 | 99860150 |
| 100 / 10000 | 6 / 6 | 100 | 554752748 |
| 1000 / 1000 | 6 / 8 | 100 | 9986015082 |

A=100, fee=4 bps. Проверены normalized domain/U256, min-out/min-shares/min-assets,
zero mint/output, ownership/protected account, полный LP exit/bootstrap,
late rollback, отрицательный iteration limit case и естественное колебание
integer solver на малых единицах. Несходимость даёт явный отказ; подставного D нет.
180 детерминированных cases в обоих направлениях и с A=1/100/1000000 проверяют
сохранение tokens и D с заявленной integer tolerance. Это native-модель;
совместимость с конкретным Curve deployment не заявлена.

В `sc-stable`: первый swap использует 129 units (9 solver iterations).
Proportional mint/burn использует 140; цикл StableSwap→CPMM — 151;
поздний отказ — 162; исчерпание — 111. Последние две попытки не меняют reserves
и LP shares; nonce расходуется. После успешного цикла Alice имеет
1935557132 Q atoms / 2099860150 R atoms (это балансы, не общий MTM/PnL).

`AssertNetBalanceDelta` заменяет фиксированную оценку gas в guard политики:
после всех solver-вызовов он проецирует точный fee/refund settlement. Разрешён
только последним root-вызовом и в fee asset; вложенный/непоследний guard
отвергается. Тесты CPMM policy с ненулевой fee и self fee recipient сохранены.

UI/CLI показывают семь presets, текущие reserves обеих площадок, точные таблицы
котировок, графическое сравнение и каждую итерацию D/y. Query API не меняет
каноническое состояние. Browser проверил success/LP/mixed route/rollback,
оба checkpoint paths, depeg и мобильную ширину. Снимки просмотрены.

Артефакты: `output/m2e-final/` (семь demos и два byte-identical resumes),
`output/m2-stable-curves.png`, `output/m2-stable-solver.png`,
`output/m2-stable-depeg.png`, `output/m2-stable-mobile.png`.

### M2f: TWAP, время и публикация

DEC-17 реализован: точные рациональные сегменты/интеграл, 128 сегментов с
rebase, gaps пустого пула и NotReady неполного окна. Genesis 100 Q/X,
swap в t=30 даёт 200; W=60 даёт 150, W=120 — 175. Окна сопоставлены с
независимой посекундной суммой. Проверены same-timestamp updates, 150 физических
swaps/eviction, границы внутри сегмента, LP gap и однократное floor к 10^18.
Недопустимый PriceOutOfRange сохраняет предыдущую настоящую публикацию.

В `sc-twap` цена 600 из отменённой попытки не попадает в history. Повтор окна
с end=60 в t=120 ещё допустим для borrow (age=60), в t=150 — stale (age=90).
Repay остаётся допустим. Текущая read-only проекция 200 не заменяет опубликованные
150; UI показывает обе цены, окно, сегменты и observation/publication time.
Injected ModelError отменяет integral/publication вместе со всем блоком.

### M2g: реальная flash-funded liquidation

SC-07 использует TWAP X/Q с явно заданным W=1 s: 20 в первом блоке, 11 во
втором. Отдельный TWAP R/Q также опубликован. Reference interest/factor и
waterfall сохраняют числа §8.5. Liquidity venues: sale CPMM 100 X/1500 Q,
StableSwap 1000 Q/1000 R (A100, fee4 bps), exit CPMM 1000 Q/1000 R (fee30 bps).
Все активы имеют 0 decimals. До flash у Carol нет X/Q/R.

| Действие | Реальный ledger-результат |
| --- | --- |
| Flash provider → Carol | 60 Q |
| Liquidation | 60 Q в cash; 6 X из escrow; loss30 = equity2 + claims28 |
| CPMM → StableSwap → CPMM | 6 X → 84 Q → 83 R → 76 Q |
| Flash settlement | 61 Q; provider10000 → 10001 |
| Закрытый цикл | Carol15 Q, X/R=0; execution price=0 явно задана |
| Позже withdraw25 shares | Alice20 Q; C/D/S/E = 60/0/60/0, shares75 |

Ожидания закреплены в `docs/reviews/flash-liquidation-reference.py/.json`;
StableSwap quadratic reference даёт 83.896… R до целого output83.
Первая попытка demo продаёт collateral дешевле через oracle CPMM:
6 X → 54 Q → 53 R → 50 Q. Это меньше due61: весь flash scope, liquidation,
writeoff и source changes отменяются. Независимая публикация TWAP и системный
процент сохраняются. Failure219 units, success229; комиссии исполнения равны 0.

Семь composed tests проверяют реальные transfers, недостаточный output,
min-out, поздний отказ после settlement, каждый resource limit от 100 до 218,
fee-preserving rollback, net guard с ненулевой fee и checkpoint на каждой
границе. При цене 1 Q/unit guard отклоняет цикл: 15 Q не покрывают execution.
UI дополнительно показывает C/D/S/E в сценариях с AMM.

Артефакты M2f/M2g: `output/m2fg-final/` — все десять demos и три побайтно
идентичных resumes; build `531d4803dd191abe4fc42a723604ea1e5ea6928983c3920f83ceb2680bca2f38`.
Browser проверил все десять presets, snapshot viewing, trace, resume и mobile.
Снимки `output/m2-twap-*.png`, `output/m2-composite-*.png`; просмотрены.
Эксперименты выполнены до M3; следующий build намеренно изменит fingerprint.

### M3a: стакан по DEC-18

`native-book-v1` реализован на том же ledger/journal: до 16 стаканов, до 1024
resting orders на каждый, ordered price/sequence indexes, защищённый escrow.
Проверка инвариантов пересобирает index из order records и сопоставляет каждое
обеспечение с реальным balance. Счётчик ID входит в rollback/checkpoint.

Reference lot10 X, quote tick2 Q/lot, maker fee0, taker fee100 bps.
Два maker: 2 lots по 50 ticks и 3 по 55. Независимые исходные ветки:

| Операция | Наблюдение |
| --- | --- |
| Buy4 IOC | 40 X, gross420 Q, fee5 Q, резерв 448, возврат 23 |
| Buy6 IOC | 50 X, gross530 Q, fee6 Q, резерв 672, возврат 136, 1 lot unfilled |
| Buy6 FOK | Prefix виден в попытке; orders/fees/escrow/IDs полностью возвращены |
| Buy6 GTC | 50 X, 1 lot resting/110 Q locked, возврат 26 Q |
| Cancel остатка | Возврат 110 Q владельцу; неизвестный/чужой ID отклоняется |

14 book tests включают эти числа, FIFO, обе стороны, SelfTrade, пустой IOC,
zero-net sell из ceil fee, верхний резерв buy, fee recipient=taker, overflow,
capacity1024, поздний отказ и все 52 resource limits от 100 до 151 для Buy4.
Отдельная flat sorted-list модель совпала с indexed executor на 96 случаях:
цены/объёмы, обе стороны, fees0/1/100/9999 bps и три TIF. Это differential
проверка native-контракта, не заявленная совместимость со сторонней биржей и
не независимый review. Дополнительный injected ModelError отменяет блок с
новой заявкой/escrow/sequence. Read-only quote проверяет nonce/deadline/fee
budget и точный state_ref; stale book исполняется заново, скрытого fill нет.

SC-08 состоит из 6 блоков. Buy4 использует 152 units, FOK prefix147,
late rollback162; empty IOC120 и refund exhaustion115. В конце открытых
заявок/escrow нет, next_id10, treasury16 Q; Alice1980 Q/100 X,
Bob1155 Q/10 X, Carol3849 Q/100 X. Это balances, не MTM/PnL.
Execution price=0 явно задана учебным preset.

UI показывает levels/FIFO queue, locked amounts, gross/fee/net и каждую
отменённую попытку. Форма пробного ордера использует read-only snapshot и
не меняет текущий запуск. Сценарий демонстрирует постановку/отмену шагами;
общий интерактивный редактор и частичный router M3/M5 ещё не заявлены готовыми.

Артефакты: `output/m3a-final/` —11 demos, четыре byte-identical resumes
(стакан, SC-07, TWAP, StableSwap); build
`28a4d3a246aa12cb38d6ae05b8828b374a28038ac6b950531283ee3f0e476061`.
Browser прошёл все 11 presets и read-only quote success/FOK, checkpoint и mobile.
Снимки `output/m3-book-quote.png`, `output/m3-book-fills.png`,
`output/m3-book-mobile.png`; desktop/mobile просмотрены.

### M3b: CLMM по DEC-19

DEC-19 реализован в ядре: точный Q96 tick math в bounds±32768, позиции,
пересечения, Q128 fee growth, burn/collect, surplus и явный partial input.
Пинованные ожидания совпали: 1000 X →996 Q; burn5984 X/3991 Q, collect2 X,
остаток2 X/1 Q. Три диапазона дают15755 Q за16000 X с crossings−100/−200.

15 CLMM tests проверяют обе стороны, exact boundary, gap при L=0,
fees до crossing, отсутствие старых fees у нового LP, частичный burn,
collect frequency, minima/auth/no-output, capacity1024/2048, U256/domain,
readonly/stale quote, checkpoint каждой границы и rollback. Sweep192 обменов
с fees0/30/300/9999 bps сохраняет tokens и обеспечение claims.
Injected ModelError отменяет crossings/growth вместе с блоком. Вся Rust suite:
117 passed, fmt/Clippy прошли. UI/CLI путь обоих demos проверен.

SC-02: mint120 units, swap121, поздний отказ131, burn+collect135,
empty pool отказ115, exhaustion120. Crossing swap123 units. Обратный partial
swap тратит20834 из50000 Q, возвращает29166 Q и даёт20934 X; использует128 units.
После collect/burn трёх LP pool surplus=7 X/9 Q. Fee allocation после обоих
направлений: Alice15 X/30 Q, Bob30 X/29 Q, Carol2 X/2 Q. Эти amounts —
переводы из накопленных fees, не общая оценка доходности.

UI показывает distribution L/active position, principal отдельно от fees,
claims/reserves/surplus и steps/crossings. Read-only форма проверяет actual
liquidity, limit и allow_partial через тот же executor; состояние после запросов
не меняется. Полная M3b не объявляется принятой до независимого review.

Артефакты: `output/m3b-final/` —13 demos и оба byte-identical CLMM resumes;
build `08d8c843ebf03006dafc5af844d01230ae246b3b7a848068a3d137ec2b55b59d`.
Browser прошёл все13 presets, включая оба CLMM, quote/partial/rollback/replay.
Снимки `output/m3-clmm-position.png`, `output/m3-clmm-crossing.png`,
`output/m3-clmm-mobile.png`; desktop/mobile просмотрены.

### M3c: общий маршрут по DEC-20

ExecuteRoute вызывает реальные AMM/CLMM/IOC операции, передавая только их
фактический output. Typed result хранит budget/spent/unspent/output и residuals
по активам; динамические children учитываются в node/depth limits. Требование
полного расхода относится и к lot dust, и к price-improvement refund. Native
fee отделена от результата каждой ноги, fee recipient=sender учтён по ledger.

SC-03 heterogeneous: 35 X → 198 Q → 165 R, остаток 15 X; 10000 X через CLMM
тратит 5029 X → 4987 Q → 3327 R, остаток 4971 X. Первый strict swap откатывается;
поздний отказ откатывает обе исполненные ноги. Третья демонстрация проходит
book/CPMM/StableSwap. Стоимость: book route 166 units, strict отказ 146, late
отказ 176; CLMM route 156, strict отказ 136; три семейства 195.

Поиск на исходном состоянии даёт четыре полных кандидата с явно выбранными
book/CLMM limits; лучший выход 194 R через book/deep с residual 15 X. UI и CLI
сортируют output/resource/path, показывают отказы и `truncated`, не оценивают
стоимость остаточного портфеля. Старый AMM search сохранён для профиля M2.

11 tests покрывают pinned fixtures, price improvement, self fee recipient,
preexisting inventory, все resource boundaries, late/minimum/flash rollback,
generated-child limits, stale quote, explicit directional limits, 64 simulations,
admission failures, tie-break по фактическому ресурсу и checkpoint всех границ.

Артефакты: `output/m3c-final/` — 14 demos, byte-identical route resume и CLI
route quote. Build `864851b44dc6e875241345bf00e203bf41259eda21253495bb9c88580d5ee760`.
Browser прошёл все 14 presets, route search/strict/partial/readonly/replay/mobile.
Снимки `output/m3-route-search.png`, `output/m3-route-partial.png`,
`output/m3-route-mobile.png`; desktop/mobile просмотрены.
LP/MM-политики и heterogeneous arbitrage integration добавлены в M3d ниже.
Независимый review DEC-20 выполнен после M3d, см. отдельный отчёт.

### M3d: LP/MM-политики и heterogeneous arbitrage

LP получает собственные позиции и public market state. Alice вносит L=1000000
в [-100,100) за 4988 X/4988 Q и далее Hold, включая выход цены из диапазона.
Bob вносит такую же позицию, в блоке 3 выполняет collect/burn/mint диапазона
[-200,-50), L=500000, а в блоке 5 — collect/burn и Exit. После блока 2 активная
L=0, после перестановки 500000, после обратного swap 1500000; после выхода Bob
остаётся позиция Alice. Стоимость mint 120, reposition 150, exit 130 units.

MM получает собственные orders и опубликованный feed. Oracle 10→12 Q/X в блоке 2
ещё не меняет решение по блоку 1; в блоке 3 MM отменяет bid45/ask55 и ставит
bid55/ask65. Escrow 180 Q/20 X → 220 Q/20 X. В блоке 4 пользователь покупает
один lot, Carol исполняет full-input цикл 10 X → 108 Q → 11 X; стоимость
171 units при явно нулевой цене ресурса. После частичных fills MM восстанавливает
размер quotes. Возраст feed 2 s допускается, 3 s при max_age=2 вызывает только
отмену. Стоимость первой постановки 135, cancel/reprice 165, stale cancel 130.

Heterogeneous arbitrage делает не более 64 полных simulations вместе с финальным
AssertTransactionNetGain. Отдельный fixture с native price=1 даёт 270 X за 10 X,
минус 171 X fees = 89 X net; при fee recipient=sender net=260 X. Lot dust
отклоняется даже при положительном выходе. ModelError пробного исполнения
пробрасывается отдельно от admission rejection и не скрывается как котировка.

16 tests покрывают schedule/ownership/limits, отсутствие скрытого retry,
scripted nonce conflict, zero-L collect, обеспечение обеих сторон MM, fixed
anchor/cadence, rational floor, stale/missing oracle, отсутствие средств на cancel,
GTC taker fee, задержку реакции, closed-cycle fee/self-recipient, ограниченное
наблюдение, неизменность checkpoint/RNG при simulations и replay всех границ.

Артефакты: `output/m3d-final/` — 16 demos и три byte-identical resumes
(LP policies, MM/oracle, heterogeneous route). Build
`1d0c329597116159f16cff02250ad3ea68dfc53076505eba601808f8db072278`.
Browser прошёл все 16 presets, планы LP, решения Hold/Skip/Submit, anchor/age,
арбитражный цикл, stale cancel и checkpoint каждого нового demo. Снимки:
`output/m3-lp-policies.png`, `output/m3-mm-decisions.png`, `output/m3-mm-mobile.png`;
MM desktop/mobile просмотрены. Full scheduler, pending visibility, bundles,
greedy builder, hooks, остальные агенты и MTM остаются M4–M5.

Независимый reviewer проверил снимок `1bc17ee`, diff `3827f63..1bc17ee`, без
подтверждённых P0–P3 findings. Его отдельные тесты теперь включены в обычную suite
как `tests/independent_dec20.rs`: 3384 sell и 1120 buy cases, промежуточные residuals
15 X + 111 Q, prefix fill/SelfTrade rollback, LP/MM границы ровно 16 собственных
объектов с сохранением чужого имущества. Большие topology/performance и полный
U256-domain этим sweep не доказаны; число DFS prefixes отдельно не ограничено
64 полными пробами. Это сохраняется как ограничение G-16, не скрытая гарантия latency.

После переноса независимых тестов пересобран release:
`ade3682708627f5fc4807cb28e860445c00646a446163d8abf186eab02a8bb6a`.
В `output/m3d-reviewed/` повторены три CLI exports/resumes (LP, MM/oracle,
heterogeneous route): каждый `run.json` побайтно совпал. Исправлено отслеживание
новых файлов в build fingerprint: добавление файла меняет hash, удаление
восстанавливает исходный; проверка сохранена в `fingerprint-check.json`.

### M4a / DEC-21 — scheduler, mempool и базовый builder

`event-scheduler-v1`, `bounded-mempool-v1`, `basic-builders-v1` и опциональный
`on-visible-program-v1` реализованы отдельно от legacy scheduler M1. Сценарий
явно задаёт rounds, network, emissions, reactive rules и все caps. Cutoff
включительный; очередь упорядочена `(max(due_at,parent_timestamp), event_id)`.
Отложенные события сохраняют due_at; trace показывает фактическое время и lateness.

Public доставляется подписанным агентам, Private — только recipients; builder
имеет свою задержку. `AgentObservation` содержит parent market/own property,
доставленные сообщения и own outbox; статусы включения/отказа доступны после
commit. `ReactivePolicy::decide` не получает Scenario/World. Queued reaction
хранит разрешённый delivery trigger, но не копию скрытого mempool; visible payloads
вытесняются/истекают отдельно. Один policy intent на агента за раунд, bounded
reaction/depth; повторный trigger не создаёт новый ответ.

Admission отклоняет неверный envelope, deadline, stale nonce и невместимый limit;
будущий nonce и нехватка fee budget ожидают. Нет reserve, fee или nonce до
включения. Дубликат не обновляет arrival; конфликт payload сохраняет первую
запись. FIFO, fee-per-unit priority и Fixed выбирают сейчас включаемый кандидат,
затем пересчитывают eligibility. Declared limit должен помещаться, actual used
уменьшает ресурс блока. Protocol revert включается, ModelError отменяет весь
рабочий раунд вместе с queue/cursor/memory. `begin_block` и executor общие с M1;
системное начисление выполняется ровно один раз на канонический блок.

| Демо | Проверенный результат |
| --- | --- |
| Public: send7, visible8, reaction9, arrival10 | В блоке1 victim200 Q→166 X; ответ100 Q→90 X→116 Q, net16 Q, units120+160=280 |
| Private без searcher | Один receipt; нет реакции, капитал searcher неизменен |
| Response delay2 | Arrival11 остаётся в queue после блока1; ответ включается в блок2, timestamp20, deadline39 |
| Nonce1 arrival1, nonce0 arrival2 | Порядок0,1; sender740 Q, recipient30 Q, builder230 Q |
| Два кандидата nonce0 / FIFO | Ранний price1; sender875 Q, fee115 Q, конкурент stale без fee |
| Те же кандидаты / FeePriority | Price2; sender760 Q, fee230 Q, конкурент stale без fee |

18 integration tests и 2 новых unit tests покрывают численные fixtures,
нулевые delays, private recipients, funding wakeup, declared-limit carry,
TTL, dedup/conflicting IDs, Fixed, queue/known/mempool/visible/event/reaction/
intent/depth/attempt/check caps, outbox status boundary, busy periodic policies,
единственное accrual, checkpoint/tamper/read-only/fork и ModelError после
реального исполнения/при EventId overflow. Все прежние тесты также прошли.

CLI исполнил все 22 demos; шесть M4 exports/resumes после блока1 совпали побайтно
по `run.json`, `events.jsonl`, `delivery.jsonl`, `transactions.csv`.
Артефакты: `output/m4a-final/`. Release fingerprint:
`c48aac062243de0279c1045ea287b170f4fa5d703898cbd1080419015d972c9d`.
Browser прошёл все 22 presets, просмотр прошлого состояния без продвижения
очереди, checkpoint каждого M4 demo и mobile без overflow/JS errors. Снимки:
`output/m4-public-delivery.png`, `output/m4-delayed-queue.png`,
`output/m4-delayed-mobile.png`; delayed desktop/mobile просмотрены.

Результат — M4a, не весь M4. Bundles/greedy, поисковые backrun/liquidator policies,
hooks и дальнейшие роли не реализованы. Исчерпывающая performance-приёмка G-16
не выполнена: caps определяют результат, но не обещают latency на максимальных
размерах payload/очереди/истории.

Независимый review DEC-21 завершён для `215ffcf`, diff `d66959c..215ffcf`, без
подтверждённых actionable findings. Reviewer сам исполнил полную suite, шесть
CLI/browser demos, проверки API ModelError/invalid profile и 10 собственных tests.
Они сохранены как `tests/independent_dec21.rs`: отдельный integer ledger/oracle
сравнил 360 расписаний (8640 emissions, 2160 раундов) для трёх builders;
42 случая проверили exhaustion при limit100..120 и fee recipient=sender.
Дополнительно проверены event tie/clamping, terminal rejection при повторной
доставке, private own outbox, reaction после истечения visible message, actual-time
deadline, authorization TransferFrom и полный rollback после обработки событий.
Тесты повторены в основном checkout. Отчёт и границы доказательства —
`docs/reviews/dec21-independent-review.md`. Performance на совместных максимальных
caps, cross-platform replay и будущие поисковые/обучаемые политики не доказаны.

После переноса тестов в основную suite Clippy/fmt прошли; release пересобран:
`39132d01c8561b3c85c475b6e7b681604b4428135416c05e292eb1d740c82375`.
Шесть exports/resumes повторены в `output/m4a-reviewed/`, все четыре файла
результата каждого demo совпали побайтно. Реализация ядра/UI после review
не менялась; новый build fingerprint учитывает добавленный файл тестов.

### M4b / DEC-22 — bundles, bid и ограниченный greedy builder

`native-bundles-v1` и `bounded-greedy-v1` реализованы поверх event scheduler и
общего executor. Config `scheduler.bundles` задаёт emissions и явные caps.
CandidateId различает Transaction/Bundle; Own требует author=submitter, а
VisibleTxId разрешается при отправке только из уже доставленных messages либо
own outbox. Нет обращения к глобальному private registry. Resolved group
закрепляет payload; неверная группа не оставляет частично зарегистрированные
members. Ссылка может использовать разрешённый payload ранее доставленной
группы; отдельных скрытых member deliveries и OnVisible triggers не создаётся.

Native registry общий, но standalone delivery и bundle membership различаются.
Отклонённая проба не удаляет общие members других групп. Выбранный TxId
исполняется единожды; поздняя standalone delivery сохраняет исходный Included
receipt. Terminal standalone rejection не обходится группой. Active own bundle
учитывается busy guard; после закрытия группы guard освобождается.

Bid>0 обеспечен последним обычным Transfer в fee asset на fee account, с
управляемым плательщиком, отличным от builder. Payment не может быть разрешённым
revert. Probe последовательно проверяет каждый declared limit, nonce, deadline
и fee budget; списывает actual used. Allowed ProtocolRevert/ResourceExhausted
сохраняет собственные fees/nonce и ранние успешные members. Невыбранная проба
полностью остаётся диагностикой; ModelError отменяет весь рабочий раунд.
Canonical prefix начинается с общих системных переходов один раз. Выбранный
проверенный snapshot фиксируется без повторного исполнения/fee settlement.

Greedy score = реально выплаченные native fees от других senders + designated
bid; self-fee и обычное поступление не повышают score. Builder balance delta
отдельно; отрицательный delta не выбирается. Tie-break: меньше actual used,
затем FIFO. Полного поиска/оптимальности нет. FIFO/Fixed/FixedItems/FeePriority
поддерживают те же группы; fee-key группы — minimum member price, не bid.
Полная длина группы резервируется перед probe; фактически выполненные attempts
расходуют общий бюджет, неиспользованная часть освобождается. Truncated и
непроверенные candidates видны в результатах.

| Fixture | Проверенный результат |
| --- | --- |
| Transfer20, Transfer10+allowed false assert, bid50; limits200/prices1 | Success/Revert/Success, units115/125/115; Alice575 Q, Bob20 Q, builder405 Q, nonce3 |
| Тот же late revert запрещён | Нет канонических receipts/fees/nonce; прежние балансы |
| Remaining439/440 при actual used355 | 439 не вмещает последний declared limit; 440 достаточно |
| Ordinary fee345 против group fees355+bid50; block500 | Greedy выбирает group405; FeePriority — ordinary345; второй candidate далее не вмещается |
| Visible victim + response + bid5, native price0 | Victim исполняется один раз; Alice10011 Q, builder5 Q; private victim не даёт разрешённой ссылки |
| Ordinary Transfer100 builder с native fee115 | Builder delta215, score115; перевод не превращается в bid |
| Fee recipient=sender, пустая программа price3 | Receipt fee300, builder delta0 и score0 |
| 65 обычных candidates | Ровно64 probes, поздний более дорогой candidate не исследован; truncated=true |
| 17 групп по16 общих members | Ровно256 member attempts, probe work29440 units; канонически одна группа, fees1840 Q, Bob16 Q |

26 integration tests и новый unit fault injection покрывают эти числа, точный
cap16/17, реальные пределы64/256, reserved-ID/auth/bid/byte caps, общий producer
namespace, terminal/overlap/conflicting payloads, target/TTL/deadline/cutoff,
private/future references, общий visible cap, отсутствие скрытых member
triggers, busy guard, self-fee, отрицательный builder delta, checkpoint и
ModelError внутри probe после уже успешного канонического префикса рабочего
раунда. User amount overflow остаётся ProtocolRevert, не ModelError.
Все 211 Rust tests и Clippy/fmt прошли.

CLI исполнил 28 demos; шесть новых exports/resumes после блока1 побайтно совпали
по `run.json`, `events.jsonl`, `delivery.jsonl`, `builder-probes.jsonl` и CSV.
Артефакты: `output/m4b-final/`. Release fingerprint:
`eb19a36ad83b8b72c4436ba0601055b9033d3807867838197a7740c8ee010886`.
Browser прошёл все 28 presets, read-only inspection, restore и mobile без
JS errors/overflow. Снимки `output/m4b-greedy.png`, `output/m4b-forbidden.png`,
`output/m4b-forbidden-mobile.png`; greedy desktop и forbidden mobile просмотрены.
UI не называет пробные комиссии каноническими, не показывает нулевой score
вместо отсутствующей оценки budget-limited/незавершённой пробы.

Независимый review DEC-22 завершён без подтверждённых findings на `7fc0007`;
211 штатных tests, 12 независимых tests/640 differential cases и шесть
CLI export/resume проверены reviewer. Ограничения —
`docs/reviews/dec22-independent-review.md`; live browser reviewer не запускал.
На границе M4b поисковые backrun/liquidator
политики, hooks, прочие роли, MTM и M5 оставались впереди. Caps ограничивают
модельную работу, но performance на совместных максимальных payload/history
не доказана; G-16 не закрыт. Сценарный bundle сам по себе не является
реализацией поискового searcher.

### M4c / DEC-23 — поиск backrun по разрешённой victim

Профиль `bounded-backrun-v1` и конфигурация явно выбираются вместе с event
scheduler и bundles. Один агент использует одну policy. Новый trigger появляется
только после разрешённой native delivery от другого агента/неуправляемого sender.
Reaction delay, actual event time, depth/reaction/intent caps, busy nonce и
trigger dedup соблюдаются. Вытесненное к моменту реакции сообщение даёт
TriggerUnavailable; общий registry не становится источником скрытого payload.

Контроллер создаёт временный parent+trigger через общий begin_block/executor.
Policy получает собственный preview и публичные venues, без полного World,
чужих счетов, pending mempool и будущих emissions. После успешного prefix
каждый кандидат пробуется с одной и той же копии. Размеры сортируются, DFS идёт
по VenueId; до8 inputs,3 hops,64 route trials и4096 venue checks за раунд.
Member attempts ограничены256, полный response/payment tail резервируется
до пробы. Бюджеты общие между searchers и отдельны от builder probes. При
исчерпании общего бюджета новый prefix тоже не исполняется.

ExecuteRoute RequireFull и финальный AssertTransactionNetGain учитывают
фактическую fee ответа, bid и native fee последнего payment. Guard остаётся
верным после изменения builder prefix. Baseline conditional net берётся после
victim: её собственный перевод searcher не объявляется прибылью маршрута.
Успешный кандидат выбирается по net, затем own used/input/path. Отправляется
одна группа на текущий target block; опоздание не выпускает members отдельно.

| Демо | Проверенный результат |
| --- | --- |
| public: inputs25/50/100/150/200, bid5, prices0 | 10 route candidates; выбран100 Q→90 X→116 Q; net11; Alice10011, builder5; units120/160/115 |
| private без searcher | Нет реакций/поиска/groups, только victim; Alice10000 |
| delayed: response outgoing delay2 | Найденный ответ прибывает11 после cutoff10, target1 истекает; Alice10000, own fees0 |
| FIFO на том же orderflow | Ordinary victim включается раньше общей группы; поиск прогнозировал11, но canonical gain0 и own fees0 |
| competition: bids5/7 | Два поиска по10 trials; Greedy выбирает7, Carol10009, Alice10000, builder7, victim ровно один раз |
| fees: input100000, own price1 | 100000 Q→90909 X→118032 Q; fees160+115, bid5; net17752, Alice1017752, builder280 |

27 integration tests и fault injection проверяют сетку/сортировку, fees/bid0,
положительный guard, payment resource/fee reserve, prefix nonce/funding/failure,
limits619/620, caps и release reservation, shared budgets, private recipients,
old visible eviction, own-sender filter, BusyNonce и duplicate trigger.
Проверены изменённый builder prefix с поздним guard revert, отдельный transfer
victim, StableSwap metered solver fees, отказ CLMM/book partial results,
checkpoint с queued reaction и tamper с пересчитанным checksum.
ModelError внутри prefix, response и payment оставляет исходный World/scheduler
без изменений. Всего251 Rust tests; после последнего уточнения pre-prefix cap
и проверки payment fee reserve повторно прошли27 backrun tests и Clippy.

CLI выполнил все34 demos. Для шести новых сценариев после checkpoint блока1
побайтно совпали `run.json`, `events.jsonl`, `delivery.jsonl`,
`builder-probes.jsonl`, новый `search-probes.jsonl` и `transactions.csv`.
Артефакты: `output/m4c-validated/`. Build fingerprint:
`7595e6132809d0ac95cb61f5611486915b65303760ed8b7f5ec5b967ff153001`.
UI отображает доступный trigger, conditional net, input/path, search work,
отдельный статус группы и actual receipts. Все34 browser presets прошли:
read-only inspection/checkpoint/restore, desktop/mobile, JS errors0 и overflow0.
Браузерная проверка выявила длинные TxIds в flex-строках transactions;
исправлен перенос текста, затем весь UI suite повторно прошёл. Снимки
`output/m4c-competition.png`, `output/m4c-fees.png`,
`output/m4c-fees-mobile.png`; competition desktop и fees mobile просмотрены.

Независимый review DEC-23 завершён: два P2 исправлены и перепроверены.
Nonce exhaustion даёт Skip NonceExhausted, overflow lifetime — Skip
DeadlineOverflow, непредставимое due_at реакции — BackrunReactionTimeOverflow
без enqueue. Точные границы MAX сохраняются. 16 независимых tests покрывают768
integer cases,96 cap variations, private/future noninterference и replay;
дополнительный тест —10 nonce boundaries. Полный review suite на `f99114a`:
271 passed. Ограничения — `docs/reviews/dec23-independent-review.md`.
Flash-funded search, liquidator
competition, прочие роли, hooks и M5 ещё не реализованы этим срезом. Ограничение
числа проб не доказывает performance при максимальных payload/history; G-16
остаётся открыт. Условный лучший ответ не обещает включения или глобального
оптимума MEV и не заменяет будущие MTM/experiment metrics.

### M4d / DEC-24 — liquidator, фактический залог и конкуренция

`liquidate-and-sell-v1` выполняет common Liquidate и RequireFull route. Бюджет
продажи — прирост collateral непосредственно от liquidation; существующий
inventory не расходуется. Receipt сохраняет requested/paid/seized, фактический
route и oracle use. Поздний guard, settlement shortage и partial fill отменяют
liquidation, sale и write-off вместе. Включённая попытка сохраняет native fee/nonce;
канонические typed results отменённой операции пусты, диагностическая trace сохранена.

`bounded-liquidator-v1` выбирает разрешённый target, repayment и путь по одному
accrual preview из parent state. Наблюдение содержит только собственные balances,
публичные venues/feeds, перечисленные позиции и выбранный flash provider.
Текущие/future emissions и private messages в него не входят. Policy использует
Own либо Flash funding и final transaction net guard; max_repay flash principal
не становится доходом. До16 targets,8 sizes,3 hops;64 trials и4096 venue checks
всех liquidators за раунд. Порядок AgentId/market/borrower/size/venue детерминирован;
best completed может отправляться при truncated, неисследованные варианты не
объявляются убыточными. BusyNonce и проверенные nonce/deadline/delivery overflow
дают явный Skip. Отправка использует обычный native scheduler, без bundle bid.

Числа `docs/reviews/liquidator-reference.json` подтверждены Rust и CLI/browser:

| Demo | Фактический результат |
| --- | --- |
| Own | Request60, paid60 Q, seized6 X, sale84 Q; net24 Q, write-off20 Q; C/D/S/E=80/0/80/0 |
| Flash ×1000, price1 | Principal60000 Q, flash fee30 Q, sale84905 Q; native200 units/200 Q, net24675 Q; Carol25675 Q |
| FIFO competition | Alice124735 Q; Carol99760 Q, NoDebt и fee240 Q; builder410 Q |
| FeePriority competition | Alice99880 Q, NoDebt и fee120 Q; Carol124565 Q; builder460 Q |
| Stale oracle | Первоначальный заём создан по свежей цене; на preview30 s новая цена имеет age10/max9, Skip, долг80 Q остаётся |
| Delayed | Отправка20, arrival31; блок3 без receipt, блок4 BusyNonce и одно включение; Carol1024 Q |

Winner использует170 units, NoDebt loser120. Оба включённых конкурента расходуют
nonce; write-off выполняется один раз. Flash использует200 units, включая10 units
существующего FlashCallback; численная ошибка исходного reference исправлена до
policy implementation в `f16bb8a` и явно описана в §23.2.12.

Полная проверка: **305 Rust tests**, fmt и Clippy `-D warnings`, wire test и JS syntax
прошли. Новые34 tests:12 native liquidation_sale,20 liquidator_policy и2 unit
наблюдения/fault injection. Покрыты actual inventory/prior partial close,
Own actual paid vs request, native reserve/flash cash shortage, settlement/min_out
rollback, stable solver work, CLMM/book partial rollback, authority/path/depth,
ресурсные границы169/170 и199/200, HF equality/missing/stale oracle, single accrual,
16/17 targets,8/9 sizes,64 trials/4096 venue checks, общие budgets/порядок,
private/future noninterference, поздняя потеря прибыли, nonce/time exhaustion,
expired intent, inflight checkpoint и tampered report, ModelError после sale/write-off.

UI показывает config/funding, parent→preview, target health/freshness, условные
кандидаты и net, выбранное погашение, включённого winner/loser и native fees.
Инспектор раскрывает actual paid/seized/sale и точную oracle record. `liquidator-probes.jsonl`
экспортирует диагностический поиск; canonical events/receipts остаются отдельными.
Все40 browser presets прошли, включая read-only viewing, checkpoint/resume и mobile;
новые desktop/mobile screenshots просмотрены, overflow и JS errors отсутствуют.
Все40 CLI demos завершены. Для6 liquidator +6 backrun сценариев checkpoint/resume
даёт побайтно одинаковые `run.json`, `events.jsonl`, `delivery.jsonl`,
`transactions.csv` и применимые builder/search/liquidator probe exports.

Release fingerprint: `e5347c96728677f327938fc4db0223f39fa031affad8c8aad4cbe92dace4c205`.
Артефакты: `output/m4d-validated/`, `output/m4d-competition.png`,
`output/m4d-flash-mobile.png`; логи `/tmp/chainarium-m4d-{full-tests,clippy,browser,cli}.log`.
Независимый review DEC-24 diff `72590bb..976a5cb` завершён без подтверждённых
findings: `docs/reviews/dec24-independent-review.md`. Семь reviewer tests сохранены
в `tests/independent_dec24.rs` и адресно прошли в основном checkout. Они добавляют
768 независимых integer cases,344 resource boundaries, точные1024 nodes/depth32,
Fixed/Greedy с оплачиваемым проигравшим, U256 request/minimum, multi-target/path
tie-breaks и checkpoint каждого из6 demos на каждой committed boundary.
Reviewer независимо выполнил весь baseline305 suite. Новая численная модель —
liquidation+CPMM; StableSwap/CLMM/book опираются на чтение и существующие tests.
UI/CLI reviewer проверил статически; browser40 acceptance выше выполнена автором.
Это M4d, не вся M4 или v1.0: hooks, остальные роли, MTM/эксперименты/explorer и
performance/user-task gates M5 остаются открытыми.

### M4e / DEC-25 — CPMM hooks с ограниченными capabilities

`bounded-pool-hooks-v1` задаёт до64 связей hook↔CPMM, отдельный защищённый account
и counter. В Before/AfterSwap и Before/AfterLiquidity разрешено до16 typed
instructions: fee по input, максимальный input, явный отказ, increment counter,
выплата исходному sender из собственных средств. Проверяются capabilities,
точка вызова, сумма выплат на точку и уникальность fee rule по направлению.
Hook получает ограниченный context своего пула/состояния/вызова; доступа
к чужим кошелькам, private pending, произвольным вызовам и reentry нет.

Вход и каждая начатая инструкция стоят10 units, перевод5. Hook frames входят
в общие1024 nodes/depth32, включая generated children route/flash/liquidation.
Поздний отказ, нехватка hook cash, exhaustion, flash settlement и net guard
отменяют counter, выплаты, swap/LP и composed effects вместе. Native fee/nonce
включённой неудачи сохраняются; ModelError отменяет весь блок.

Route использует реальный output swap отдельно от hook payout: выплата в input
asset не становится refund или отрицательным consumption; выплата в output asset
не увеличивает следующий budget. Heterogeneous arbitrage оценивает полный
собственный net после native fee, включая обеспеченную награду. При hooks
старый аналитический arbitrage требует перехода на общий execution search.
Pure CPMM quote явно направляет пользователя к execution quote с sender и ресурсом.

Числа `docs/reviews/hooks-reference.py`/`.json`, сохранённые до реализации,
повторно совпали побайтно. Свежий fixture: pool1000 X/1000 Q, Alice1000 X/1000 Q,
hook20 Q, counter0, native price1; threshold100, fee0/1000 bps, max_input200,
два increments и payout3 Q:

| Случай | Фактический результат |
| --- | --- |
| Swap50 | Output47 Q,195 units; Alice950 X/855 Q; hook17 Q,counter2 |
| Swap100 | Output82 Q,195 units; Alice900 X/890 Q; hook17 Q,counter2 |
| Контроль без hook | Output90 Q,120 units; Alice900 X/970 Q |
| Input201 / min_out83 | Отказ140 /150 units; counter/cash/pool исходные |
| Поздний assert / cash2 / limit194 | Отказ205 /195 /194 units; все hook/swap effects отменены |
| LP add100 / remove100 | По175 units; Alice728→656 Q; counter2→4; hook17→14 Q |
| Route X→Q→R | Output82 Q→75 R,225 units; payout3 Q остаётся отдельно |

Новые21 integration tests и unit fault injection проверяют reference,
192 resource/price boundaries, 16/17 instructions, 64/65 bindings, MAX counter,
fee0/9999, empty programs/profile, authority/capability/spend defense, точные
nodes/depth, повторные вызовы, SwapOutput/Flash, route payouts в обоих assets,
greedy probes, backrun и liquidator search с обеспеченной наградой и late rollback.
Проверены ограниченное observation, read-only quote с неизменными RNG/checkpoint,
все шесть checkpoint paths и tampered state. Полный suite334 прошёл; после
адресного уточнения пустого набора bindings повторно прошли21 hook tests,
Clippy и release build. Других изменений runtime после полного прогона нет.

UI показывает config/capabilities, counter/cash, receipt status/native fee,
Before/After frames, effective fee, restricted context и отменённые effects.
`hook-attempts.jsonl` отделяет диагностические frames с committed-флагом от
канонических events. Все46 browser presets прошли, включая hook execution quote,
read-only просмотр, checkpoint/resume и mobile. JS errors/overflow отсутствуют;
`output/m4e-hook-fee.png` и `output/m4e-hook-rollback-mobile.png` просмотрены.
Browser проверял build `4bd7b91e71395b583a6febb2b88b1bb2269f0dd5699752983d5eb04d2d5da3d3`;
последующее уточнение validation пустых bindings не меняло UI или выполнение hooks.

Финальный release fingerprint:
`5b82feb8bda61eabf89f8623b1642ee8d538374b12d27e5d4ce3639dbb2ab4c3`.
Все46 CLI demos исполнены этой сборкой; для6 hook demos checkpoint после блока1
дал побайтно одинаковые run/events/hook-attempts/transactions exports.
Артефакты: `output/m4e-validated-final/`; логи полного прогона и браузера
`/tmp/chainarium-m4e-{full-tests,browser}.log`, финальных адресных проверок
`/tmp/chainarium-m4e-{hooks,clippy,release}-final.log`.
Независимый review DEC-25 diff `145ea8a..d4a4709` завершён без подтверждённых
P0–P3 findings: `docs/reviews/dec25-independent-review.md`. Reviewer независимо
выполнил baseline334 suite и14 собственных tests:3545 прямых block executions,
101 execution quotes,8 arbitrage runs,4 Own/Flash liquidator runs (16 candidates),
6 backrun+Greedy runs (60 candidates),6 demo export/replay checks и fault injection
после второй успешной hook tx. Категории не складываются в общее число executions:
policy runs включают свои probes, а restore — replay.
Тесты сохранены в `tests/independent_dec25.rs` и test-only
`src/engine/independent_dec25_fault.rs`; при переносе изменены только форматирование,
имя test module и вводный comment. Все14 адресно прошли в основном checkout,
fmt/Clippy также прошли; runtime не менялся. Логи переноса:
`/tmp/chainarium-dec25-retained-{tests,fault,clippy}.log`.
Review не является exhaustive/formal proof; deepest LiquidateAndSell+flash+hook
tree и все смешанные non-CPMM пути не перебраны заново независимой matrix.
Browser/HTTP walkthrough reviewer не запускал; browser46 acceptance выше выполнена
автором. Hooks ограничены CPMM; остальные
роли M4, MTM/эксперименты/explorer и performance/user-task gates M5 остаются открытыми.

### M4f / DEC-26 — пользовательские, borrower и publisher политики

`baseline-agents-v1` работает через event scheduler: одна policy на AgentId
среди всех семейств, до одного intent и одного preflight на round. Общая
системная проекция parent→cutoff используется также liquidator; начисления
не повторяются канонически при probes. Policy получает собственные balances,
разрешённые public venues/feeds и только собственную выбранную credit position.
ModelError после draws, preflight либо canonical build возвращает RNG, очередь
и World к прежней committed-границе. Успешный round сохраняет выбор, даже если
intent пропущен, отвергнут при доставке или включён с protocol revert.

ScriptedUser выполняет точный event без скрытого переноса. RandomUser использует
ChaCha12 `agent/<id>` и две равномерные выборки с максимум32 draws на каждую;
gate и template выбираются даже при probability0/1000000 и BusyNonce. Rejection
sampling использует расширенную границу2^64, не modulo fallback. BorrowerTarget
выбирает один шаг Repay→DepositCollateral→Borrow→WithdrawCollateral по текущей
цели и accrued preview, не уменьшает сумму из-за cash и не включает collateral
автоматически. NativeOraclePublisher отправляет обычные Spot/Twap calls:
цена Spot определяется при inclusion, TWAP end остаётся из исходного решения.
NotReady любой publication отменяет весь preflight; sequence overflow даёт Skip.

Независимый pre-implementation reference DEC-26 повторно совпал побайтно;
12 scalar ChaCha12 draws совпали с действующим DEC-10 Streams до написания policy.
Шесть запускаемых сценариев проверены Rust, CLI и браузером:

| Демо | Проверенный результат |
| --- | --- |
| RandomUser, seed42 | Переводы20 Q в rounds1/4; sender730, recipient40, builder230, nonce2; stream position24 |
| Scripted AnyReceipt | Late revert125 units, перевод10 Q отменён; затем перевод20 Q за115; финал sender740, recipient20, builder240 |
| Scripted SuccessfulOnly | Та же неуспешная проба даёт Skip без fee/nonce; следующий перевод20 Q за115, sender865 |
| Borrower | Deposit6 X→Borrow80 Q→Hold→Repay80 Q→Withdraw6 X→Hold; fees460 Q, финал540 Q/6 X, C/D/S/E=100/0/100/0 |
| NativeSpot | Публикации20→5 Q/X после swap10 X→100 Q; по110 units, publisher780 Q |
| NativeTwap W2 s | Round1 NotReady/Skip без платы; round2 price20, sequence1, observed_at=published_at=2,111 units, publisher889 Q |

Borrower demo создаёт reference cash100/claims100 обычным Supply вкладчика в
первом блоке; первая операция borrower лишь вносит залог и не требует этих cash.
Supply явно имеет price0, borrower — price1. Начальное состояние demo не выдаётся
за genesis с бесплатно созданными deposit claims. Наблюдаемые borrower шаги и
послеблочные C/D/S/E точно совпадают с независимой таблицей reference.

Новые19 tests:16 integration и3 unit. Проверены точные draws/balances/costs,
probability0/1000000, cadence,1/16/17 templates и feeds,1024/1025 schedules,
порядок/дубликаты событий, роли и protected accounts, BusyNonce, задержки/expiry,
nonce/deadline/delivery/sequence/stream exhaustion,32 raw rejects,2^64 sampling
boundary, отсутствие private/future/чужих balances в Observation. Проверены
Repay после единственного accrual, LTV equality/stale/cash failures без
переписывания targets, partial preflight publications и actual spot после
доставки, фиксированный delayed TWAP end, paid revert после успешного preflight.
Проверены все committed checkpoint boundaries новых demos и rehashed RNG tamper.
Fault injection после preflight и canonical execution сохраняет старые World,
scheduler и RNG; следующий честный запуск использует исходную выборку.

Полный suite **367 passed**, fmt и Clippy all-targets `-D warnings` прошли.
Node exact-wire test и JS syntax checks прошли. Release fingerprint:
`7c210b13a33d3767fc817f799b54f90334161c98759dd13dfe2b2e9da72f2282`.
Все52 CLI demos исполнены. Для6 новых demos checkpoint после блока1 даёт
побайтно одинаковые `run.json`, `events.jsonl`, `baseline-decisions.jsonl`,
`delivery.jsonl` и `transactions.csv`; выборка RNG не повторяется при restore.
Артефакты: `output/m4f-validated/`; логи
`/tmp/chainarium-m4f-{full-tests,clippy,release,cli,browser}.log`.

UI показывает config, активную цель, sampled gate/template и позицию stream,
parent/preview, диагностический receipt и отдельно фактически включённые fees.
Все52 browser presets прошли, включая чтение старых блоков без RNG draw,
checkpoint/resume и mobile. JS errors/overflow отсутствуют. Снимки
`output/m4f-random.png` и `output/m4f-borrower-mobile.png` просмотрены.
Независимый [review DEC-26](reviews/dec26-independent-review.md) на
`f65c8b3..fc364d9` завершён: P1 — foreign wallet через preflight;
P2 — потеря refusal регистрации в Decision/UI. Исходный отчёт сохранён без
перезаписи результата. Перенесены8 integration tests (375 RNG сценариев,
3000 пар выборок,35 borrower targets, publishers/privacy/replay) и2 unit tests.

Исправление P1 рекурсивно ограничивает AssertBalance/AssertBalanceDelta,
TransferFrom и credit calls собственным account/borrower. Проверка выполняется
при validation и повторно перед probe; изменение ранее validated шаблона не
даёт trial/fee/nonce. Обычные native calls сохраняют свой контракт.
Исправление P2 сохраняет typed submission outcome: Queued либо Rejected с
причиной. KnownTransactionsLimit/QueueLimit/duplicate/conflict дают Skip;
attempted transaction и diagnostic trial остаются для объяснения. UI различает
отказ отправки, пробный успех и фактическое включение. Дополнительные регрессии
проверяют nesting, оба preflight режима, runtime defense и checkpoint с отказом.

Исправление закоммичено и отправлено в origin/work: `ce78187`.
После исправлений полный suite381 passed, fmt и Clippy прошли. Release
fingerprint `c22dddda5a934a87ee13322111247492b9d0a2473c851d7d510d95b9c609aa23`.
Все52 CLI demos и52 browser presets прошли;6 baseline checkpoint/resume exports
совпали побайтно. Browser отдельно проверил Skip/Rejected для KnownTransactionsLimit
и QueueLimit, успешный trial при отсутствии actual receipt, restore отказа и
HTTP rejection обоих foreign-wallet fixtures с сохранением текущего run.
Независимая [перепроверка `ce78187`](reviews/dec26-fix-verification.md)
тем же разрешённым reviewer завершена: F1/P1 и F2/P2 закрыты, новых
подтверждённых findings нет. Reviewer самостоятельно выполнил381 suite и ещё
6 integration +3 unit tests. Дополнительная матрица покрывает240 static/runtime
cases, оба preflight режима,6 запрещённых calls и5 уровней/видов nesting;
Known/Queue отказы проверены на6 rounds с replay каждой границы. Проверены
observer-only QueueLimit, поздняя expiry, typed dedup и rollback после отказа.
Missing/foreign order ID меняет diagnostic text, но не Observation/action/fee;
это ограничение researcher diagnostics явно записано в отчёте.
Логи: `/tmp/chainarium-dec26-fix-{full,target,clippy,release,cli,browser}.log`.
Экспорты: `output/dec26-fix-validated/`.
Flash-funded arbitrage реализован в M4g ниже;
MTM/experiments/full explorer и M5 performance/user-task gates остаются открытыми.

### M4g / Accepted DEC-27 — bounded flash arbitrage

В `docs/spec.md` §23.2.15 записан конкретный контракт bounded flash arbitrage:
один principal/provider, own native fee reserve, закрытые RequireFull пути,
3 hops /64 trials /4096 venue checks, общий budget семейства, actual net после
settlement/fees и явные submission outcomes. Статус **Accepted**: владелец согласовал контракт 2026-11-06 до реализации;
policy/controller, CLI export и UI реализованы. M1 scheduler профиль не принимает;
Own policy продолжает работать по ранее принятому контракту.

Независимый integer reference `docs/reviews/flash-arbitrage-reference.py`/`.json`
содержит8 случаев. До реализации policy они сопоставлены с literal programs
существующего native executor: success, exact/minimum+1, fee reserve shortage,
late guard revert и settlement shortfall. Все точные balances/reserves/provider
и used совпали. Артефакты: `output/flash-arbitrage-reference-check/`.
Это pre-implementation арифметический reference; policy/search проверены отдельно.

Новая policy получает restricted common system preview и выбранный public
provider. Simulation capability разрешает только fixed FlashLoan→ExecuteRoute→
root net guard, проверяет envelope, замкнутость, отсутствие повторных venues
и directional limits. Нет произвольного callback/foreign-wallet probe.
Кандидаты обходятся DFS в VenueId order; общий budget ограничивает и успешные,
и непродуктивные префиксы. Лучший завершённый выбирается по net, used, path;
при exhaustion truncated остаётся видимым. Отправка сохраняет Queued/Rejected;
позднее включение/revert отдельно отражается в canonical receipt.

20 integration tests покрывают точные two/three-hop расчёты, 64 trials и4096
непродуктивных venue checks, shared budget в AgentId order, tie-break по used/path,
CPMM/StableSwap/CLMM/book, price limits и RequireFull partial rollback,
hook dynamic fee/payout/counter с изолированными probes и actual rollback,
funding/nonce/time/U256 boundaries, Public/Private/busy/expiry, Known/Queue
refusal, foreign/future noninterference и rehashed report tamper.
Две unit проверки: ограничение simulation capability и ModelError после probe/
canonical flash settlement, возвращающий весь round и RNG другой baseline policy.
Дополнительно сохранены6+3 tests независимой перепроверки DEC-26.

Полный suite412 passed, fmt/Clippy all-targets `-D warnings`, Node wire и JS syntax
checks прошли. Release fingerprint
`af4edd565c07650308d75d1b4efcc6262b83d6416d4ed7d1547375b775d282aa`.
Все58 CLI demos исполнены;6 новых checkpoint/resume дали побайтно одинаковые
run/events/flash-arbitrage-probes/delivery/transactions exports. Во всех новых
demos RNG.positions остаётся пустым. Логи `/tmp/chainarium-m4g-{full,target,clippy,release,cli}.log`,
экспорты `output/m4g-final-validated/`.
Все58 browser presets прошли на финальной сборке, включая новые6 cases,
readonly просмотр, restore с genesis, реальные route cards/отменённые legs,
точные balances/fees и mobile без overflow. JS ошибок нет; лог
`/tmp/chainarium-m4g-browser.log`. Снимки `output/m4g-success.png` и
`output/m4g-shortfall-mobile.png` просмотрены.
Независимый review DEC-27 ещё не выполнен;
MTM/experiments/full explorer и M5 gates остаются открытыми.

### Подготовлен M5a / Proposed DEC-28 — reference MTM

Контракт `docs/spec.md` §23.2.16 подготовлен до реализации. Статус **Proposed**:
новая production valuation policy/evaluator/API/UI ещё не реализованы и не
согласованы владельцем. Открытый G-11 остаётся gate этого среза.

Предложены exact signed rational quantities/values и floor только для
представления, restricted reference inputs с availability/observation time,
фиксированная граница portfolio и явные capital-transfer bindings. CPMM/Stable
LP оцениваются по точной доле reserves; CLMM principal/collectable fees и
post-loss deposit claims следуют своим принятым native claim правилам.
LP mark, исполнимый exit и доступный reserve cash различены. Hidden reference
не входит в agent observations; evaluator производен от committed snapshots.

Независимый Python Fraction reference `docs/reviews/valuation-reference.py`/`.json`
содержит24 arithmetic cases: положительные/отрицательные дроби, отсутствие цены,
gross offset, flow-time prices и накопление без промежуточного floor,
borrow/interest, post-loss shares, LP/CLMM/order escrow, freshness/availability,
wide report и ArithmeticLimit. Он не импортирует simulator и не реализует
production extractor. Повторный вывод совпадает побайтно.

Два примера сопоставлены с уже существующим native executor:

- Sole Transfer900 Q из own1000 при limit200/price1 даёт actual revert115,
  own885/Bob0. Capital amount не исполнен; предполагаемый MTM PnL−115.
- В M4g success Alice владеет обоими CPMM. При reference X/Q=1 полный portfolio
  имеет4501000→4500760 Q, MTM PnL−240, хотя wallet cycle net равен+24104 Q.
  Это отдельные метрики; LP losses и flash/native fees не исчезают из MTM.

Артефакты этих сверок: `output/valuation-reference-check/`. Полный Rust suite
не повторялся: изменения только в Proposed docs/reference, production код не
менялся. Для M4g остаётся ранее запрошенное разрешение на отдельный read-only
review diff `d4d5726..5d183ef`; новый агент без него не запускался.

### Fixtures и границы доказательства

| Проверка | Доказательство | Ограничение |
| --- | --- | --- |
| Ledger/auth/execution/block | acceptance + 5 injected ModelErrors после успешной tx, включая accrual/oracle | M4b/M4c/M4d независимо проверены; ограничения указаны в отдельных отчётах |
| CPMM/LP/router/flash | composition: rounding/minima/ownership/reentry/actual output/rollback; 500 CPMM invariant cases | Native models; совместимость с внешним deployment не заявлена |
| Lending accounting | exact fixture, caps/HF/LTV/cash, aggregate rates, multiple escrow, fractional carry, no-holder income | Прямой долг, заранее зарегистрированные позиции, максимум 1024/reserve |
| Liquidation/loss | exact waterfall, partial close, dust, min_collateral, fee-preserving rollback, 256 deterministic boundary cases | Нет recovery/treasury; вывод equity запрещён |
| Deposit shares | zero mint, minima, cash shortage без burn, last exit, bootstrap с E | Bankrupt проверен на invariant-valid defensive snapshot, не заявлен достижимым из genesis этого positive-payment preset |
| Oracle | unauthorized/source/pair/future/duplicate/out-of-order; возраст 60/61; safe stale operations; consumer trace | External/native spot/TWAP; bounded history и однократное floor проверены |
| Hooks | exact pre-implementation reference, resource/node/depth/capability boundaries, composed search/payout/rollback, ModelError; независимый DEC-25 review без findings | CPMM only; область и ограничения в docs/reviews/dec25-independent-review.md |
| Flash arbitrage | independent integer reference, bounded paths/probes, heterogeneous legs/hooks, fee/rollback, restricted capability, shared budgets и replay | DEC-27 реализован; независимый review впереди |
| Baseline agents | independent RNG/ledger reference, target/publisher/preflight boundaries, paid actual failure, private/future isolation, atomic RNG, replay | DEC-26 P1/P2 исправлены и независимо закрыты на ce78187; не экономически оптимальные политики |
| Checkpoint/export | replay/resume/fork/tamper, протоколы, LP/MM schedules/decisions | M4b groups, M4c trigger/search и M4d preview/trials/inflight snapshots проверены |
| UI | реальные balances/trace, 58 presets, desktop/mobile, exact HTTP wire | Прототип; полная приёмка M5 не закрыта |

Численные ожидания следуют §6/§8/§20 и принятым DEC; тесты не заменяют
независимый review. Analytical sweep — детерминированная проверка инвариантов,
не сравнение с конкретной внешней реализацией.

## Измерение M1

Команда: `target/release/chainarium benchmark 1000`; Rust 1.99.1, Linux x86_64,
AMD EPYC 9V74, среде доступны 5 vCPU; исполнение одним worker, release,
полные traces. Нагрузка — 1000 блоков, 50 scripted senders, 4 CPMM,
50 кандидатов/блок. Все 50000 swaps завершились успешно.
Это масштабный CPMM-вариант M1, не имитация будущих торговых механизмов.

- Исполнение: 1.730 s; 28904 tx/s.
- Процесс вместе с подготовкой и сериализацией checkpoint: 2.787 s.
- Peak RSS: 545520 KiB (532.7 MiB).
- Checkpoint: 64141834 bytes.
- Build fingerprint: `d0fa16d9c5c94302fd8c7b553da80060289980dbb99ff8b595588cc363a0142d`.

Это baseline, **не утверждённые performance budgets**. G-16 остаётся открыт;
latency UI и полные workloads M5 ещё не измерены/не приняты. Материалы текущей
среды: `output/validation/`, `output/final-run/`, `output/m1-inspector.png`.

## Что остаётся

1. Независимый review в отдельном контексте по §23.4: владелец разрешил одного
   read-only агента для M1–M3b (4d5b6bb..42651ee). Проверка завершена: три findings
   исправлены в `3827f63` и адресно перепроверены reviewer; отчёт и ограничения —
   `docs/reviews/m1-m3b-independent-review.md`. DEC-20 в её область не входит.
2. DEC-20 — общий partial-fill router и LP/MM политики.
   DEC-20 принят владельцем в `docs/spec.md` §23.2.8: контракт остатков,
   bounded search и политики. Аналитические ожидания до реализации сохранены
   в `docs/reviews/heterogeneous-route-reference.py`/`.json` и совпали при
   пересчёте. M3c/M3d реализованы, функционально и независимо проверены;
   границы review — `docs/reviews/dec20-independent-review.md`.
3. M2f/M2g входят в завершённый review M1–M3b; ограничения проверки указаны
   в его отчёте. Новые механизмы M4 потребуют отдельного review.
4. M4a / DEC-21 функционально и независимо проверен. M4b / Accepted DEC-22
   реализован, численные ожидания совпали, все функциональные проверки прошли;
   отдельный read-only review diff `82e52c7..7fc0007` завершён без findings,
   его отчёт и 12 независимых тестов сохранены.
   M4c / DEC-23 (§23.2.11) реализован: поиск backrun по видимой victim,
   sizes/route caps, bundle и net после fees/bid. Независимый
   read-only review этого среза завершён; два P2 исправлены и перепроверены. Для M4d принят
   DEC-24 (§23.2.12) с численными ожиданиями; M4d реализован,
   независимый review нового среза завершён без подтверждённых findings.
   Для hooks принят DEC-25 (§23.2.13), M4e реализован и функционально проверен;
   отдельный независимый review этого среза завершён без подтверждённых findings;
   отчёт и14 дополнительных tests сохранены.
   Для user/random/borrower/oracle ролей принят Accepted DEC-26 (§23.2.14),
   независимые численные ожидания — `docs/reviews/baseline-agents-reference.py`/`.json`.
   Reference ChaCha12 совпал с12 существующими DEC-10 draws; новые политики
   реализованы в M4f; независимый review нашёл P1/P2, исправления ce78187 независимо перепроверены и закрыты.
   Accepted DEC-27 реализован в M4g; отдельный независимый review ещё нужен;
   M5: MTM, эксперименты, полный explorer и приёмка. Новые существенные решения
   требуют конкретных карточек и согласования по §23.7.
5. Полные user-task/latency/performance budgets и независимое review должны
   быть выполнены до приёмки целевой v1.0. SC-06 с конкуренцией исполняется в M4d и отдельно проверен.

По новому явному поручению владельца готовые Conventional Commits периодически
отправляются в GitHub. После обновления прав записи успешно создана
`origin/work` на `d4a4709`; remote SHA подтверждён отдельно. Среда и релизы
не публиковались. Ранее опубликованная конфигурация среды не изменялась повторно.

### M5a / DEC-28 — согласование и ядро оценки

Владелец согласовал DEC-28 ответом «Ок» 2026-11-06. Контракт теперь Accepted;
это не означает прохождения независимого review реализации.

Добавлен чистый evaluator `valuation`: reduced signed fractions, provenance
reference records, bounded extraction wallet/LP/CLMM/order/deposit/collateral/debt,
помеченные committed capital transfers и точный PnL. Export потоково сохраняет
policy, каждую committed boundary (включая genesis) и capital flows; manifest
содержит hashes policy/reference inputs. Mutable valuation state в Run отсутствует.

15 интеграционных тестов прошли: arithmetic reference, native claims, failed fees,
delayed transfer/VisibleTxId bundle, historical Unpriced, caps/foreign zero scans,
validation mutations, agent/RNG noninterference, replay и byte-identical exports.
Clippy all-targets с `-D warnings` прошёл. Общий regression suite и шесть UI demos
проверяются следующим шагом; независимый review DEC-27/DEC-28 ещё не выполнен.

Порядок bounded extraction определён реализацией: wallet, LP, CLMM, book,
lending, внутри — BTreeMap keys; затем расчёт суммы по стабильным component keys.
Помимо каждой просмотренной записи счётчик включает lookup account/protocol
container: пустые контейнеры также не обходят cap. Quantity/value каждой строки
проверяются отдельно, агрегация проходит без промежуточного floor.

### M5a / DEC-28 — сквозная реализация и проверки

Добавлены HTTP `GET /api/valuation/{block}`, MTM-панель с выбором portfolio,
компонентами, price provenance, Unpriced/reasons, capital flows, пересечениями
счетов и скачиванием policy + выбранной boundary. Деньги отображаются точными
строками/дробями; UI не пересчитывает PnL из округлённых строк. Просмотр не меняет
исполнение. Новые presets `sc-mtm-{capital,unpriced,flash,credit,clmm,book}`
добавлены к58 предыдущим: всего64 CLI/UI demos.

Проверка дополнительного edge case выявила неоднозначный binding, когда TxId
literal emission повторно определён как Own bundle member. Валидация теперь
отвергает обе такие definitions, включая идентичные; VisibleTxId остаётся ссылкой
на единственный literal target. Отдельный regression test фиксирует это правило.

Проверено на release fingerprint
`d6e0e56b25b40455a52e1ce7c290b1c0f6bf76bc2a947ea2be33162e3d15953f`:

- Полный workspace: **428 Rust-тестов**,0 failures/ignored;16 новых integration
  tests MTM. Format и Clippy all-targets `-D warnings` прошли.
- JS syntax и `web/wire.test.mjs` прошли.
- `scripts/check-demos.py`: **64 CLI demos**, каждый новый MTM export проверен
  отдельной Python Fraction арифметикой. Все committed boundaries шести demos
  восстановлены; полный resumed export совпадает побайтно.
- `scripts/check-ui.py`:58 прежних demos прошли; `scripts/check-valuation-ui.py`:
  все6 новых demos прошли, включая точный `1/2`, capital timing, historical
  Unpriced, LP/CLMM/order/debt, history/readonly, download, restore и mobile.
- Визуально просмотрены `output/m5a-capital.png` и
  `output/m5a-unpriced-mobile.png`; горизонтального переполнения нет.

Артефакты CLI: `output/m5a-final-validated/`. Модель демонстрирует:
стоимость115 Q и PnL10 Q после взноса20/вывода15; известную текущую стоимость10 Q
с Unpriced PnL из-за исторического взноса; wallet cycle +24104 Q при полном
LP-owner MTM PnL−240 Q; post-loss депозит и долг, native CLMM fees/surplus и
remaining book escrow. Существующие scenarios без valuation policy сохраняют
прежний JSON/экспортный профиль (кроме обычного обновления build fingerprint).

**Границы готовности:** независимый review DEC-27 и DEC-28 ещё не выполнен.
Предыдущие разрешения на агента охватывали код до DEC-26; новый reviewer без
согласования не запускался. Сравнительные серии, HODL, полный Explorer,
оставшиеся user-task и performance gates v1.0 впереди. Среда не публиковалась.

### Независимый review DEC-27–DEC-28 и исправление F-01

Пользователь разрешил одного общего read-only reviewer ответом «Продолжай».
После восстановления доступа к среде отдельный агент проверил
`d4d5726..e32849c` и отдельным snapshot исправление
`22b1be27f02ab80320ba1d587ff997ea74d74c5b`.
Полный отчёт: `docs/reviews/dec27-dec28-independent-review.md`; исходные
самостоятельные скрипты и JSON reproducer сохранены без изменений в
`docs/reviews/dec27-dec28-regressions/` с hashes и условиями воспроизведения.

Подтверждён один P2: capital binding допускал незарегистрированного sender.
Это нарушало ordinary-endpoints validation; сама emission позднее отклонялась
на admission, ложного capital flow не было. Fix добавил проверку регистрации
sender. В независимой перепроверке original reproducer отвергается до запуска,
контроль без capital label остаётся admission rejection, а зарегистрированный
помеченный Transfer создаёт ровно один inflow. F-01 закрыт; других подтверждённых
findings нет. Прежнее разрешение не запрашивалось повторно.

Reviewer выполнил36 профильных integration tests и2 capability/rollback unit
checks; собственные160 flash arithmetic cases,60 bounded DFS graphs,100 wide
Fraction portfolios,129 independently extracted MTM components и отдельный
StableSwap, live negative-half/gross-offset, latest/future/stale records,
bundle commit/rollback/Own collision, noninterference и CLI replay10 demos.
HTTP/Chromium smoke6 MTM demos проведён на отдельном порту8091, сервер остановлен.
Основной repo reviewer не менял. Scope и пределы проверки явно перечислены
в отчёте; новые worst-case performance budgets этим review не закрыты.

После fix основной агент проверил17 valuation integrations и весь workspace:
**429 Rust tests**, format/Clippy all-targets `-D warnings` — pass.
Release build `4b3941b9ec0f00f4a81be4c24d366a301a0e34075a266342747d7da41a5a84ea`:
**64 CLI demos** и all-boundary MTM resumed exports прошли через
`scripts/check-demos.py`; независимый JSON reproducer также отвергнут этой сборкой.
Артефакты: `output/dec28-fix-validated/`. Большой browser/differential scope
review относится к e32849c; на fix независимо проверены адресные случаи,
основной агент не повторял весь браузерный набор ради одной input-проверки.
Независимый gate DEC-27–DEC-28 закрыт. Comparative series/HODL/Explorer и
performance/user-task gates всей v1.0 впереди. Публикации среды не выполнялось.

### Подготовлен M5b / Proposed DEC-29 — парные серии экспериментов

Контракт `docs/spec.md` §23.2.17 подготовлен до реализации. Статус **Proposed**;
владелец ещё не согласовывал новый ExperimentPlan, runner/report/UI importer.
Принятые DEC-27/28 и их закрытый independent review этого решения не заменяют.

Предложены два режима: BuilderOrder только с заранее материализованными
кандидатами и без автономных политик; AgentPolicy с перечисленными targets,
общим builder, неизменными authority/network/внешними входами. Полные Scenario
каждой seed/variant пары сохраняются, изменения проверяются по разрешённым
путям и common-input hash. Native RNG/fee/execution не меняются. Runner
последовательный, bounded, экспортирует один Run перед переходом к следующему.

Отчёт сохраняет отрицательные/нулевые результаты, fees, included failures,
rejection attempts и все причины Unpriced/ModelError. Paired deltas соединяют
одинаковые seeds только при наличии обеих полных оценок; отдельно показаны
cohort sizes/exclusions. Mean/median/nearest-rank p05/p95/sample variance —
exact signed fractions. Первый UI slice читает сохранённый series report;
live batch controller, HODL и полный Explorer не включены скрыто в этот контракт.

`docs/reviews/experiment-series-reference.py/.json` содержит12 независимых
pre-implementation cases: Python Fraction и scalar ChaCha12 из DEC-26 reference.
Основные ожидания: builder four-case mean delta−575/2 Q, variance66125/3 Q²;
RandomUser probability0 vs500000 на seeds0..19 — mean delta−1629/4 Q,
median−390 Q, sample variance2093695/76 Q². Missing-cohort example даёт paired
mean1, тогда как разность несопоставимых mean98/3 была бы ошибочной подменой.

Reference assertions прошли; повторная генерация JSON совпадает побайтно.
Сверены48 отдельных native runs (8 builder,40 policy) на release build
`4b3941b9ec0f00f4a81be4c24d366a301a0e34075a266342747d7da41a5a84ea`:
фактические balances/PnL, paid fees и RNG positions совпали с независимыми
ожиданиями. Это проверка исходных single-run fixtures, а не утверждение о
готовности production runner серий. Артефакты: `output/dec29-reference-crosscheck/`.
Для реализации DEC-29 требуется согласование владельца согласно §23.7.

### M5b / Accepted DEC-29 — реализация парных серий

Владелец согласовал DEC-29: «продолжай пожалуйста, всё согласовываю»;
согласование отражено в1a4e5af. Предыдущая запись Proposed описывает подготовку
контракта до этого ответа; повторное согласование реализации не требуется.

Добавлены typed Plan/diff/common hashes, проверки двух режимов и восьми policy
families, последовательный native runner, полные per-run exports, exact
statistics/cohorts и paired deltas. В отчёте сохранён исходный план, а final
manifest служит единственным completion marker. I/O failure оставляет Incomplete;
ModelError сохраняет replayable prefix и не становится конечной observation.
Report import проверяет структуру, план, hashes и пересчитывает derived summaries.
Открытие выбранного checkpoint отдельно проверяет replay, Scenario, результаты
строки и воспроизводимость заявленного ModelError. Browser не считает денежные
величины и не исполняет серию. Readonly import отделён от singleton Run.

Добавлены две серии:8 builder runs и40 RandomUser runs с заранее сохранёнными
Fraction/scalar ChaCha12 ожиданиями. Предварительные проверки включают equality
с отдельным native Run, порядок seeds, all-negative/zero outcomes, missing pairs,
arithmetic caps, строгий wire/diff null-vs-absent, запрет посторонних изменений,
ModelError prefix, I/O interruption и восстановление только выбранной строки.
Финальные результаты полных CLI/UI/workspace проверок будут записаны ниже.
Независимый review DEC-29 ещё не выполнен; HODL/Explorer/performance/user-task
приёмка всей v1.0 остаётся открытой. Среда не публиковалась.

### Финальная функциональная проверка M5b

Реализация: `3c66c21`; исправления мобильной навигации и таблиц:
`13786fe`, `2bc4465`. Коммиты отправлены в `origin/work`.

- Полный workspace: **457 Rust-тестов**,0 failures/ignored, включая28 новых
  проверок DEC-29; format, Clippy all-targets `-D warnings`, JS syntax и wire test
  прошли. Rust-код после этого прогона не менялся; последующие правки — CSS и
  браузерная проверка. Самостоятельные Fraction/scalar ChaCha12 эталоны сохранены
  до реализации в `docs/reviews/experiment-series-reference.py/.json`.
- Финальный release fingerprint:
  `da001a1040a93f35a728b37290426681ab0466bd85e8fb7ad31c2689b3864711`.
  **64 прежних CLI demos** прошли; шесть MTM demos сверены Python Fraction
  и восстановлены на всех committed boundaries с побайтно равными exports.
- `scripts/check-experiments.py`: **48 native runs** (8 builder и40 policy),
  точные rows/statistics сверены с pre-implementation ожиданиями. Каждый полный
  native export побайтно совпадает и с отдельным `run`, и с `resume` checkpoint.
  Повторная запись в существующий каталог отвергается без изменения manifest.
  Дополнительно экспортированы реальные Unpriced и ModelError серии.
- Все **64 прежних browser demos** прошли на реализации `3c66c21` до двух CSS
  исправлений. На финальной сборке `scripts/check-experiments-ui.py` проверил
  четыре серии: builder/policy/Unpriced/ModelError, точные дроби и counts,
  все seeds, исключения, readonly import, отклонение чужого checkpoint/подменённой
  сводки, успешное открытие выбранного checkpoint, download и mobile.
  Попытка продолжить ModelError из UI снова выдаёт ошибку и сохраняет prefix.
- Визуальный просмотр выявил два дефекта mobile: длинный U256 timestamp
  расширял block navigation; двухколоночная таблица исключений прятала причины
  за горизонтальной прокруткой. Исправлены оба; финальный browser test проверяет
  отсутствие page overflow и полную ширину причин в таблице исключений.

Наблюдаемый результат: builder mean paired PnL−575/2 Q, variance66125/3 Q²;
RandomUser20-seed mean−1629/4 Q, variance2093695/76 Q²,20 отрицательных outcomes
сохранены. Unpriced PnL не удаляет fee observations; ModelError prefix−125 Q
доступен в inspector, но все его final metrics исключены из distributions.

Артефакты: `output/m5b-final-validated/` (серии, независимые single-run/resumed
exports, screenshots), `output/m5b-final-demos/` (64 demos).
Финальный importable report — `manifest.json`; он содержит typed Plan и derived
rows, а не доказательство исполнения без native replay. UI проверяет план и
сводки; выбранный checkpoint дополнительно проверяется исполнением.

Функциональный gate M5b закрыт; **независимый review DEC-29 ожидается**.
Предыдущие reviews новый runner/importer не проверяли. Запрошен один read-only
reviewer для `1a4e5af..2bc4465` согласно §23.4. Следующие срезы — HODL/LP benchmark,
полный Explorer и performance/user-task приёмка; вся v1.0 ещё не завершена.
Среда не публиковалась.

### Подготовлен M5c / Proposed DEC-30 — HODL с фактическими внешними потоками

Пока идёт независимая перепроверка исправлений DEC-29, подготовлен следующий
контракт `docs/spec.md` §23.2.18. **Proposed**, production реализация не начата.
Профиль `flow-matched-hodl-v1` замораживает начальные underlying quantities
портфеля и зеркалирует только фактические labelled external capital transfers
в canonical receipt/event order. Он не исполняет сделки и не занимает активы.
Если конкретного актива для вывода нет, benchmark становится InfeasibleOutflow;
поздний inflow не исправляет невозможную раннюю историю.

Сравниваются actual/HODL PnL и excess после тех же внешних потоков. Отдельный
value_gap не подменяет Unpriced PnL, историческая цена потока не заменяется
конечной, fees не вычитаются дважды. Начальная liability не превращается в
неявный пассивный short. Native state/receipts/RNG остаются неизменными.

Независимый от production код `docs/reviews/hodl-reference.py/.json` содержит
18 Fraction cases. Примеры: CPMM100X/100Q после swap100Q при reference1→4 даёт
HODL500, actual400, excess−100; динамический inflow2X при10 и конечной цене15
даёт HODL PnL60; fractional claim1/3X даёт excess−1/3 с presentation floor−1;
unknown historical flow price оставляет PnL недоступным при известном current
value19/value_gap−1. Assertions и побайтная повторная генерация прошли.
Перед реализацией нового профиля требуется согласование DEC-30 по §23.7.
