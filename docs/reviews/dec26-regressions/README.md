# DEC-26 independent regressions

Исходный review: `f65c8b3..fc364d9`; [отчёт](../dec26-independent-review.md)
сохранён без изменения его выводов. На этом head независимые integration tests
дали 6 pass / 2 fail (F1/F2); два unit tests прошли.

Перенесены в обычную suite:

- `crates/chainarium-core/tests/independent_dec26.rs` — восемь тестов;
- `crates/chainarium-core/tests/dec26-independent-vectors.json` — scalar ChaCha12 vectors;
- `crates/chainarium-core/src/scheduler/runtime/independent_dec26.rs` — два unit tests.

Запуск: `scripts/cargo.sh test --locked --offline --test independent_dec26`
и `scripts/cargo.sh test --locked --offline --lib scheduler::runtime::independent_dec26`.
`generate-vectors.py` повторяет исходный расчёт; изменён только путь к repository root.

`hidden-wallet-0.json`/`hidden-wallet-1.json` — исторические контрпримеры F1;
исправленная версия обязана отклонять оба при validation.
`known-cap.json` — F2; второй round должен сохранять успешный trial,
но завершаться Skip / Rejected KnownTransactionsLimit без actual receipt.

`browser-check.py` сохранён как исторический репродуктор исходной версии
на отдельном loopback18026, не как acceptance исправления. Он утверждает
исходный неверный Submit и потому должен падать на исправленной версии.
Текущие браузерные регрессии F1/F2 и QueueLimit — в `scripts/check-ui.py`.
Временные пути из исходного отчёта описывают артефакты review; постоянные
fixtures и тесты перечислены выше. Проверка исправленного hash записывается отдельно.
