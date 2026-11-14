import copy
import json
from pathlib import Path
import urllib.request
import urllib.error

ROOT = Path('/tmp/chainarium-dec29-evidence')
BASE = 'http://127.0.0.1:8765'

def request(path, data=None):
    req = urllib.request.Request(BASE + '/api/' + path,
        data=None if data is None else json.dumps(data).encode(),
        headers={'Content-Type': 'application/json'})
    try:
        with urllib.request.urlopen(req) as r:
            return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()

original = json.loads((ROOT/'builder/manifest.json').read_text())
request('run', original['plan']['cases'][0]['variants']['baseline'])
before = request('run')
assert request('experiment-report', original)[0] == 200
results = []
for label in ['horizon', 'included', 'rejections']:
    report = copy.deepcopy(original)
    if label == 'horizon':
        for row in report['rows']:
            row['horizon'] = row['committed_blocks'] = '0'
    elif label == 'included':
        report['rows'][0]['outcomes']['included']['Success'] = '999'
    else:
        report['rows'][0]['outcomes']['inclusion_rejection_attempts'] = '999'
    file = ROOT/f'accepted-inconsistent-{label}.json'
    file.write_text(json.dumps(report))
    status, body = request('experiment-report', report)
    assert status == 200, (label, status, body)
    assert request('run') == before
    results.append({'mutation': label, 'http_status': status,
        'singleton_unchanged': True,
        'row_horizon': body['rows'][0]['horizon'],
        'plan_horizon': body['plan']['cases'][0]['variants']['alternative']['scheduler']['rounds'],
        'included': body['rows'][0]['outcomes']['included'],
        'success_metric': body['rows'][0]['metrics']['success'],
        'rejected_attempts': body['rows'][0]['outcomes']['inclusion_rejection_attempts'],
        'rejection_metric': body['rows'][0]['metrics']['rejections']})
(ROOT/'import-invariants-result.json').write_text(json.dumps(results, indent=2))
print(json.dumps(results, indent=2))
