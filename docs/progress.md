# Прогресс Chainarium — 2026-11-06

Цель: весь v1.0 из §2.1. Текущий результат — **исполняемый прототип M1–M4d**.
Это не готовая v1.0. Независимый review M1–M3b завершён, три findings исправлены
и перепроверены. Отдельный review M3c–M3d завершён без подтверждённых findings;
область и ограничения сохранены в `docs/reviews/dec20-independent-review.md`.
DEC-05–DEC-24 приняты владельцем.
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
сохранены `docs/reviews/liquidator-reference.py`/`.json`. Нужен отдельный
независимый review нового экономического call и policy; DEC-23 его не охватывает.

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

UI содержит сорок presets, play/pause/step, просмотр block→tx→call, балансы, сравнение котировок и трасса solver,
LP shares, C/D/S/E, escrow, точный HF и запись oracle, фактически использованную
в конкретном вызове. Расчёт риска выполняет Rust; JS отображает готовые строки.
Есть export/checkpoint/restore/fork и A/B параметров SC-01.

## Фактическая проверка

- `scripts/cargo.sh fmt --check` и Clippy всех targets с `-D warnings` — прошли.
- `scripts/cargo.sh test --workspace --locked --offline` — **305 tests passed**:
  11 unit tests (включая fault injection), 19 acceptance, 20 composition, 19 credit,
  10 stable, 10 twap, 7 flash_liquidation, 14 book, 17 clmm, 11 heterogeneous,
  16 policies, 6 independent_dec20, 10 independent_dec21, 12 independent_dec22,
  18 scheduler, 26 bundles, 30 backrun, 16 independent_dec23,
  1 independent_dec23_boundaries, 12 liquidation_sale, 20 liquidator_policy;
  ignored=0. Независимые тесты сохраняют аналитические sweeps предыдущих reviews.
- `node --test web/wire.test.mjs` — канонические строки, диапазоны и точный
  decimal conversion прошли; JS экономические команды не проходят через Number.
- `python3 scripts/check-ui.py` — пройден путь всех сорока presets, readonly
  просмотр, trace, checkpoint/resume, fork/A-B, точные Amount/Price через HTTP,
  desktop/mobile; ошибок JavaScript и горизонтального overflow нет.
- Release build и `git diff --check` — прошли.
- CLI: все 40 demos исполнены. Оба StableSwap demos после checkpoint/resume
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
Независимый review DEC-24 ещё не выполнен; прежний DEC-23 review исключает этот scope.
Это M4d, не вся M4 или v1.0: hooks, остальные роли, MTM/эксперименты/explorer и
performance/user-task gates M5 остаются открытыми.

### Fixtures и границы доказательства

| Проверка | Доказательство | Ограничение |
| --- | --- | --- |
| Ledger/auth/execution/block | acceptance + 5 injected ModelErrors после успешной tx, включая accrual/oracle | M4b/M4c независимо проверены; review M4d впереди |
| CPMM/LP/router/flash | composition: rounding/minima/ownership/reentry/actual output/rollback; 500 CPMM invariant cases | Native models; совместимость с внешним deployment не заявлена |
| Lending accounting | exact fixture, caps/HF/LTV/cash, aggregate rates, multiple escrow, fractional carry, no-holder income | Прямой долг, заранее зарегистрированные позиции, максимум 1024/reserve |
| Liquidation/loss | exact waterfall, partial close, dust, min_collateral, fee-preserving rollback, 256 deterministic boundary cases | Нет recovery/treasury; вывод equity запрещён |
| Deposit shares | zero mint, minima, cash shortage без burn, last exit, bootstrap с E | Bankrupt проверен на invariant-valid defensive snapshot, не заявлен достижимым из genesis этого positive-payment preset |
| Oracle | unauthorized/source/pair/future/duplicate/out-of-order; возраст 60/61; safe stale operations; consumer trace | External/native spot/TWAP; bounded history и однократное floor проверены |
| Checkpoint/export | replay/resume/fork/tamper, протоколы, LP/MM schedules/decisions | M4b groups, M4c trigger/search и M4d preview/trials/inflight snapshots проверены |
| UI | реальные balances/trace, 40 presets, desktop/mobile, exact HTTP wire | Прототип; полная приёмка M5 не закрыта |

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
   независимый review нового среза ещё впереди.
   Затем M4: hooks и остальные роли;
   M5: MTM, эксперименты, полный explorer и приёмка. Новые существенные решения
   требуют конкретных карточек и согласования по §23.7.
5. Полные user-task/latency/performance budgets и независимое review должны
   быть выполнены до приёмки целевой v1.0. SC-06 с конкуренцией исполняется в M4d, независимый review впереди.

Изменения находятся в рабочем checkout. Среда не публиковалась, push и публикация релизов
не выполнялись. Ранее опубликованная конфигурация среды не изменялась повторно.
