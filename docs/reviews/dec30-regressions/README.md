# Артефакты независимого review DEC-30

Reviewer проверил `490881b..315a2a0` в отдельном контексте с разрешения владельца.
Подтверждённых findings нет. Production sources не изменялись; добавленные во
время review документы не меняют проверенный build fingerprint.

[Оригинальный отчёт](dec30-review-report.md) сохранён без редактирования; его
побайтная копия — [основной отчёт](../dec30-independent-review.md).
В этом каталоге сохранены все 16 файлов компактного архива reviewer без
изменений: шесть самостоятельных scripts/harness, семь logs, coverage, исходные
SHA-256 и отчёт. [artifacts.json](artifacts.json) фиксирует их размеры, SHA-256,
original paths, reviewed commit и build ID. `README.md` и `artifacts.json`
добавлены root-агентом при архивировании.

Проверки:

- `dec30-review-sweep.py`: 96 новых Fraction-сценариев, 672 boundaries,
  1 344 labelled flows, multiple portfolios, canonical order, fees, unavailable
  prices и permanent failures; coverage приведён в JSON и log.
- `dec30-review-native.py`: 589-bit mark overflow/recovery при cap512,
  пять on/off/changed-policy native runs, все prefix checkpoints и DEC-29
  paired exports/common-input rejection.
- `dec30-review-boundaries.py`: 0/1/64/65 активов, slot reuse/cumulative cap,
  rejection/revert/exhaustion и independently computed provenance hashes.
- `dec30-review-claims.rs`: invariant-valid synthetic LP fractions,
  gross initial debt, incomplete extraction и initial quantity overflow.
  Используется настоящий неизменённый debug core; native-reachable genesis
  для этих synthetic cases не заявляется.
- `dec30-review-ui.py`: семь presets, >2^53, −1/3, readonly history,
  downloads, escaped identifiers и desktop/mobile Chromium.
- `dec30-review-fixtures.py`: исходные18 HODL и24 valuation expectations
  совпадают с preimplementation JSON; fixtures не перезаписываются.

Команды и необходимые rlib/build IDs приведены в оригинальном отчёте. Scripts
зафиксированы для `/workspace/chainarium`; вместо `/tmp/dec30-review-*.py`
можно передать путь к соответствующему файлу этого каталога. Каждый запуск
задаёт новый `DEC30_REVIEW_ROOT`; boundary script также использует
`DEC30_SWEEP_ROOT` с результатами sweep. Native CLI не перезаписывает exports.
После другой сборки имена debug rlib следует брать из её фактического Cargo
output. UI использует текущий localhost8080 и восстанавливает исходный
singleton checkpoint после проверки.

Большие generated exports/checkpoints и binary harness в Git не добавлены:
они воспроизводятся scripts. Полный исходный архив reviewer (1 897 файлов,
39 937 749 bytes до сжатия, включая scenarios/exports/checkpoints/screenshots)
сохранён в рабочей среде как `output/dec30-independent-review/full-artifacts.tar.gz`;
его SHA-256 указан в manifest. Компактный исходный архив находится рядом.

Это review экономической корректности в указанном покрытии. Worst-case
performance при всех hard caps, полный Explorer и пользовательская приёмка
всей v1.0 остаются отдельными gates.
