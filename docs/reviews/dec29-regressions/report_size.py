import copy
import json
from pathlib import Path
import subprocess

ROOT = Path('/tmp/chainarium-dec29-evidence')
SNAPSHOT = Path('/tmp/chainarium-dec29-independent')
plan = json.loads((SNAPSHOT/'scenarios/experiments/builder.json').read_text())
plan['cases'] = plan['cases'][:1]
# Legal Plan: one case, two variants, six total blocks, < 64 MiB input.
# hypothesis is copied into both report.hypothesis and report.plan.hypothesis.
plan['hypothesis'] = 'x' * (33 * 1024 * 1024)
file = ROOT/'large-legal-plan.json'
file.write_text(json.dumps(plan, separators=(',', ':')))
output = ROOT/'large-legal-series'
p = subprocess.run([str(SNAPSHOT/'target/debug/chainarium'), 'experiment', str(file), str(output)],
    text=True, capture_output=True)
result = {'cli_status': p.returncode, 'stdout': p.stdout.strip(),
    'input_bytes': file.stat().st_size,
    'report_bytes': (output/'manifest.json').stat().st_size,
    'browser_and_Report_parse_limit_bytes': 64*1024*1024,
    'outcome': json.loads((output/'manifest.json').read_text())['outcome']}
assert result['cli_status'] == 0
assert result['input_bytes'] < 64*1024*1024 < result['report_bytes']
(ROOT/'report-size-result.json').write_text(json.dumps(result, indent=2))
print(json.dumps(result, indent=2))
