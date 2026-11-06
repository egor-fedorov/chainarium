"""Run every demo and verify DEC-28 exports/replay using independent Fraction math.

Usage: python3 scripts/check-demos.py output/a-new-directory
Requires target/release/chainarium from the current sources.
"""
from fractions import Fraction
from pathlib import Path
import hashlib
import json
import subprocess
import sys

root = Path(sys.argv[1])
root.mkdir(parents=True, exist_ok=False)
binary = Path('target/release/chainarium').resolve()


def command(*args):
    subprocess.run([str(binary), *map(str, args)], check=True, capture_output=True, text=True)


def exact(wire):
    assert all(isinstance(wire[k], str) for k in ['numerator', 'denominator', 'floor_atoms'])
    value = Fraction(int(wire['numerator']), int(wire['denominator']))
    assert wire == dict(numerator=str(value.numerator), denominator=str(value.denominator), floor_atoms=str(value.numerator // value.denominator))
    return value


count = 0
mtm_count = 0
for scenario in sorted(Path('scenarios/demos').glob('*.json')):
    target = root / scenario.stem
    command('run', scenario, target)
    run = json.loads((target / 'run.json').read_text())
    assert len(run['blocks']) == run['scenario'].get('scheduler', {}).get('rounds', len(run['scenario']['blocks']))
    count += 1
    if 'valuation_policy' not in run['scenario']:
        assert not (target / 'valuations.jsonl').exists()
        continue
    mtm_count += 1
    policy = json.loads((target / 'valuation-policy.json').read_text())
    manifest = json.loads((target / 'manifest.json').read_text())
    assert policy == run['scenario']['valuation_policy']
    assert manifest['valuation_policy']['policy_sha256'] == hashlib.sha256((target / 'valuation-policy.json').read_bytes()).hexdigest()
    boundaries = [json.loads(line) for line in (target / 'valuations.jsonl').read_text().splitlines()]
    flows = [json.loads(line) for line in (target / 'capital-flows.jsonl').read_text().splitlines()]
    assert len(boundaries) == len(run['blocks']) + 1
    assert flows == [f for boundary in boundaries for f in boundary['capital_flows']]
    totals = {key: dict(Inflow=Fraction(0), Outflow=Fraction(0)) for key in policy['portfolios']}
    for index, boundary in enumerate(boundaries):
        assert boundary['block'] == str(index)
        for flow in boundary['capital_flows']:
            assert flow['block'] == boundary['block']
            assert flow['timestamp'] == boundary['timestamp']
            value = flow['mark']['value']
            current = totals[flow['portfolio']][flow['direction']]
            totals[flow['portfolio']][flow['direction']] = current + exact(value) if value is not None and current is not None else None
        for portfolio in boundary['portfolios']:
            mark = portfolio['current']['mark']
            components = portfolio['current']['components']
            values = []
            for c in components:
                quantity = exact(c['quantity'])
                if c['mark']['value'] is not None:
                    value = exact(c['mark']['value'])
                    assert value == (quantity * exact(c['price']['mark']['value']) if quantity else 0)
                    values.append(value)
            if mark['value'] is not None:
                assert len(values) == len(components)
                assert exact(mark['value']) == sum(values)
            for direction, field in [('Inflow', 'inflows'), ('Outflow', 'outflows')]:
                expected = totals[portfolio['portfolio']][direction]
                assert (None if portfolio[field]['value'] is None else exact(portfolio[field]['value'])) == expected
            inputs = [mark['value'], portfolio['initial']['value'], portfolio['inflows']['value'], portfolio['outflows']['value']]
            if any(v is None for v in inputs):
                assert portfolio['pnl']['value'] is None
            else:
                current, initial, incoming, outgoing = map(exact, inputs)
                assert exact(portfolio['pnl']['value']) == current - initial - incoming + outgoing
    # All committed boundaries can reconstruct identical full exports.
    for boundary in range(len(run['blocks']) + 1):
        checkpoint = root / f'{scenario.stem}-{boundary}.checkpoint.json'
        resumed = root / f'{scenario.stem}-resumed-{boundary}'
        command('checkpoint', scenario, boundary, checkpoint)
        command('resume', checkpoint, resumed)
        assert sorted(p.name for p in target.iterdir()) == sorted(p.name for p in resumed.iterdir())
        for artifact in target.iterdir():
            assert artifact.read_bytes() == (resumed / artifact.name).read_bytes(), (scenario.name, boundary, artifact.name)
    print(f'PASS {scenario.stem}: {len(boundaries)} valuations, {len(flows)} flows, Fraction cross-check and all-boundary replay', flush=True)
print(f'PASS: {count} CLI demos; {mtm_count} MTM demos with independent exact arithmetic and byte-identical resumed exports')
