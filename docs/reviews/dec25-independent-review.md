# DEC-25 / M4e — независимый adversarial review

Дата: 2026-11-06. Reviewer: отдельный read-only агент, независимый контекст.

## Scope и исходные материалы

Проверен diff `145ea8ab68330ff93a6f7fbe2f172c2441cff15c..d4a47091fdebf9f04ba0ebec9cc84bce1682ad61`.
Нормативные требования: `docs/spec.md` §7.6, §23.2.13 Accepted DEC-25,
§23.3–23.4; применимые ранее принятые правила authority, native execution/cost,
CPMM/LP, strict flash, heterogeneous routes DEC-20, bundles/Greedy DEC-22,
backrun DEC-23 и liquidator DEC-24.

Проверка выполнялась в `/tmp/chainarium-dec25-review`, созданном через
`git archive d4a4709`. До запуска прочитана skill
`cloud-environment-runtime`; применимых `AGENTS.md` не обнаружено.
Rust закреплён `1.99.1`, использован `scripts/cargo.sh` и локальный offline cache.
Основной checkout reviewer не изменял, коммиты и публикации не выполнял,
других агентов не запускал. Live server на 8080 не останавливался и не менялся.
Позднейшее docs-предложение DEC-26 / `7179bc3` в scope не входит.

Основания выводов: спецификация, diff/исходники, pre-implementation
`docs/reviews/hooks-reference.py` и `.json`, собственные вычисления и прогоны.
Объяснения разработчика и его итоговые заявления не использовались как доказательство.

## Findings P0–P3

Подтверждённых нарушений в проверенном scope не обнаружено: **P0: 0; P1: 0;
P2: 0; P3: 0**. Нет воспроизводимого finding, для которого можно было бы честно
указать нарушенный контракт, минимальный reproducer и виновные строки.

Это результат ограниченного adversarial review, а не доказательство полной
корректности, закрытие всех требований §23.3 либо приёмка v1.0.

## Выполненные проверки и объём

| Проверка | Результат / число |
| --- | --- |
| Чистый snapshot: `./scripts/cargo.sh test --workspace --locked --offline` | **334 passed, 0 failed**, до добавления reviewer tests |
| Повторный запуск pre-implementation Python reference | JSON семантически совпал с закреплённым `hooks-reference.json` |
| Собственные integration tests | **13 passed, 0 failed** |
| Собственная unit fault injection | **1 passed, 0 failed** |
| Прямые block executions в собственных matrices | **3 545**: 2 110 swap + 30 route/payout + 968 LP + 424 resource/reject + 3 cap + 7 node/trace + 3 counter/fee boundary |
| Повторные execution quotes | **101**: первоначальная quote и 100 повторов; checkpoint неизменен |
| Arbitrage | **8** полных runs; prices 0/1, payout 100/213/214/300 |
| Liquidator | **4** runs: Own/Flash × success/AfterSwap Reject; **16** проверенных candidates |
| Backrun с Greedy | **6** runs: prices 0/1 × payout 1/3/400; **60** проверенных candidates |
| Export/checkpoint/replay | **6** hook demos; checksums/replay, counter/config tampering и признаки commit |
| Дополнительные invalid-config cases | **21** case, включая protected role aliases, capabilities, points, assets, fee10000, нулевые значения, UTF-8 reason bytes и повтор fee rule/binding |

Эти категории не суммируются в одно число «всех executions»: policy runs
содержат свои preview, probes и canonical blocks, а restore выполняет replay.
Отдельно fault injection наблюдала **две** успешные транзакции перед ModelError.

Логи: `/tmp/dec25-baseline-tests.log`, `/tmp/dec25-independent-tests.log`,
`/tmp/dec25-fault-test.log`. Переносимые исходники и инструкция установки:
`/tmp/dec25-regression-tests/`.

## Проверенные инварианты и независимые ожидания

### Capabilities, authority и ограниченный контекст

Проверены validation и runtime authorization в `hooks.rs:141`, `hooks.rs:218`
и `engine/hook_calls.rs:94`. Необъявленные эффекты, неподходящие points,
несуществующий asset, fee10000, threshold0, amount0, пустой/слишком длинный
Reject, 17 инструкций и конфликтующие bindings отклоняются; существующая suite
дополнительно проверяет 16/17 инструкций и 64/65 hooks.

Независимо проверены совпадения hook account с управляемыми sender/owner,
builder и pool; source review охватил защиту pool/lending/flash/escrow/fee
accounts и обычные Transfer/TransferFrom. Approve не создаёт исполняемого
способа тратить hook account: protected account не может стать tx.sender,
а набор hook instructions не содержит TransferFrom.

Суммы PaySender проверяются по asset и по вызову точки, включая несколько
инструкций. Проверены точное достижение cap, разные assets, независимые caps
Before/After и runtime отказ после изменения ранее validated config.
Изменение чужого wallet не изменило разрешённую policy observation;
в hook evaluate передаётся typed Observation, без World, чужих wallets,
private/pending messages и protocol callback (`hooks.rs:167`, `hooks.rs:368`).

### Swap economics, effective fee, limit и route output

Собственные ожидания использовали формулу
`floor(Y * a * (10000-f) / (X*10000 + a*(10000-f)))` с u128,
без вызова `engine::cpmm_output` для получения эталона.
Matrix: обе стороны X/Q; inputs 0..210; fees 0/1/30/1000/9999;
threshold100, maximum200, несовпадающее направление и base fee37.
Проверены полный ledger, fee recipient, nonce, counter, hook cash и reserves
в Before/After observation.

При исходных 1000/1000 и input100: output82 при fee1000, payout3, used195,
sender900 X/890 Q, hook17 Q/counter2. Input201 останавливается на140 units;
нулевой/unexecutable output после BeforeSwap — на150 units.
Успешный край fee9999 проверен отдельно: reserves1000 X/1 000 000 Q,
input100 X дают ровно9 Q и used140 при единственном fee instruction.

Route при base fee0: 100 X →90 Q →82 R. Независимо проверены payout
1/3/82/100/300 в X, Q и R как BeforeSwap, так и AfterSwap: hop2 получает
ровно90 Q, final output82 R, input_spent100 X, residuals пусты. Payout остаётся
отдельным inventory, даже когда превышает вход или output. Существенная
граница реализации: `engine/route_calls.rs:46` использует typed AMM output,
а не разность wallet balance, содержащую payout.

Обходы через SwapOutput, flash callback и LiquidateAndSell проверены source
review и существующей suite; дополнительные Own/Flash liquidation trials
подтвердили общий executor path. Подтверждённого обхода hooks не найдено.

### LP, ресурсы, дерево и atomic rollback

LP matrix: max_x/max_y независимо0..21, late guard включён/выключен.
При reserves/shares1000/1000/1000 ожидаемый mint=min(max_x,max_y), transfers
равны mint; used175, либо185 при позднем отказе. Zero mint отказывает на130.
Проверены supply shares, собственные shares, cash и полный rollback.
RemoveLiquidity и LP round trip из закреплённого fixture прошли baseline tests.

Для before/after programs `[PaySender, IncrementCounter, Reject?]`
перебраны resource limits100..205. Сначала charge, затем effect; успех
не оставляет неоплаченных effects, а нехватка/Reject возвращает весь execution
ledger и hook counter, сохраняя included fee/nonce. Это дополнено исходной
suite с лимитами100..195 для полной reference-программы.

Node count независимо сопоставлен фактической trace: swap с одним counter
в каждой точке даёт5 nodes и60 units поверх transaction100. Проверены
1/2/3/31/128/200/204 swaps, ровно1024 nodes и отказ на1025.
Worst-case счётчик включает обе hook points (`hooks.rs:307`;
`model.rs:675`) даже при раннем отказе; source review и baseline покрывают
generated route/flash children и depth32/33. Прямой самостоятельный execution
самого глубокого LiquidateAndSell-tree не добавлялся.

Counter MAX и MAX−1 не wrap: собственный тест делает успешный первый increment,
затем overflow второго, проверяя rollback обоих. Canonical events отсутствуют
при revert; успешные attempt frames получают RolledBack.

Независимая fault injection через существующий `build_with_check` сначала
пропустила успешную hook tx, затем проверила вторую: counter4, hook14 Q,
sender767 Q, builder390 Q. После ModelError функция не возвращает рабочий
block, а исходный parent полностью совпадает с snapshot, включая первый
receipt's economic effects и envelope accounting. Test-only module не меняет
production execution. Round installation дополнительно проверена чтением
`Run::step`: state/scheduler устанавливаются только после успешного return.

### Arbitrage, backrun, liquidator и изоляция probes

Arbitrage: равные два CPMM дают 100 Q→90 X→82 Q; `[counter,payout]`
стоит35 units, весь route+guard —195. При price1 net=`reward−213`:
213 даёт Skip, 214 даёт ровно1, 300 даёт87. При price0 net=`reward−18`.
Проверены advertised net, canonical balances, output82 и единственный
canonical counter/payout. Ни аналитическая base-fee оценка, ни route output
вместо full balance delta не использованы для прибыли.

Liquidator на независимом исходном DEC-24 fixture: paid20/40/60/60,
seized2/4/6/6, sale29/57/84/84; с payout3 Own net12/20/27/27,
Flash5bps net11/19/26/26. Used205/235 соответственно. Все16 candidates
сверены, поздний hook Reject откатывает liquidation, write-off, sale и payout;
поиск не меняет canonical debt/cash/counter. Для успешного выбранного Own
carol1000→1027 Q, Flash→1026 Q; counter1, hook17 Q.

Backrun: victim200 Q даёт166 X, A становится834 X/1200 Q; каждый из10
кандидатов вычислен отдельно по двум CPMM formulas. Лучший input100:
100 Q→90 X→116 Q, net=`16+reward−5−310*price`, где response195 и payment115.
Reward3/price0 даёт14 Q; reward400/price1 даёт101 Q. Payout victim не
прибавляется к searcher net. Greedy commits trigger и ответ по одному разу:
counter2, hook cash1000−2*reward; при Skip остаётся только trigger counter1.
Проверены canonical victim/searcher/builder balances и число receipts.

### Read-only quote, observer, exports и checkpoint

101 повторная execution quote дала одинаковые receipt/output82/used205,
сохранив byte-identical checkpoint, cash, counter и RNG state.
Чистый `engine::quote` на hooked pool возвращает HookRequiresExecution.
UI source явно направляет к execution quote с sender/resource/minima;
hook panels читают сохранённые state/trace. Source inspection:
`web/hooks.js`, `web/routes.js`, `web/app.js` и соответствующие CLI routes.

Для всех6 hook demos проверены export, соответствие `committed` реальному
receipt.status, отсутствие мутации при чтении и replay restoration.
Rehashed checkpoint с изменённым committed counter или hook program
отвергается replay. Демо limit/rollback имеют последующую успешную tx;
её committed events корректно отличаются от effects первой отменённой попытки.

## Ограничения и незакрытые утверждения

- Отсутствие findings не доказывает корректность всех U256 inputs и всех
  сочетаний протоколов. Собственная differential matrix ограничена указанными
  детерминированными диапазонами; это не exhaustive fuzzing и не formal proof.
- Финансовая арифметика независима; входные сценарии/конфиги и часть plumbing
  взяты из repository demos. Это не отдельная полноценная реализация executor.
- Browser walkthrough/screenshot и HTTP UI acceptance reviewer не выполнял.
  Проверены core quote/export/replay и UI source; live 8080 не использовался.
- Полная Own/Flash liquidation runtime проверена; наиболее глубокая комбинация
  LiquidateAndSell+flash+hook и все смешанные StableSwap/CLMM/book paths не
  перебраны заново независимой matrix. Их source paths и относящиеся baseline
  tests были проверены в заявленных пределах.
- Hooks ограничены native CPMM согласно DEC-25. Performance gates, остальные
  policies, MTM/experiments, M5 explorer и полная готовность v1.0 этим review
  не закрываются.

Переносимые тесты прилагаются как дополнительная regression evidence.
Итог review применим только к указанному runtime hash и принятому DEC-25.
