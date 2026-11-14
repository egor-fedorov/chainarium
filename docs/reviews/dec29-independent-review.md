# Независимый review DEC-29 / M5b

Пользователь разрешил одного read-only reviewer ответом «Продолжай, да».
После восстановления среды агент проверил implementation diff
`1a4e5af..2bc4465` в отдельном `git archive` snapshot, затем исправления отдельно.
Основной репозиторий и singleton8080 reviewer не менял; локальные серверы
использовали отдельный порт. Authoring/delegation другим агентам не выполнялась.

Исходный review выявил **два P2**. Оба исправлены и независимо перепроверены;
**открытых findings DEC-29 нет**.
Полный оригинальный отчёт и неизменённые самостоятельные scripts/results:
[архив reviewer](dec29-regressions/README.md). `artifacts.json` фиксирует hashes;
большие generated reports воспроизводятся скриптами и не хранятся в Git.

| Finding | Поведение исходного snapshot | Исправление |
| --- | --- | --- |
| R1 / P2 | Import принимал row.horizon/committed_blocks=0 при Scenario.rounds=3; outcome.Success=999 при IncludedCount=1; rejection attempts=999 при selector=1. Это противоречия самого отчёта; singleton оставался неизменным | `a05ec23`: row.horizon сверяется с соответствующим Scenario; count selectors с outcomes, включая ModelError prefix_metrics |
| R2 / P2 | Plan с hypothesis33 MiB создавал Completed report69,232,220bytes, превышающий importer64 MiB. Повторное форматирование почти предельного отчёта при UI download также превышало cap | `4728abf`: metadata-size preflight и финальный bounded serialization check; oversized результат оставляет Incomplete. `1d4e3a2`: experiment download сохраняет compact JSON, остальные downloads имеют прежний default |

R1 воспроизведён root-агентом двумя падающими regression tests до исправления.
R2 metadata reproduction также сначала падал; добавлена отдельная регрессия
позднего роста reports через большой MetricId: все native rows/checkpoints
сохранены, final manifest остаётся Incomplete. В обычной suite теперь32
experiment tests; дополнен ModelError prefix-count случай.

Независимо от root reviewer проверил:

- 48 native demo exports против standalone и resume, каждый файл побайтно;
- 64 новых fractional-price cases /128 native runs собственной Fraction
 арифметикой: PnL, fees, paired differences, mean/median/nearest-rank quantiles,
 variance, signs и exact per-operation caps на64 prime denominators;
- асимметричные реальные Unpriced cohorts, no-op/no-receipt, отрицательные и
 нулевые результаты;2 Success+1 Revert+1 Exhausted дают failure1/2,
 paid fees455 и PnL−457;
- 4 seeds с target draw count12/24 при неизменных24 draws и selections другого
 агента; variant-specific ModelError после committed PnL−125, затем Completed
 другого варианта; selected native checkpoint replay;
- реальный child-only EFBIG после первого native export: сохранены prefix/row,
 manifest Incomplete, summary отсутствует;27 invalid Plan mutations до создания
 каталога; самостоятельное вычисление Plan/Scenario/common/external hashes и diff;
- Chromium exact wire/exclusions/ArithmeticLimit/mobile/readonly imports,
 mismatch/matching checkpoints и download. Исходные findings наблюдались в UI.

Контроль `4728abf` отдельно подтвердил HTTP400 для трёх оригинальных
противоречивых reports и altered prefix selectors; пять корректных reports
включая ModelError/ArithmeticLimit принимаются read-only. Original33 MiB Plan
отвергается до каталога. Независимый late-growth Plan с MetricId2 MiB/24 runs
оставляет все24 rows/checkpoints и Incomplete. Fresh builder и mixed ModelError
checkpoints проходят replay. В этом же контроле обнаружен остаток R2:
compact67,106,816bytes → HTTP200, pretty67,138,488bytes → HTTP413.


Финальная независимая browser-проверка `1d4e3a2`:
near-limit report67,106,816bytes → import HTTP200 → download того же размера
и SHA-256 → reimport HTTP200. Обычный report55,135bytes прошёл тот же цикл;
singleton не изменился, page errors отсутствуют. R1 и R2, включая остаток R2
на `4728abf`, закрыты без новых findings.

Для этого адресного JS recheck reviewer подтвердил побайтную идентичность всего
`crates/` между `4728abf` и `1d4e3a2`, затем использовал HTTP core4728abf и точные
финальные app.js/experiments.js через request interception. Большой hypothesis
был скрыт CSS, чтобы не превращать сериализацию/reimport в layout benchmark
32 MiB текста; реальные handlers/API/DOM updates/download работали. Полная
финальная release-сборка отдельно проверена root на обычных fixtures ниже.

Root validation:461 Rust tests, format/Clippy all-targets `-D warnings`, Node
syntax/wire;64 CLI demos и MTM Fraction/all-boundary replay на `4728abf`.
После финального JS-only fix на release fingerprint
`57ccee9e12746314d7f92a06e6e92ebf94888b796f5eab49d611b707ea0a1ad9`
повторены48 native/standalone/resume сравнений и четыре browser series
(builder/policy/Unpriced/ModelError), включая равенство размера compact download.
Артефакты: `output/dec29-final-verified/`, `output/dec29-review-fix-demos/`.

Границы: это bounded adversarial coverage, не исчерпывающее доказательство,
не performance budget и не power-loss durability test. Проверка внутренней
согласованности импортированного report не доказывает истинность его произвольного
PnL без native checkpoint replay. Полная приёмка M5/v1.0, HODL, Explorer и
performance/user-task gates остаются отдельными задачами.
