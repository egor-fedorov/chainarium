# Архив независимых проверок DEC-27–DEC-28

Автор — отдельный read-only reviewer. Файлы ниже скопированы без изменений из
`/tmp/dec27-dec28-independent-review`; полный отчёт —
[dec27-dec28-independent-review.md](../dec27-dec28-independent-review.md).

Основной snapshot: `e32849c870b3e22a8885bb5e62b0d4819aa4ce3f`.
Fix snapshot: `22b1be27f02ab80320ba1d587ff997ea74d74c5b`.

Это архив реально выполненных проверок, а не новый CI runner. Скрипты ожидают
точные paths исходного review: `source/target/debug/chainarium`,
`fix-target/debug/chainarium` и соседние scenario/output файлы под указанным
scratch root. Для повторения нужна новая чистая scratch-среда с этими paths и
git archive соответствующих commits; повторный запуск поверх старого output
может читать ранее сохранённые результаты. `valuation_adversarial.py`,
`negative_fraction.py` и `noninterference_replay.py` импортируют `differential.py`.

Постоянная CI-регрессия F-01 включена в `crates/chainarium-core/tests/valuation.rs`:
`capital_sender_must_be_registered_even_for_admission_rejected_emissions`.
Она проверяет native input validation и контроль без capital label.

SHA-256 сохранённых originals:

| Файл | SHA-256 |
| --- | --- |
| differential.py | `3044d343fa264a4f8606f4d8ad430ccf50095397b09bb3d0491a0e96e7704ab6` |
| search_differential.py | `59dc69460306f79a39e595cebaec344f004557d7afd3122e348ae5c88b6bb45d` |
| valuation_adversarial.py | `d9da82f5341ad455888737ab426630e0425c4da6436592afee47ba1f26c181fe` |
| negative_fraction.py | `138fcf0c50e8969cc261f484d2b84cd823a7c78937475c746168be260fef1159` |
| noninterference_replay.py | `f418c9a42595739b953c9038f3776ddc2cdef8fc092d4662e1784e3c257744e5` |
| verify_fix.py | `f9a4a10780eea115b316786160b28fdfe0f600eb31689b2bf12f48ce087708d3` |
| unregistered-capital-sender.json | `d766c7c00a34d97b7d785994ca3f4700e021d2b599bd5f877891dfcbec3c782f` |
