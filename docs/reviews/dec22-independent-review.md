# Независимый review Chainarium DEC-22 / M4b

Дата: 2026-11-06. Область: **82e52c7..7fc000747e337d7c28abb65205edfe82701d1b91**.

**Подтверждённых findings не найдено.** В проведённых проверках не удалось воспроизвести нарушение принятого контракта DEC-22. Это ограниченный review указанного diff, а не утверждение полной корректности Chainarium или готовности всей v1.0.

## Изоляция и источники

- Review выполнен одним независимым агентом, без дальнейшей делегации.
- Создана копия `git archive 7fc0007` в `/tmp/dec22-independent-snapshot`. Production-код в этой копии не менялся; добавлен только отдельный integration test.
- Прочитаны DEC-22 (§23.2.10), DEC-21 (§23.2.9), применимые §5–6, §11–13, §17, §23.3–23.4, изменённые исходники/сценарии/тесты/UI/export и используемые ими общие executor/checkpoint paths.
- `docs/reviews/bundle-reference.py` запущен независимо; результат побайтно совпал с сохранённым `.json`. Численные ожидания сверены с выполнением и экспортом.
- `docs/progress.md` и объяснения автора не использовались как доказательство корректности.
- Reviewer не менял основной checkout, не выполнял commit/push/publish, не останавливал сервер 8080 и не отправлял данные за пределы среды. Во время работы родитель продолжил работу над DEC-23/M4c; последующий HEAD и его изменения **не входят** в этот review.

## Findings

Подтверждённых P0/P1/P2/P3 findings нет. Нет воспроизводимого замечания, для которого требовались бы исправление или принятие риска до завершения именно этого review.

## Проверенные экономические и информационные границы

Указанные ниже строки относятся к snapshot **7fc0007**, а не к последующим изменениям checkout.

| Область | Проверка и результат |
| --- | --- |
| Регистрация и полномочия | `scheduler/runtime/bundle_calls.rs:35` разрешает Own/VisibleTxId и проверяет каждый payload до вставки набора members. Проверены invalid tail, чужой sender/agent, конфликт TxId, повтор member, reserved ID, byte/native-ID caps. Невалидный хвост не регистрирует предыдущий валидный member и не мешает последующей валидной группе с тем же member. |
| Приватность и время разрешения | `scheduler/bundles.rs:225` получает payload только из разрешённых messages/own outbox. Дополнительно проверены private bundle с известным global registry member: чужой агент не получает права ссылаться на него; явный получатель может после доставки; будущая доставка не разрешает более раннюю ссылку. Уже разрешённый payload сохраняется после eviction сообщения. |
| Общие лимиты | `bundle_calls.rs:263`, `:342` и config validation используют общие pending/visible/queue/emission ограничения. Собственный тест проверил combined pending cap при native→bundle и bundle→native порядке. Штатные тесты покрывают 16/17 members, byte/native-ID/BundleId caps, общий visible cap, 64 probes и 256 member attempts. |
| Реальный bid | Проверяется последний member, payer, fee asset/account, точная сумма и единственный Transfer; payment запрещён в allowed_reverts. Fixture дал fees355 + bid50 = balance delta405, Alice575, Bob20, nonce3. Отказ позднего платежа не создаёт canonical fees/nonce. |
| Групповое включение | `bundle_calls.rs:533` проверяет inclusion перед каждым member на текущей пробной копии. Разрешённые ProtocolRevert/ResourceExhausted расходуют свои native fees/nonce; запрещённый исход оставляет весь кандидат вне canonical receipts. Проверены границы declared remaining439/440 и полная длина относительно inclusion slots. |
| Overlap / terminal IDs | `bundle_calls.rs:246` и `:404` исключают уже включённые, terminal standalone и stale members. Проверены совместные native/bundle IDs, конфликтующий payload, перекрывающиеся группы, поздняя standalone delivery после bundle selection. Один TxId не исполняется повторно. |
| Waiting / nonce / funding | Повторная проверка выполняется после каждого выбранного префикса. Независимый пример: ранний Bob bundle сначала не имеет fee budget; Alice bundle переводит ему250; затем Bob bundle включается. Итог Alice645, Bob125, builder230, три probes; пробная неудача не оплачена. |
| Deadline / range / TTL | Проверены deadline equality, expiry member/bundle, target waiting, inclusive range, retention. Повтор BundleId после TTL не оживляет candidate и не обновляет arrival. |
| Builders и ties | FIFO, FeePriority(min member price), старый Fixed, FixedItems и Greedy сопоставлены с независимым целочисленным эталоном на 640 мирах. Сверялись порядок выбранных candidates, порядок и статус receipts, used, fees, балансы и nonce. Штатные targeted cases дополнительно проверяют equal score → lower used → FIFO, self fee, произвольный перевод builder и отрицательный cash delta. |
| Бюджеты | `bundle_calls.rs:560` резервирует место на полную группу в selected-native cap; `:568` требует member-attempt budget на полную длину probe; фактически расходуются реальные попытки. Проверены eligibility exhaustion посередине группы без частичного commit, budget truncation, освобождение неиспользованной резервации и возобновление Waiting в следующем раунде с новым round budget. |
| Score и commit | `bundle_calls.rs:618` учитывает чужие native fees и designated bid; builder balance delta рассчитывается отдельно. `:652` переносит выбранный проверенный World и receipts целиком. Списания и execution не запускаются повторно при commit. |
| ModelError всего раунда | Кроме штатного fault-injection теста, собственный тест вызвал настоящий checked overflow при суммировании gross fees после уже выбранного успешного префикса. Supply, отдельные balances и fee budgets валидны; builder возвращает fee Alice между двумя большими оплатами. `Run::step` вернул ModelError, а весь Run, включая world, blocks, scheduler cursors/queue, остался побайтно эквивалентен состоянию до шага. Это проверка границы отказа; не утверждение поддержки gross score за пределами U256. |
| Accrual | `bundle_calls.rs:698` вызывает begin_block один раз перед поиском. Независимый credit fixture с overlapping candidates и многократно повторяемой неуспешной пробой дал те же final World, system events и receipts, что исходный последовательный credit fixture. |
| Checkpoint / replay | `run.rs:135` сверяет checksum/build и повторное исполнение. Собственный сценарий с pending overlapping groups и отложенной standalone delivery дал точное equality после restore/finish; TxId выбран один раз. Изменённый resolved bid отклонён даже после пересчёта checksum. |
| Canonical / diagnostic export | `run.rs:187` и `:225` сохраняют canonical events/CSV отдельно от probes. Для всех шести bundle demos выбранные probe receipts точно совпали с canonical receipts; CSV fees и canonical events совпали. Forbidden fixture экспортировал 0 canonical receipts/fees/events, несмотря на непустые диагностические receipts с fee. |

## Выполненные проверки

1. На исходном snapshot до добавления теста:

   ```sh
   cd /tmp/dec22-independent-snapshot
   ./scripts/cargo.sh test --workspace --locked --offline
   ```

   **211 tests passed**, 0 failed. Лог: `/tmp/dec22-independent-workspace-test.log`.

2. Независимые переносимые тесты:

   ```sh
   cp /tmp/dec22-independent-tests.rs \
     /tmp/dec22-independent-snapshot/crates/chainarium-core/tests/independent_dec22.rs
   cd /tmp/dec22-independent-snapshot
   ./scripts/cargo.sh test -p chainarium-core --test independent_dec22 --locked --offline
   ```

   **12 tests passed**, 0 failed. Один test содержит **640 differential cases**: 128 deterministic вариантов × FIFO/FeePriority/Greedy/FixedItems/Fixed. Эталон не вызывает production executor: он рассчитывает Transfer/late-false-assert, reserve/refund, revert/exhaustion, nonce, последовательный declared-limit fit, score и ordering через обычную целочисленную арифметику. Лог: `/tmp/dec22-independent-adversarial-test.log`.

   Переносимый файл: `/tmp/dec22-independent-tests.rs`.
   SHA-256: `49bdaf1878bb8655341a42de0b889c83fc33f938bf8b1102b9806306aedced75`.

3. CLI собран офлайн:

   ```sh
   ./scripts/cargo.sh build -p chainarium-cli --locked --offline
   ```

   Лог: `/tmp/dec22-independent-cli-build.log`. Сборка CLI для экспортов включает дополнительный test source в build fingerprint согласно build.rs; production implementation соответствует snapshot7fc0007.

4. Выполнены `run` и `resume` для всех шести bundle demos. Проверены полное equality `run.json` после resume, выбранные receipts, CSV fees и число canonical events.

   | Demo | Canonical receipts | Canonical fees Q | Builder Q | Diagnostic probe entries |
   | --- | ---: | ---: | ---: | ---: |
   | allowed | 3 | 355 | 405 | 1 |
   | forbidden | 0 | 0 | 0 | 1 |
   | greedy | 3 | 355 | 405 | 3 |
   | fee | 1 | 345 | 345 | 2 |
   | visible | 3 | 0 | 5 | 2 |
   | private | 1 | 0 | 0 | 1 |

   Лог: `/tmp/dec22-independent-cli-export.log`. Артефакты: `/tmp/dec22-independent-export-{allowed,forbidden,greedy,fee,visible,private}` и соответствующие `-resumed` каталоги.

5. `python3 docs/reviews/bundle-reference.py` + `cmp` с сохранённым JSON: совпадение. Результат: `/tmp/dec22-independent-reference-regenerated.json`.

6. `node --input-type=module --check` для `web/bundles.js`, `web/scheduler.js`, `web/app.js`: успешно. Изменения UI и browser acceptance checks прочитаны; проверено явное отображение diagnostic receipts отдельно от canonical, score/fees/bid и truncated status.

7. Финальная проверка сравнила каждый исходный tracked file snapshot с новым `git archive 7fc0007`: различий нет. Переносимый test file побайтно совпадает с выполненной версией. Лог: `/tmp/dec22-independent-integrity.log`.

## Ограничения и незаявленные результаты

- Live browser/UI, HTTP endpoints, мобильная вёрстка и полный `scripts/check-ui.py` этим reviewer не запускались. Сервер8080 не изменялся. Проверка UI здесь ограничена чтением кода и JS syntax; фактический end-to-end результат подтверждён через CLI/export.
- Собственный differential oracle ограничен Transfer и late AssertCondition, небольшими значениями и ограниченным набором кандидатов; это не исчерпывающий поиск U256-пространства или всех composed DeFi programs.
- Lending/accrual проверены отдельным реальным fixture; остальные протоколы покрыты штатным workspace suite и чтением общего executor path, но не отдельным независимым differential oracle для каждого протокола внутри bundle.
- Штатные negative tests и исходники — дополнительное свидетельство, а не замена независимому oracle или доказательство всех caps при максимальном совместном размере. Полный стресс-профиль 10000 rounds × maximal queues не выполнялся.
- Ошибки первоначальных версий собственных тестовых ожиданий исправлены в тестовом harness: funding требовал3 probes, а не4; сравнение credit fixture приведено к тому же FIFO-порядку; переполнение тестируется через валидный supply с оборотом fees, а не недопустимый initial supply. Эти несоответствия не классифицированы как defects production-кода.
- Отчёт применим только к указанному diff; DEC-23/M4c и изменения после7fc0007 не оценивались.
