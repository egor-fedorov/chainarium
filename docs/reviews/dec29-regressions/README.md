# Самостоятельные проверки reviewer DEC-29

Исходные scripts/results/report скопированы побайтно из
`/tmp/chainarium-dec29-evidence`, без редактирования root-агентом.
`artifacts.json` фиксирует размеры и SHA-256. Исходный snapshot — `2bc4465`,
implementation diff `1a4e5af..2bc4465`. Эти файлы не являются production evaluator.

`REVIEW-2bc4465.md` — полный исходный review с двумя P2. `import_invariants.py`
и три `accepted-inconsistent-*.json` воспроизводят принятие внутренне
противоречивого отчёта. `report_size.py` воспроизводит Completed manifest,
который больше лимита его собственного viewer. Большие33/66 MiB артефакты
не включены в Git: script пересоздаёт их, размеры/хеши сохранены в artifacts.json.

Остальные scripts независимо проверяют native/standalone/resume equality,
Fraction statistics, missing cohorts, cap arithmetic, receipt fees/failure
fraction, RNG isolation, variant-specific ModelError, EFBIG/Incomplete,
input validation, hashes/diffs и Chromium UI. Findings и ограничения перечислены
в исходном report; это не exhaustive proof и не performance budget.

Для воспроизведения исходных findings подготовьте новый изолированный
`git archive 2bc4465` в `/tmp/chainarium-dec29-independent`, соберите его debug
CLI через `scripts/cargo.sh build --locked --offline` и создайте новую папку
`/tmp/chainarium-dec29-evidence`. Scripts содержат именно эти абсолютные пути.
Сначала получите native builder/policy series, затем запускайте
`independent_checks.py`, `import_invariants.py`, `report_size.py`,
`runtime_checks.py`, `validation_checks.py`, `ui_review.py`.
Для HTTP/UI требуется сервер этого же snapshot на loopback8765; Chromium здесь
находится в `/usr/bin/chromium`. В облачной песочнице sockets могут требовать
штатной escalation. Основную ветку и singleton на8080 эти проверки не меняют.

На исправленном коде original finding scripts с assert HTTP200/Completed должны
падать: это ожидаемый отрицательный контроль, не требование вернуть дефект.
Окончательная перепроверка сохранена в `FINAL-REVIEW.md`; оба P2 закрыты.
`verify_fixes.py` / `fix-4728abf/` сохраняют core/API recheck и residual download
размера. `verify_download_fix.py` / `fix-1d4e3a2/` — финальный Chromium
import/download/reimport near-limit и обычного отчёта. В final setup побайтно
идентичный Rust core4728abf обслуживал финальный JS1d4e3a2 через request
interception; большой hypothesis скрывался CSS для исключения layout benchmark.
Скрипт проверяет идентичность crates до начала этой адресной проверки.
Регрессии R1/R2 включены root-агентом в обычные experiment unit tests.
