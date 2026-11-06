# Chainarium

Локальная лаборатория DeFi с Rust-ядром. **v1.0 в разработке.**
Сейчас исполняются M1–M3a: CPMM swaps/LP, маршруты, арбитражная политика
на собственных средствах, flash loans, lending, oracle, проценты и ликвидации, StableSwap, TWAP и flash-ликвидация,
стакан GTC/IOC/FOK, rollback, fees/nonce и исчерпание ресурса. Полный объём и приёмка: [спецификация](docs/spec.md).

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
```

Экспорт создаёт **новую** папку: manifest, scenario, run, checkpoint, events JSONL,
аналитический CSV и отчёт. Для повторного запуска выберите новое имя папки.
Checkpoint восстанавливается только на committed-границе; checksum и повторное
исполнение проверяют его целостность. CSV не является форматом восстановления.

Доступны одиннадцать presets в UI и CLI:

| Файл | Демонстрация |
| --- | --- |
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

В SC-05 аналитическая ставка 12.5% за модельную секунду и reserve factor 20%;
это fixture, не рыночный прогноз. UI показывает C/D/S/E, отдельный залог,
точный HF, историю убытков и запись oracle, использованную в каждом вызове.

В M2 presets execution fee явно равна нулю. Отдельные тесты проверяют прибыль
после ненулевой execution fee, включая стоимость solver. Финальный
`AssertNetBalanceDelta` проверяет результат с фактической комиссией и возвратом
резерва; допустим только последним root-вызовом и в fee asset. LP claims не прибавляются к обеспечивающим активам;
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
```

Эта проверка проходит пользовательский путь, точные U256 через браузер/API,
checkpoint/resume, fork, A/B и мобильную верстку. Снимки сохраняются в `output/`.

## Границы текущего среза

- M1 использует версию сценария `chainarium-scenario-m1-v1` и фиксированные
  списки кандидатов блоков. Полный scheduler/mempool и реагирующие агенты M4 ещё
  не реализованы; отклонённые кандидаты показаны отдельно от receipts.
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
- CLMM, общий partial-fill router, MEV и hooks ещё не реализованы.
- Исследовательский MTM/PnL не рассчитывается; баланс не называется прибылью.
- UI — прототип M1–M3a с одиннадцатью presets. Полный explorer M5 впереди.
- Критерии всей v1.0 не объявляются выполненными. Независимый review и следующие
  открытые экономические контракты фиксируются согласно §23–24 спецификации.

Фактические результаты и оставшиеся проверки: [журнал прогресса](docs/progress.md).
Сценарий CLI ограничен 64 MiB, отдельная транзакция — 1 MiB. Web restore
принимает checkpoints до 64 MiB; CLI restore не имеет этого HTTP-ограничения.
Checkpoint привязан к SHA-256 исходников/lockfile/toolchain, target и build profile;
другая сборка явно отвергается. Начальная эмиссия каждого актива должна помещаться
в U256, как и балансы.
