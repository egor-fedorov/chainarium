# Независимый review DEC-23 / M4c

Дата: 2026-11-06. Исходный scope: `07c5018..99c26d13fe15c9e68f2610890b3b222fedb03d7a`.

**На проверенном head подтверждены два findings уровня P2 / Medium:** исчерпанный nonce searcher и переполнение времени, заданного backrun policy, могут остановить весь раунд вместо определённого отказа недоступного действия. Изменений канонического состояния при этих остановках не обнаружено. **Оба findings адресно закрыты после независимого recheck исправлений на `f99114a6613a3ef5b07531ebaf1e5fdfb01af322`; полный suite этой копии —271 passed.** Приложение сохраняет отличие между исходным review и проверкой исправлений; результат не является доказательством полной корректности.

## Независимость и границы

- Проверка выполнена одним отдельным reviewer без дальнейшего делегирования. Основной checkout не изменялся reviewer; коммитов, push, публикаций и внешних запросов reviewer не делал.
- Исходники извлечены командой `git archive 99c26d1` в `/tmp/dec23-independent-snapshot`. Дополнительные тесты, сборка, экспорты и логи находятся в `/tmp`. Изменения основного checkout после создания snapshot не использованы как доказательство исходной корректности.
- Нормативные источники: `docs/spec.md` §5–6, §11–13, §17, Accepted DEC-20/21/22/23 (§23.2.8–23.2.11), §23.3–23.4. Рассмотрены diff, `docs/reviews/backrun-reference.py`/`.json`, все шесть backrun presets и связанный scheduler/bundle/executor/router код. `docs/progress.md` и объяснения автора не использованы как доказательство.
- Сервер 8080 не останавливался и не изменялся. Новый browser/UI прогон не выполнялся; UI проверен чтением diff. Проверка экспорта CLI выполнена отдельно на snapshot.

## Confirmed findings на 99c26d1

### F1 — P2 / Medium: исчерпанный nonce с положительным bid прерывает весь раунд

**Место:** `crates/chainarium-core/src/scheduler/backrun.rs:368`, построение `payment.nonce = obs.nonce.checked_add(1)?` до передачи response в inclusion callback. Контекст: строки 335–379; native проверка допустимости nonce находится в `crates/chainarium-core/src/model.rs:518`.

**Воспроизведение:** взять `scenarios/demos/sc-backrun-public.json`, заменить только `accounts.alice.nonce` на строку `115792089237316195423570985008687907853269984665640564039457584007913129639935` (`U256::MAX`), оставить bid=5. `Scenario::parse` и `Run::new` принимают сценарий. Первый `Run::step()` возвращает `ModelError: amount overflow`; valid victim не фиксируется, весь раунд отменяется. Для контрольного варианта с тем же nonce и bid=0 inclusion отклоняет невозможный response, victim фиксируется нормально.

Переносимый воспроизводитель: `exhausted_searcher_nonce_must_skip_without_aborting_unrelated_victim` в `/tmp/dec23-independent-tests.rs`; готовый изменённый сценарий — `/tmp/dec23-independent-nonce.json`. В исходном snapshot:

```sh
./scripts/cargo.sh test -p chainarium-core --test independent_dec23 --locked --offline exhausted_searcher_nonce_must_skip_without_aborting_unrelated_victim -- --nocapture
```

**Ожидание:** определённый Skip/отказ недоступного собственного nonce sequence; отсутствие response/payment fees; victim и другие корректные действия могут продолжить раунд. Предварительная проверка должна учитывать расход nonce каждого собственного member: один increment без payment, два с payment. Проверка только возможности записать следующий nonce не защищает payment с nonce=MAX, который также должен суметь расходовать nonce.

**Основание:** DEC-23 требует обычные inclusion/executor checks и исключение неуспешного response/payment candidate. §6.3 отличает невалидные пользовательские параметры envelope от ModelError и запрещает останавливать корректную модель из-за предусмотренной проверки диапазона. Ошибка проявляется на крайнем, но принимаемом и достижимом значении собственного счётчика; guard поиска вводит зависимость остановки всего мира от bid.

### F2 — P2 / Medium: переполнение времени backrun policy превращается в ModelError

**Места:**

- `crates/chainarium-core/src/scheduler/backrun.rs:226`: `decision.at.checked_add(self.policy.lifetime)?`.
- `crates/chainarium-core/src/scheduler/runtime.rs:270`: `at.checked_add(delay)?` при постановке `BackrunReaction`.

**Воспроизведение A (deadline):** в public preset изменить только `scheduler.backrun.policies.trader.lifetime` на `U256::MAX` либо `U256::MAX−8`. Сценарий принимается, но actual reaction at9 даёт `ModelError: amount overflow` вместо Skip/ошибки входа. При lifetime=`MAX−9` deadline=`MAX` представим и поиск/включение успешно завершаются. CLI reproducer: `/tmp/dec23-independent-lifetime.json`; вывод: `/tmp/dec23-independent-lifetime.log`.

**Воспроизведение B (reaction delivery time):** изменить только `reaction_delay` на `U256::MAX` либо `U256::MAX−7`. Authorized native delivery at8 вызывает ModelError при расчёте due_at, victim не фиксируется. Контроль `reaction_delay=MAX−8` корректно ставит одну отложенную `BackrunReaction` на due_at=MAX; victim фиксируется, поиск в текущем горизонте не запускается. Готовый изменённый сценарий — `/tmp/dec23-independent-reaction-delay.json`.

Переносимые тесты: `nonrepresentable_user_lifetime_is_input_rejection_or_skip_not_model_error` и `nonrepresentable_reaction_delay_is_rejected_without_round_failure` в `/tmp/dec23-independent-tests.rs`. Оба падают на исходном head. Они допускают как ранний input rejection для непредставимого времени, так и явный Skip/drop с диагностикой; представимые граничные значения должны оставаться точными.

**Ожидание:** ошибочный пользовательский временной параметр отклоняется при проверке конфигурации либо делает конкретную backrun реакцию/отправку недоступной с сохранением диагностики. Saturation/wrap и искусственная подстановка MAX недопустимы. Корректные native действия продолжают раунд.

**Основание:** положительные lifetime и reaction delay входят в явно заданную конфигурацию DEC-23; это предусмотренная проверка диапазона пользовательского параметра по §6.3, а не нарушение инварианта мира. Два новых пути M4c возвращают checked arithmetic error как общий ModelError. Унаследованные из DEC-21 произвольные outgoing/observation/network delays не входят в этот finding и отдельно не исправлялись reviewer.

## Выполненные проверки

1. **Исходный полный suite:** `./scripts/cargo.sh test --workspace --locked --offline` — **251 passed, 0 failed**. Лог: `/tmp/dec23-independent-baseline-tests.log`. Включены 27 backrun integration tests, исходные injection tests ModelError в prefix/response/payment и прежние scheduler/bundle/router suites.
2. **Компиляция:** `./scripts/cargo.sh check --workspace --all-targets --locked --offline` — PASS. Лог: `/tmp/dec23-independent-check.log`.
3. **Независимые тесты на 99c26d1:** **16 tests: 13 passed, 3 failed**; три failed tests точно соответствуют F1 и двум ветвям F2. Лог: `/tmp/dec23-independent-adversarial.log`. Переносимый файл: `/tmp/dec23-independent-tests.rs`; в snapshot расположен как `crates/chainarium-core/tests/independent_dec23.rs`.
4. **Integer differential oracle:** 768 детерминированных сценариев / 12288 полных route candidates. Независимый u128 расчёт CPMM не вызывает executor/quote helpers. Варьируются оба направления victim, резервы двух пулов, pool fees, восемь размеров, собственный капитал, оба native prices, declared limits, block capacity, minimum net и bid. Сверяются net каждого кандидата, выбранный кандидат, own fees/units, канонические балансы и число receipts. Получены 64 прибыльные выбранные группы; все 768 случаев прошли.
5. **Видимость и информация:** 12 вариантов скрытого pending/future payload не изменяют нормализованный search report; служебные IDs, зависящие от EventId, нормализованы, экономические результаты и порядок кандидатов сравниваются полностью. Проверены неподтверждённое авторство/chain/sender, отсутствие native triggers от bundle members, недоступность funding из чужого pending, own inflight bundle busy guard, TTL и stale trigger без fallback в registry.
6. **Порядок и бюджеты:** отдельный точный DFS fixture с двух- и трёхшаговыми циклами, без повторения venue; 96 комбинаций уменьшенных общих caps; сверка метрик prefix/candidates/members/actual work. Глобальная граница64 candidates проверена как48 первого searcher +16 второго. Граница4096 venue checks проверена на512 полностью просмотренных disconnected venues ×8 inputs и на513 venues с незавершённым поиском. Reaction cap общая, а Skip не расходует intent cap.
7. **Системные переходы:** преобразованы credit и TWAP legacy fixtures в event scheduler с активными backrun previews. Receipts, system events, lending state, price history и feeds на каждой границе совпадают с legacy. Повторного начисления поверх preview не обнаружено.
8. **Atomicity/replay:** искусственное исчерпание checked EventId оставляет весь Run/queue неизменным, повторяется той же ошибкой. Каждый из шести presets восстановлен с genesis и обеих committed-границ: 18 byte-identical comparisons всего Run. Дополнительно checkpoint с delayed reaction, TTL/stale и dedup проверен независимо.
9. **Аналитический fixture:** `python3 docs/reviews/backrun-reference.py` воспроизводит сохранённый JSON побайтно (`cmp` PASS). Сгенерированный результат: `/tmp/dec23-independent-reference.json`.
10. **CLI:** локальная offline сборка snapshot и запуск всех шести presets. `search-probes.jsonl` имеет ровно по записи на блок, phase=`ConditionalSearchDiagnostic`, содержимое равно соответствующему per-block report; конечные Alice/builder balances соответствуют reference. Логи: `/tmp/dec23-independent-cli-build.log`, `/tmp/dec23-independent-cli.log`. Экспорты: `/tmp/dec23-independent-cli-{public,private,delayed,fifo,competition,fees}`.

При чтении execution path отдельно проверены authorized native payload → current visible message → prefix begin_block → immutable preview → own response/payment callback; baseline после victim; последние guard и действительные fees/bid; bundle target/deadline/allowed_reverts/namespace; terminal intersections и отсутствие standalone emission собственных members. Дополнительных подтверждённых нарушений этих правил в рассмотренных случаях не найдено.

## Приложение: адресная проверка исправлений

### 467876f — исправление F1

Отдельный `git archive 467876f` создан в `/tmp/dec23-independent-fix-snapshot`. Production diff к99c26d1 добавляет только проверку вместимости всей собственной nonce sequence в `Search::run`; ошибка становится `Skip NonceExhausted` до route trials. Авторская регрессия не использовалась как единственное доказательство.

Проверены исходная независимая nonce regression и отдельный `/tmp/dec23-independent-fix-tests.rs`: nonce=`MAX−0`…`MAX−4`, bid0/5 (10 случаев), правильный spend nonce, контроль отсутствия trial work при Skip, успешные соседние значения, единственное исполнение victim и byte-identical restore. Полный suite этого snapshot с тогдашними13 независимыми tests и отдельным boundary test: **266 passed, 0 failed**. Лог: `/tmp/dec23-independent-fix-tests.log`.

**F1 адресно закрыт для467876f.** Эта проверка предшествовала добавлению temporal regressions F2; она не означает отсутствия F2 в467876f.

### bcacbcf / f99114a — исправление F2 и заключительный recheck

Проверены точные production diffs:

- `bcacbcfc4fed14a1e19f37d77fcd28aa17a38593`: непредставимый response deadline теперь даёт `Skip DeadlineOverflow` до route trials.
- `f99114a6613a3ef5b07531ebaf1e5fdfb01af322`: непредставимое время реакции теперь даёт scheduler diagnostic `BackrunReactionTimeOverflow`, без enqueue и без отмены раунда; trigger dedup сохраняется.

Отдельный `git archive f99114a` создан в `/tmp/dec23-independent-final-snapshot`. Добавлены те же16 независимых тестов `/tmp/dec23-independent-tests.rs` и дополнительный nonce boundary test `/tmp/dec23-independent-fix-tests.rs`. Исходный snapshot99c26d1 не исправлялся. На последнем snapshot проверены все три ранее падавшие регрессии, положительные границы deadline=MAX / reaction due_at=MAX и 10nonce boundary cases. Полный `./scripts/cargo.sh test --workspace --locked --offline` завершился **271 passed, 0 failed**, включая17 независимых tests. Лог: `/tmp/dec23-independent-final-tests.log`.

Дополнительно `./scripts/cargo.sh check --workspace --all-targets --locked --offline` на итоговой копии —PASS; лог `/tmp/dec23-independent-final-check.log`.

Все три исправления сохраняют checked арифметику без saturation/wrap. Повторно прогнаны differential, caps, visibility, credit/TWAP и replay проверки из основного независимого файла. **F1 и F2 закрыты в пределах этих адресных воспроизведений; новых подтверждённых findings на исправленных путях не найдено.** Proposed DEC-24, новые liquidator reference документы и последующая реализация не оценивались, даже если соответствующие документы присутствуют в snapshot более позднего commit.

## Ограничения вывода

Это конечный независимый review, не доказательство полной корректности. Систематический новый integer oracle охватывает CPMM; StableSwap/CLMM/book здесь покрыты чтением общего execution path и исходным suite, без отдельной новой математической реализации каждого семейства. Не выполнялись новый browser/mobile прогон, длительная нагрузка, исчерпывающий перебор всех графов/сочетаний queue budgets и межплатформенная проверка. Добавление test-only файлов меняет fingerprint сборки snapshot; эти сборки не выдаются за неизменённый серверный fingerprint `7595e6132809d0ac95cb61f5611486915b65303760ed8b7f5ec5b967ff153001`.
