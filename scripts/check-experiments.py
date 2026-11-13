"""DEC-29 CLI against pre-implementation Fraction/scalar-RNG expectations.

Usage: python3 scripts/check-experiments.py output/new-experiment-validation
Uses the release binary; every one of the 48 demo runs is compared with an
individually executed native run and a resumed checkpoint, byte for byte.
"""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys

binary = str(Path('target/release/chainarium').resolve())
root = Path(sys.argv[1])
root.mkdir()  # Never reuse or overwrite a previous validation.
reference = json.loads(Path('docs/reviews/experiment-series-reference.json').read_text())['cases']

def command(*args):
    return subprocess.run([binary, *map(str, args)], check=True, capture_output=True, text=True)

def load(path):
    return json.loads(path.read_text())

def statistics(actual, expected):
    for key in ['minimum', 'maximum', 'mean', 'median', 'p05', 'p95', 'sample_variance']:
        assert actual[key]['value'] == expected[key], (key, actual[key], expected[key])
    for key in ['negative', 'zero', 'positive']:
        assert actual[key] == expected[key]

def compare_exports(a, b):
    assert sorted(p.name for p in a.iterdir()) == sorted(p.name for p in b.iterdir())
    for f in a.iterdir():
        assert f.read_bytes() == (b / f.name).read_bytes(), (a, b, f.name)

for kind in ['builder', 'policy']:
    destination = root / kind
    command('experiment', f'scenarios/experiments/{kind}.json', destination)
    report = load(destination / 'manifest.json')
    assert report['outcome'] == 'Completed'
    assert report['plan_sha256'] == hashlib.sha256((destination/'plan.json').read_bytes()).hexdigest()
    refs = reference['builder_four_case_sensitivity' if kind == 'builder' else 'random_policy_twenty_seeds']
    statistics(report['analysis']['paired']['alternative']['pnl']['statistics'], refs['paired']['statistics'])
    for row in report['rows']:
        seed, variant = int(row['seed']), row['variant']
        expected = refs['rows'][seed]
        pnl = expected[f'{variant}_pnl'] if kind == 'builder' else expected[variant]['pnl']
        assert row['metrics']['pnl']['value'] == pnl
        fees = expected[f'{variant}_fee'] if kind == 'builder' else expected[variant]['paid_native_fees']
        assert row['metrics']['fees']['value']['numerator'] == fees
        exported = destination / row['path']
        independent = root / f'{kind}-native-{row["path"].split("/")[-1]}'
        resumed = root / f'{kind}-resumed-{row["path"].split("/")[-1]}'
        command('run', exported / 'scenario.json', independent)
        command('resume', exported / 'checkpoint.json', resumed)
        compare_exports(exported, independent)
        compare_exports(exported, resumed)
    before = (destination / 'manifest.json').read_bytes()
    assert subprocess.run([binary, 'experiment', f'scenarios/experiments/{kind}.json', str(destination)], capture_output=True).returncode != 0
    assert (destination/'manifest.json').read_bytes() == before
    print(f'{kind}: {len(report["rows"])} runs, exact reference, native equality and replay passed', flush=True)

# Import/inspection fixtures: missing observations and canonical ModelError prefix.
p = load(Path('scenarios/experiments/builder.json'))
p['cases'] = p['cases'][:1]
unpriced = copy.deepcopy(p)
for s in unpriced['cases'][0]['variants'].values():
    s['valuation_policy']['reference_feeds'] = {}
(root / 'unpriced-plan.json').write_text(json.dumps(unpriced))
command('experiment', root/'unpriced-plan.json', root/'unpriced')
p['comparison'] = dict(type='AgentPolicy', targets=[dict(family='Arbitrage', agent='trader')])
maximum = str(2**256 - 1)
for s in p['cases'][0]['variants'].values():
    tx = s['scheduler']['emissions'][0]['transaction']
    tx['deadline'] = maximum
    del s['scheduler']
    s['model_versions'] = [v for v in s['model_versions'] if v not in ['event-scheduler-v1', 'bounded-mempool-v1', 'basic-builders-v1', 'visible-program-policy-v1']]
    s['model_versions'].append('scheduler-m1-v1')
    s['blocks'] = [[tx], []]
    s['chain']['interval'] = maximum
    s['valuation_policy']['max_age_seconds'] = maximum
    s['policies'] = {'trader': dict(account='alice', asset='Q', input='1', min_profit='1000000', resource_limit='200', resource_price='1')}
(root / 'model-error-plan.json').write_text(json.dumps(p))
command('experiment', root/'model-error-plan.json', root/'model-error')
r = load(root/'model-error/manifest.json')
assert r['outcome'] == 'CompletedWithModelErrors'
assert all(row['committed_blocks'] == '1' and row['metrics']['pnl']['reasons'] == ['ModelError'] and row['prefix_metrics']['pnl']['value']['numerator'] == '-125' for row in r['rows'])
assert r['analysis']['paired']['alternative']['pnl']['eligible_n'] == '0'
print('Unpriced and ModelError exports passed; ready for scripts/check-experiments-ui.py', flush=True)
