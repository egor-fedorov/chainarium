"""DEC-31 CLI/reference acceptance. Usage: python3 scripts/check-explorer.py NEW_OUT."""
from fractions import Fraction
from pathlib import Path
import hashlib
import json
import subprocess
import sys

root = Path(sys.argv[1])
root.mkdir(parents=True, exist_ok=False)
binary = Path('target/release/chainarium').resolve()
expected = json.loads(Path('docs/reviews/explorer-reference.json').read_text())['cases']

def command(*args, ok=True):
    result = subprocess.run([str(binary), *map(str, args)], capture_output=True, text=True)
    assert (result.returncode == 0) == ok, result.stderr
    return result

def metric(c, key):
    return next(m for m in c['metrics'] if m['metric'] == key)

def exact(q):
    if q is None:
        return None
    f = Fraction(int(q['numerator']), int(q['denominator']))
    assert q == dict(numerator=str(f.numerator), denominator=str(f.denominator), floor_atoms=str(f.numerator // f.denominator))
    return f

results = []
for name, ex in expected.items():
    cps = []
    runs = []
    for side in ['a', 'b']:
        source = Path('scenarios/explorer') / f'{name}-{side}.json'
        s = json.loads(source.read_text())
        cp = root / f'{name}-{side}.checkpoint.json'
        command('checkpoint', source, len(s['blocks']), cp)
        exported = root / f'{name}-{side}'
        command('resume', cp, exported)
        run = json.loads((exported / 'run.json').read_text())
        runs.append(run)
        cps.append(cp)
    ta = [0] + [int(b['timestamp']) for b in runs[0]['blocks']]
    tb = [0] + [int(b['timestamp']) for b in runs[1]['blocks']]
    times = sorted(t for t in set(ta + tb) if t <= min(ta[-1], tb[-1]))
    reports = []
    for t in times:
        target = root / f'{name}-{t}.comparison.json'
        command('compare', *cps, t, target)
        c = json.loads(target.read_text())
        assert c['time'] == str(t)
        for side, boundaries, cp in zip(['a', 'b'], [ta, tb], cps):
            p = c[side]
            i = max(i for i, at in enumerate(boundaries) if at <= t)
            assert p['block'] == i and p['timestamp'] == str(boundaries[i]) and p['age'] == str(t - boundaries[i])
            # CLI checkpoint bytes are the same canonical serializer used by History.
            assert p['history']['key'] == hashlib.sha256(cp.read_bytes()).hexdigest()
        sync = c['a']['timestamp'] == c['b']['timestamp']
        for m in c['metrics']:
            av, bv, dv = [exact(m[k]['value']) if m[k] else None for k in ['a', 'b', 'difference']]
            if dv is not None:
                assert sync and av is not None and bv is not None and dv == bv - av
            if not sync:
                assert dv is None and 'BoundaryTimeMismatch' in m['difference']['reasons']
        reports.append(c)
    final = reports[-1]
    if name == 'synchronous':
        assert [metric(c, 'pnl')['difference']['value'] for c in reports] == ex['delta']
    elif name == 'as_of':
        assert list(map(str, times)) == ex['times']
        assert [metric(c, 'pnl')['difference']['value'] for c in reports] == ex['pnl_delta']
    elif name == 'unknown_history':
        assert metric(final, 'value')['difference']['value'] == ex['value_delta']
        assert metric(final, 'pnl')['difference']['value'] is None
    elif name == 'wide_fraction':
        assert metric(final, 'pnl')['difference']['value'] == ex['pnl_delta']
        assert metric(final, 'value')['a']['value'] == ex['actual_a']
    elif name == 'different_decimals':
        assert metric(final, 'value')['difference']['value'] is None
        assert 'AssetUnitsMismatch' in final['reasons']
    elif name == 'short_prefix':
        assert list(map(str, times)) == ex['common']
    if 'excluded_time' in ex:
        out = root / f'{name}-invalid.json'
        command('compare', *cps, ex['excluded_time'], out, ok=False)
        assert not out.exists()
    occupied = root / f'{name}-occupied.json'
    occupied.write_bytes(b'keep-existing-content')
    command('compare', *cps, 0, occupied, ok=False)
    assert occupied.read_bytes() == b'keep-existing-content'
    invalid = root / f'{name}-invalid.checkpoint.json'
    raw = json.loads(cps[0].read_text())
    raw['run']['state']['timestamp'] = '999'
    invalid.write_text(json.dumps(raw))
    out = root / f'{name}-invalid-report.json'
    command('compare', invalid, cps[1], 0, out, ok=False)
    assert not out.exists()
    results.append(dict(case=name, boundaries=len(times), checkpoints=[str(p) for p in cps]))
    print('PASS CLI Explorer', name, flush=True)
(root / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
print('PASS six paired native fixtures, exact differences, as-of history, unavailable metrics, checkpoint identity and atomic output validation')
