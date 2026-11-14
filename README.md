# Chainarium

Локальная лаборатория DeFi с Rust-ядром. **v1.0 в разработке.**
Сейчас исполняются M1–M5b: CPMM swaps/LP, маршруты, арбитражная политика
на собственных средствах, flash loans, lending, oracle, проценты и ликвидации, StableSwap, TWAP и flash-ликвидация,
стакан GTC/IOC/FOK, CLMM и общий partial-fill router, rollback, fees/nonce и
исчерпание ресурса. Добавлены event scheduler, mempool, Public/Private delivery,
FIFO/Fixed/FeePriority и сценарный ответ на видимое сообщение. Bundles
и ограниченный greedy builder выбирают группы с реальным bid и явными reverts.
Backrun policy ищет размер/маршрут по доставленной victim и отправляет bundle
с проверкой net после собственных native fees и bid. Liquidator выбирает target,
погашение и маршрут с Own/Flash funding; продаёт фактически полученный залог.
Конкуренты используют общий scheduler и оплачивают включённые попытки. Hooks
вокруг CPMM swap/LP меняют fee, ограничивают input и платят из собственного account;
их counter и transfers входят в общий rollback. Scripted/random users, borrower
с целями долга/залога и native oracle publisher используют общий scheduler.
Их выбор, preflight и фактические оплаченные receipts показаны отдельно.
Flash arbitrage ищет закрытый RequireFull цикл на фиксированный principal,
резервирует собственные native fees и проверяет net после settlement займа.
Reference MTM считает точную стоимость портфеля, учитывает позиции и долги,
оценивает внешний капитал по времени исполнения и явно показывает Unpriced.
Полный объём и приёмка: [спецификация](docs/spec.md).

## Запуск

Нужен Rust 1.99.1 (закреплён в `rust-toolchain.toml`). `scripts/cargo.sh` использует
обычный Cargo либо установленный в этой облачной среде toolchain в
`/workspace/.chainarium-tools`. Зависимости закреплены `Cargo.lock`.

```sh
scripts/cargo.sh build --locked
scripts/cargo.sh run --locked --bin chainarium -- serve 8080
```

Сервер слушает только loopback на порту 8080. В облачной песочнице запуск серверов
и сетевые запросы могут требовать разрешённого режима исполнения. UI показывает
канонические балансы отдельно от отменённых переводов. Управление исполнением
отделено от просмотра сохранённых блоков и вызовов.

```sh
scripts/cargo.sh run --locked --bin chainarium -- run scenarios/demos/sc-01.json output/demo
scripts/cargo.sh run --locked --bin chainarium -- checkpoint scenarios/demos/sc-01.json 1 output/checkpoint.json
scripts/cargo.sh run --locked --bin chainarium -- resume output/checkpoint.json output/resumed
scripts/cargo.sh run --locked --bin chainarium -- routes scenarios/demos/sc-03-heterogeneous.json 1 scenarios/queries/heterogeneous.json
```

Экспорт создаёт **новую** папку: manifest, scenario, run, checkpoint, events JSONL,
аналитический CSV и отчёт. M4 добавляет `delivery.jsonl` с scheduler diagnostics;
`run.json` содержит per-block queue snapshots и метрики. Для M4b
`builder-probes.jsonl` хранит выбор и диагностические пробы; только selected
receipts входят в канонический блок. M4c добавляет `search-probes.jsonl`:
разрешённый trigger, условные результаты поиска, выбранный размер и группа.
M4d добавляет `liquidator-probes.jsonl` с разрешёнными targets, accrual preview,
пробами и выбранным intent. Фактические paid/seized/sale сохранены в receipt.
M4e добавляет `hook-attempts.jsonl`: точки/инструкции и явный committed-флаг;
канонические hook effects и обеспеченные выплаты находятся в events JSONL.
M4f добавляет `baseline-decisions.jsonl`: цели, RNG-выборки и диагностические
пробы пользовательских, borrower и publisher политик. Реальные fees остаются
в канонических receipts; RNG восстанавливается вместе с checkpoint.
M4g добавляет `flash-arbitrage-probes.jsonl`: пути, ограничение поиска, provider,
собственный fee reserve, conditional net и typed исход отправки. Canonical
receipts отдельно показывают фактические расходы и rollback.
M5a добавляет `valuation-policy.json`, `valuations.jsonl` (genesis и committed
blocks) и `capital-flows.jsonl`. Manifest содержит hashes policy/reference inputs.
HTTP `GET /api/valuation/{block}` и UI читают те же точные дроби; панель MTM
позволяет скачать выбранную оценку вместе с правилами.
Для повторного запуска выберите новое имя папки.
Checkpoint восстанавливается только на committed-границе; checksum и повторное
исполнение проверяют его целостность. CSV не является форматом восстановления.

## Парные серии экспериментов (M5b)

```sh
scripts/cargo.sh run --locked --bin chainarium -- experiment scenarios/experiments/builder.json output/builder-series
scripts/cargo.sh run --locked --bin chainarium -- experiment scenarios/experiments/policy.json output/policy-series
```

`builder.json` сравнивает FIFO и FeePriority на четырёх фиксированных случаях:
средняя парная разность PnL −575/2 Q. `policy.json` сравнивает RandomUser
probability0 и500000 ppm на20 seeds: средняя разность −1629/4 Q.
Это демонстрации; достаточность выборки и статистическая значимость не заявляются.
Plan содержит полные Scenario. В паре различается только объявленный builder
или выбранные policy objects; остальные условия проверяются до первого запуска.

Серия создаёт новую папку с `plan.json`, `rows.jsonl`, `pairs.jsonl`,
`summary.json`, `report.md` и native exports в `runs/0000`, ... .
`manifest.json` — самодостаточный отчёт с исходным планом и единственный признак
завершения: до записи всех артефактов статус остаётся `Incomplete`.
ModelError сохраняет committed prefix и исключается из конечных метрик;
остальные запланированные запуски продолжаются. Ошибка записи останавливает экспорт.

В панели «Парные эксперименты» импортируйте итоговый `manifest.json`.
Таблицы показывают все seeds, точные распределения, парные разности и исключения
с причиной каждой стороны. Просмотр отчёта не меняет текущий запуск.
Выберите строку и откройте её `runs/NNNN/checkpoint.json`, чтобы заменить запуск
в инспекторе после native replay и проверки соответствия строке.
Импорт отчёта проверяет план и пересчитывает статистику сохранённых observations;
проверка этих observations исполнением выполняется при открытии выбранного checkpoint.

CLI Plan, завершённый отчёт и отдельный browser checkpoint ограничены64 MiB.
Размер metadata отчёта проверяется до исполнения; размер итоговых rows/statistics —
перед завершением серии, без создания второй копии JSON. Если итоговый отчёт
превысит предел, экспорт завершится ошибкой и оставит Incomplete с уже сохранёнными
native exports/rows. Completed manifest всегда укладывается в лимит импортера.
Live batch controller, возобновление всей серии, HODL и полный Explorer остаются
следующими срезами.

Проверка release-сборки и UI (сервер должен использовать ту же сборку):

```sh
scripts/cargo.sh build --release --locked
python3 scripts/check-experiments.py output/new-experiment-validation
python3 scripts/check-experiments-ui.py output/new-experiment-validation
```

Доступны 64 presets в UI и CLI:

| Файл | Демонстрация |
| --- | --- |
| `scenarios/demos/sc-mtm-capital.json` | Взнос2 X по10, цена15, вывод1 X: стоимость115 Q, PnL10 Q |
| `scenarios/demos/sc-mtm-unpriced.json` | Текущая стоимость10 Q известна; PnL Unpriced из-за цены исторического взноса |
| `scenarios/demos/sc-mtm-flash.json` | Wallet net+24104 Q и полный MTM PnL−240 Q владельца обоих LP |
| `scenarios/demos/sc-mtm-credit.json` | Залог, долг, проценты и депозитное право после убытка |
| `scenarios/demos/sc-mtm-clmm.json` | Признанные CLMM principal/fees без rounding surplus |
| `scenarios/demos/sc-mtm-book.json` | Собственный remaining escrow заявок и wallet без двойного счёта |
| `scenarios/demos/sc-flash-arb-success.json` | Flash100000 Q, net24104 Q после fee50 и native190 |
| `scenarios/demos/sc-flash-arb-unprofitable.json` | Малый цикл не окупает native fee: Skip без расходов |
| `scenarios/demos/sc-flash-arb-guard.json` | Успешная проба, поздний guard revert; оплачено190 Q |
| `scenarios/demos/sc-flash-arb-shortfall.json` | Изменение рынка нарушает settlement; rollback займа, fee180 Q |
| `scenarios/demos/sc-flash-arb-delayed.json` | Одна доставка в round3, BusyNonce в промежуточных решениях |
| `scenarios/demos/sc-flash-arb-budget.json` | Own299 не покрывает native fee reserve300; probes0 |
| `scenarios/demos/sc-01.json` | Swap, поздний отказ, исчерпание ресурса |
| `scenarios/demos/sc-lp.json` | Mint 100 shares, burn 25 и откат mint |
| `scenarios/demos/sc-03-route.json` | Политика: 100 Q → 90 X → 123 Q |
| `scenarios/demos/sc-04-flash.json` | Flash 100 Q, возврат 101 Q, 22 Q остаток; затем два отказа |
| `scenarios/demos/sc-05-credit.json` | Долг 80 → 90; ликвидация за 60 Q, убыток 30 Q, вывод 20 Q за 25 shares |
| `scenarios/demos/sc-stable.json` | StableSwap 100 Q → 99.860150 R, LP, цикл через CPMM и откат |
| `scenarios/demos/sc-stable-depeg.json` | Reserves 100 Q / 10000 R: вход 100 Q → 554.752748 R |
| `scenarios/demos/sc-twap.json` | TWAP 150, откат цены, возраст 60/90 s и lending consumer |
| `scenarios/demos/sc-twap-120.json` | Окно 120 s: TWAP 175, до полного окна NotReady |
| `scenarios/demos/sc-07-flash-liquidation.json` | Flash 60 Q → 6 X → 84 Q → 83 R → 76 Q; возврат 61 Q и откат недостаточной выручки |
| `scenarios/demos/sc-08-book.json` | 40 X за 420 Q + 5 Q fees; FOK, partial GTC, IOC, cancel и escrow |
| `scenarios/demos/sc-02-clmm.json` | 1000 X → 996 Q; burn/collect, остатки округления и rollback |
| `scenarios/demos/sc-clmm-crossing.json` | Три диапазона: 16000 X → 15755 Q; обратный partial swap и вывод LP |
| `scenarios/demos/sc-03-heterogeneous.json` | Book/CPMM: выход 165 R и остаток 15 X; CLMM/CPMM: выход 3327 R и остаток 4971 X; strict/late rollback и маршрут через StableSwap |
| `scenarios/demos/sc-lp-policies.json` | Пассивный LP удерживает позицию; второй LP переставляет диапазон в блоке 3 и выходит в блоке 5 |
| `scenarios/demos/sc-mm-oracle.json` | Oracle 10→12, реальные bid/ask 45/55→55/65, арбитраж 10 X→108 Q→11 X и отмена quotes при stale feed |
| `scenarios/demos/sc-scheduler-public.json` | Arrival ответа ровно в cutoff10: victim200 Q→166 X, ответ100 Q→116 Q |
| `scenarios/demos/sc-scheduler-private.json` | Private без searcher в recipients: ответ отсутствует |
| `scenarios/demos/sc-scheduler-delayed.json` | Arrival11: ответ переносится в блок с timestamp20 |
| `scenarios/demos/sc-scheduler-nonce.json` | Nonce1 приходит раньше0; оба исполняются, fee230 Q |
| `scenarios/demos/sc-scheduler-fifo.json` | Конфликт nonce0: ранний кандидат выигрывает, fee115 Q |
| `scenarios/demos/sc-scheduler-fee.json` | Тот же конфликт: ставка2 выигрывает, fee230 Q |
| `scenarios/demos/sc-bundle-allowed.json` | Разрешённый late revert: Alice575 Q, Bob20 Q, builder405 Q = fees355 + bid50 |
| `scenarios/demos/sc-bundle-forbidden.json` | Запрещённый revert: группа не выбирается, нет канонических fees/nonce |
| `scenarios/demos/sc-bundle-greedy.json` | Greedy выбирает bundle со score405 Q; ordinary далее не вмещается |
| `scenarios/demos/sc-bundle-fee.json` | Те же входы: FeePriority выбирает ordinary price3 с fee345 Q |
| `scenarios/demos/sc-bundle-visible.json` | Только уже видимая victim включается в bundle с ответом и bid5 Q; Alice net11 Q |
| `scenarios/demos/sc-bundle-private.json` | Недоступный VisibleTxId отклоняется, private victim исполняется только самостоятельно |
| `scenarios/demos/sc-backrun-public.json` | 5 размеров / 10 маршрутов: выбран100 Q→116 Q, net11 Q после bid5 |
| `scenarios/demos/sc-backrun-private.json` | Private без searcher: поиск не запускается |
| `scenarios/demos/sc-backrun-delayed.json` | Arrival11 пропускает целевой блок1; найденный ответ не включается |
| `scenarios/demos/sc-backrun-fifo.json` | FIFO включает standalone victim раньше группы; searcher не платит |
| `scenarios/demos/sc-backrun-competition.json` | Два searchers с bid5/7; Greedy выбирает bid7, winner net9 Q |
| `scenarios/demos/sc-backrun-fees.json` | 100000 Q→118032 Q; fees160+115 и bid5, net17752 Q |
| `scenarios/demos/sc-liquidator-own.json` | Погашение60 Q, продажа6 X→84 Q, net24 Q; списание долга20 Q |
| `scenarios/demos/sc-liquidator-flash.json` | Flash60000 Q, fee30, sale84905 Q; native200 Q, net24675 Q |
| `scenarios/demos/sc-liquidator-fifo.json` | Alice net24735 Q; Carol NoDebt и fee240 Q; builder410 Q |
| `scenarios/demos/sc-liquidator-fee.json` | Carol net24565 Q; Alice NoDebt и fee120 Q; builder460 Q |
| `scenarios/demos/sc-liquidator-stale.json` | Возраст цены10 s при max_age9 на preview: Skip, заём остаётся |
| `scenarios/demos/sc-liquidator-delayed.json` | Arrival31 после cutoff30; BusyNonce, включение в блок4 |
| `scenarios/demos/sc-hook-fee.json` | Dynamic fee1000 bps: 100 X→82 Q, payout3 Q, native195 Q, counter2 |
| `scenarios/demos/sc-hook-limit.json` | Input201>200: отказ140 units; следующий input100 проходит |
| `scenarios/demos/sc-hook-rollback.json` | Поздний отказ: fee205 Q сохранена, counter/payout/swap отменены |
| `scenarios/demos/sc-hook-shortfall.json` | Hook cash2 Q не покрывает payout3; полный rollback, fee195 Q |
| `scenarios/demos/sc-hook-lp.json` | Add/remove100 shares, по175 units; counter4, payout6 Q |
| `scenarios/demos/sc-hook-route.json` | 100 X→82 Q→75 R; payout3 Q остаётся отдельно, native225 units |
| `scenarios/demos/sc-agent-random.json` | Seed42: переводы20 Q в rounds1/4; sender730, recipient40, fees230; RNG position24 |
| `scenarios/demos/sc-agent-scripted.json` | AnyReceipt: оплаченный late revert125 Q, затем перевод20 Q с новым nonce |
| `scenarios/demos/sc-agent-preflight.json` | SuccessfulOnly: тот же отказ пропущен без платы; следующий перевод стоит115 Q |
| `scenarios/demos/sc-agent-borrower.json` | Deposit6 X→Borrow80 Q→Repay80 Q→Withdraw6 X; fees460 Q, финал540 Q/6 X |
| `scenarios/demos/sc-agent-spot.json` | Публикации цены20→5 Q/X; native Spot читает фактическое состояние при исполнении |
| `scenarios/demos/sc-agent-twap.json` | W2 s: NotReady/Skip, затем цена20 Q/X за111 units |

В SC-05 аналитическая ставка 12.5% за модельную секунду и reserve factor 20%;
это fixture, не рыночный прогноз. UI показывает C/D/S/E, отдельный залог,
точный HF, историю убытков и запись oracle, использованную в каждом вызове.

В M2 presets execution fee явно равна нулю. Отдельные тесты проверяют прибыль
после ненулевой execution fee, включая стоимость solver. Финальный
`AssertTransactionNetGain` проверяет результат относительно баланса на входе
транзакции, с фактической комиссией и возвратом резерва; допустим только последним
root-вызовом и в fee asset. LP claims не прибавляются к обеспечивающим активам;
`FlashSettled` — пояснение комиссии внутри возврата, не дополнительный перевод.

## Проверки

```sh
scripts/cargo.sh fmt --check
scripts/cargo.sh clippy --workspace --all-targets --locked -- -D warnings
scripts/cargo.sh test --workspace --locked
```

С работающим сервером, Python Playwright и Chromium:

```sh
python3 scripts/check-ui.py
python3 scripts/check-valuation-ui.py
# После release build; создаётся новая папка результатов:
python3 scripts/check-demos.py output/demo-validation
```

Эта проверка проходит пользовательский путь, точные U256 через браузер/API,
checkpoint/resume, fork, A/B и мобильную верстку. Снимки сохраняются в `output/`.

## Границы текущего среза

- Сценарии сохраняют envelope `chainarium-scenario-m1-v1`. Профиль
  `scheduler-m1-v1` использует списки кандидатов блоков; `event-scheduler-v1`
  задаётся полем `scheduler` и требует пустой `blocks`. Профили взаимоисключающие.
  Admission не резервирует баланс; включённый revert сохраняет fee/nonce.
- Реализованы LP mint/burn, маршруты до 3 hops / 64 кандидатов, ограниченная
  arbitrage policy и отдельные flash providers. SC-07 объединяет flash/lending и три площадки.
- Lending хранит отдельный escrow каждой позиции, C/D/S/E, deposit shares,
  дробные проценты и историю убытков. External oracle имеет права publisher,
  монотонные sequence/observed_at; чувствительные операции проверяют свежесть.
- StableSwap: A=100 в demos, Ann=2A, нормализация 10^18, output fee,
  до 255 metered solver iterations. UI сравнивает котировки и показывает шаги D/y.
- Native spot/TWAP: exact integral, 128 сегментов, окна 60/120 s, явный NotReady
  и возраст от observed_at. Read-only проекция не заменяет опубликованную цену.
- Стакан: lot/tick, price-time, GTC/IOC/FOK, защищённый escrow и cancel.
  UI показывает очередь и пробный ордер через тот же executor.
- CLMM: Q96/ticks±32768, позиции, crossings/gaps, Q128 fees, burn/collect
  и явное partial исполнение. UI показывает L по ranges, claims/surplus
  и пробный swap. Остатки округления не выводятся.
- Общий router исполняет до 3 native hops, передаёт фактический output и сохраняет
  типизированные spent/unspent/results. RequireFull отменяет любой остаток;
  KeepResiduals оставляет его в кошельке. Поиск делает до 64 проб через executor,
  проверяет капитал/ресурс/комиссии и требует явных направленных limits для CLMM
  и стакана. UI и CLI показывают все результаты, отказы и ограничение поиска.
- LP следует явному расписанию до 1024 событий; MM управляет до 16 собственных
  заявок по Fixed/PublicOracle anchor. Обе политики получают только разрешённое
  наблюдение и создают не более одного intent после пробного исполнения.
  Hold/Skip/Submit, фактическое включение и источник цены видны в UI.
  Heterogeneous arbitrage использует RequireFull и проверку прибыли на входе tx.
- M4c: `bounded-backrun-v1` получает доставленный native trigger и parent observation.
  Поиск ограничен 8 размерами, 3 hops, 64 route trials и 4096 venue checks за раунд.
  Preview содержит только parent+trigger; каждый route trial начинается с его копии.
  `RequireFull`, актуальный net guard и реальный payment защищают результат после
  fees/bid. Private, опоздание, FIFO и конкуренция показывают границы включения.
  Search и builder probes имеют отдельные метрики; поиск не списывает канонические fees.
- M4a: включительный cutoff, детерминированные EventIds, bounded mempool/queues,
  Public/Private visibility, delayed OnVisibleProgram и FIFO/Fixed/FeePriority.
  Backrun реализован в M4c; liquidator policies — в M4d; hooks CPMM — в M4e.
  OnVisibleProgram исполняет явное правило, не оптимизирует стратегию.
- M4b: bundle до 16 members, Own/VisibleTxId, target range, allowed reverts,
  реальный bid последним Transfer и непрерывное включение группы. Greedy делает
  до 64 probes / 256 member attempts; score — paid native fees от других senders
  плюс обозначенный исполненный bid. Self-fee и произвольный входящий перевод
  не повышают score. Нет глобального оптимума или MTM-оценки этих решений.
  FixedItems задаёт порядок типизированных candidate IDs; FeePriority группы
  использует minimum member price. Неуспешная проба не платит канонических fees.
- M4d: `liquidate-and-sell-v1` продаёт только фактический прирост collateral после
  вложенного Liquidate. `bounded-liquidator-v1` читает указанные публичные targets
  после общего accrual preview, перебирает 1..8 sizes и маршруты до3 hops.
  До16 targets,64 полных проб и4096 venue checks за раунд; один intent на policy.
  Own/Flash, native reserve, settlement, guard и конкуренция проходят общий executor.
  Late failure откатывает sale/liquidation/write-off; included revert сохраняет fee/nonce.
- M4e: `bounded-pool-hooks-v1` — до64 CPMM bindings и16 инструкций на точку.
  Before/AfterSwap и Before/AfterLiquidity имеют явные capabilities: fee по input,
  max input/reject, local counter и PaySender только из hook account. Каждая точка
  и инструкция стоят10 units, ledger transfer ещё5. Все children входят в общие
  node/depth caps. Другие venue families hooks пока не принимают.
  Payout не включается в route output; arbitrage net использует полный balance delta.
  Для hooks нужна execution quote; чистая base-fee кривая ею не считается.
- M5a: opt-in `reference-mtm-v1` — точные signed fractions и reference prices
  с availability/observation time. PnL = V−V0−in+out; capital labels относятся
  только к literal sole Transfer, исполненному в canonical block. Missing/stale
  или исторически недоступная цена сохраняет Unpriced. LP mark не является
  burn quote; deposit claim отличается от доступного cash. Пересекающиеся
  portfolios нельзя суммировать. Серии экспериментов и HODL ещё впереди.
- M4f: `baseline-agents-v1` — scripted/random user, target borrower и native
  oracle publisher. До одного intent и preflight на агента за раунд; AnyReceipt
  допускает отправку предсказанной оплачиваемой неудачи. RandomUser использует
  именованный ChaCha12 stream, две bounded выборки даже при Hold/BusyNonce;
  ModelError откатывает RNG вместе с раундом. Borrower выбирает один шаг по
  accrued debt/own collateral; publisher отправляет обычные Spot/Twap calls.
- M4g: `bounded-flash-arbitrage-v1`, фиксированный principal/provider,
  до3 hops/64 probes/4096 venue checks; RequireFull и root net guard после
  settlement. Shared family budget, explicit truncation/submit refusal;
  включённый actual revert оплачивается, диагностические пробы — нет.
- UI — прототип M1–M5b с64 presets. Полный explorer M5 впереди.
- Критерии всей v1.0 не объявляются выполненными. Независимый review и следующие
  открытые экономические контракты фиксируются согласно §23–24 спецификации.

Независимый review M1–M3b выявил три дефекта, исправленных в `3827f63` и
перепроверенных reviewer. [Отчёт и границы проверки](docs/reviews/m1-m3b-independent-review.md).
Для DEC-20/M3c–M3d выполнен [отдельный review](docs/reviews/dec20-independent-review.md)
без подтверждённых findings. Шесть независимых тестов с 4504 аналитическими
случаями и граничными LP/MM-сценариями добавлены в обычную suite.

Независимый [review DEC-21/M4a](docs/reviews/dec21-independent-review.md) завершён
без подтверждённых замечаний. В suite сохранены ещё 10 независимых тестов:
360 расписаний / 2160 раундов и 42 случая exhaustion/fee-to-self.

Независимый [review DEC-22/M4b](docs/reviews/dec22-independent-review.md) завершён
без подтверждённых findings; сохранены 12 тестов, включая 640 differential cases.
[Review DEC-23/M4c](docs/reviews/dec23-independent-review.md) выявил два P2
(nonce/time overflow); исправления до `f99114a` независимо перепроверены.

[Review DEC-24/M4d](docs/reviews/dec24-independent-review.md) завершён без
подтверждённых findings. Сохранены7 независимых tests:768 денежных сценариев,
344 resource boundaries, node/depth, Fixed/Greedy, U256 и checkpoint/replay.
[Review DEC-25/M4e](docs/reviews/dec25-independent-review.md) завершён без
подтверждённых findings. Сохранены13 integration tests и fault injection,
включая3545 прямых executions и проверку composed search/payout/rollback.
[Review DEC-26/M4f](docs/reviews/dec26-independent-review.md) выявил P1
(чужой wallet через preflight) и P2 (потеря refusal отправки). Добавлены
capability checks и явный submission outcome. [Перепроверка исправлений](docs/reviews/dec26-fix-verification.md)
на `ce78187` закрыла оба findings без новых подтверждённых замечаний.
Сохранены8 integration +2 unit tests исходного reviewer и ещё6 integration
+3 unit tests адресной перепроверки.
[Независимый review DEC-27–DEC-28](docs/reviews/dec27-dec28-independent-review.md)
завершён для flash arbitrage и reference MTM. Один P2: capital binding принимал
незарегистрированного sender — исправлен в `22b1be2` и независимо перепроверен.
Ложного capital flow не возникало; открытых findings этого review нет.
Сохранены независимые scalar/Fraction/DFS проверки и воспроизведение в
[архиве регрессий](docs/reviews/dec27-dec28-regressions/README.md).
Полный workspace после fix:429 tests;64 CLI demos. Остальные gates M5 остаются открытыми.
[Accepted DEC-29](docs/spec.md#23217-принятый-контракт-m5b--accepted-dec-29)
реализует парные серии с фиксированными входами, exact statistics и импортом
отчёта. Проверены457 Rust tests,64 CLI demos,48 native runs серий и browser import/replay.
Независимый review нового среза ожидается; полный журнал — в `docs/progress.md`.

Фактические результаты и оставшиеся проверки: [журнал прогресса](docs/progress.md).
Сценарий CLI ограничен 64 MiB, отдельная транзакция — 1 MiB. Web restore
принимает checkpoints до 64 MiB; CLI restore не имеет этого HTTP-ограничения.
Checkpoint привязан к SHA-256 исходников/lockfile/toolchain, target и build profile;
другая сборка явно отвергается. Начальная эмиссия каждого актива должна помещаться
в U256, как и балансы.

Следующий подготовленный контракт — [Proposed DEC-30](docs/spec.md#23218-предлагаемый-контракт-m5c--proposed-dec-30):
reference HODL с фактическими внешними потоками и excess PnL.18 самостоятельных
Fraction cases подготовлены до реализации; контракт ожидает согласования.
