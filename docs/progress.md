# Прогресс Chainarium — 2026-11-06

Цель: весь v1.0 из §2.1. Текущий результат — **исполняемый прототип M1–M4a**.
Это не готовая v1.0. Независимый review M1–M3b завершён, три findings исправлены
и перепроверены. Отдельный review M3c–M3d завершён без подтверждённых findings;
область и ограничения сохранены в `docs/reviews/dec20-independent-review.md`.
DEC-05–DEC-21 приняты владельцем.
Общий partial-fill router, LP/MM-политики и heterogeneous arbitrage реализованы.
Контракт M4a принят как **DEC-21** (§23.2.9) 2026-11-06;
scheduler/mempool/basic builders реализованы и функционально проверены.
Независимый review M4a завершён без подтверждённых замечаний;
отчёт — `docs/reviews/dec21-independent-review.md`.

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

UI содержит двадцать два presets, play/pause/step, просмотр block→tx→call, балансы, сравнение котировок и трасса solver,
LP shares, C/D/S/E, escrow, точный HF и запись oracle, фактически использованную
в конкретном вызове. Расчёт риска выполняет Rust; JS отображает готовые строки.
Есть export/checkpoint/restore/fork и A/B параметров SC-01.

## Фактическая проверка

- `scripts/cargo.sh fmt --check` и Clippy всех targets с `-D warnings` — прошли.
- Основная suite `scripts/cargo.sh test --workspace --locked --offline` — **174 tests passed**:
  7 unit tests (включая fault injection), 19 acceptance, 20 composition, 19 credit,
  10 stable, 10 twap, 7 flash_liquidation, 14 book, 17 clmm, 11 heterogeneous,
  16 policies, 6 independent_dec20, 18 scheduler; ignored=0.
  Независимые DEC-20 tests сохраняют 4504 аналитических случая и LP/MM границы.
- Ещё **10 independent_dec21 tests passed** в основном checkout после переноса
  из отдельного review: 360 расписаний, 2160 раундов, 42 resource/fee boundary cases.
  Всего в suite **184 tests**, все выполненные проверки прошли.
- `node --test web/wire.test.mjs` — канонические строки, диапазоны и точный
  decimal conversion прошли; JS экономические команды не проходят через Number.
- `python3 scripts/check-ui.py` — пройден путь всех двадцати двух presets, readonly
  просмотр, trace, checkpoint/resume, fork/A-B, точные Amount/Price через HTTP,
  desktop/mobile; ошибок JavaScript и горизонтального overflow нет.
- Release build и `git diff --check` — прошли.
- CLI: все шестнадцать demos исполнены. Оба StableSwap demos после checkpoint/resume
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

### Fixtures и границы доказательства

| Проверка | Доказательство | Ограничение |
| --- | --- | --- |
| Ledger/auth/execution/block | acceptance + 5 injected ModelErrors после успешной tx, включая accrual/oracle | M4a проверен; bundles/greedy впереди |
| CPMM/LP/router/flash | composition: rounding/minima/ownership/reentry/actual output/rollback; 500 CPMM invariant cases | Native models; совместимость с внешним deployment не заявлена |
| Lending accounting | exact fixture, caps/HF/LTV/cash, aggregate rates, multiple escrow, fractional carry, no-holder income | Прямой долг, заранее зарегистрированные позиции, максимум 1024/reserve |
| Liquidation/loss | exact waterfall, partial close, dust, min_collateral, fee-preserving rollback, 256 deterministic boundary cases | Нет recovery/treasury; вывод equity запрещён |
| Deposit shares | zero mint, minima, cash shortage без burn, last exit, bootstrap с E | Bankrupt проверен на invariant-valid defensive snapshot, не заявлен достижимым из genesis этого positive-payment preset |
| Oracle | unauthorized/source/pair/future/duplicate/out-of-order; возраст 60/61; safe stale operations; consumer trace | External/native spot/TWAP; bounded history и однократное floor проверены |
| Checkpoint/export | replay/resume/fork/tamper, протоколы, LP/MM schedules/decisions | M4a queues/inflight/seen IDs проверены; bundles потребуют новых fixtures |
| UI | реальные balances/trace, 22 presets, desktop/mobile, exact HTTP wire | Прототип; полная приёмка M5 не закрыта |

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
4. M4a / DEC-21 реализован, функциональные проверки и независимый review
   завершены; ограничения указаны в `docs/reviews/dec21-independent-review.md`.
   Следующий контракт — **Proposed DEC-22** в `docs/spec.md` §23.2.10:
   bundles/greedy с реальным bid, разрешёнными reverts, visibility и search budgets.
   `bundle-reference.py`/`.json` фиксируют расчёты до реализации; ожидается
   согласование по §23.7. Затем M4: поисковые backrun/liquidator policies, hooks;
   M5: MTM, эксперименты, полный explorer и приёмка. Новые существенные
   архитектурные решения требуют конкретных карточек и согласования по §23.7.
5. Полные user-task/latency/performance budgets и независимое review должны
   быть выполнены до приёмки целевой v1.0. SC-06 с конкуренцией — этап M4.

Изменения находятся в рабочем checkout. Среда не публиковалась, push и release
не выполнялись. Ранее опубликованная конфигурация среды не изменялась повторно.
