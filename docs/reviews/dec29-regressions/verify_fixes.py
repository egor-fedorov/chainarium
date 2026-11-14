"""Separate fix verification; leaves original snapshot/evidence untouched."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import urllib.request
import urllib.error

SRC=Path(sys.argv[1]); OUT=Path(sys.argv[2]); BASE=sys.argv[3]
OLD=Path('/tmp/chainarium-dec29-evidence');BIN=SRC/'target/debug/chainarium'
OUT.mkdir(exist_ok=True)
checks=[]
def load(p):return json.loads(Path(p).read_text())
def api(path,data=None):
    req=urllib.request.Request(BASE+'/api/'+path,data=None if data is None else json.dumps(data).encode(),headers={'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(req) as r:return r.status,json.loads(r.read())
    except urllib.error.HTTPError as e:return e.code,e.read().decode()
api('run',load(OLD/'builder/manifest.json')['plan']['cases'][0]['variants']['baseline'])
before=api('run')
for kind in ['horizon','included','rejections']:
    status,message=api('experiment-report',load(OLD/f'accepted-inconsistent-{kind}.json'))
    assert status==400,(kind,status)
    assert api('run')==before
    checks.append(f'R1 {kind}: original immutable corrupted report rejected400: {message}')
for kind in ['builder','policy','asymmetric-missing','rational-cap','mixed-model-error']:
    report=load(OLD/kind/'manifest.json')
    assert api('experiment-report',report)==(200,report)
    assert api('run')==before
checks.append('R1: five original valid reports import read-only, including real ModelError and ArithmeticLimit')
original=load(OLD/'mixed-model-error/manifest.json')
for field in ['success','rejections']:
    report=copy.deepcopy(original)
    report['rows'][0]['prefix_metrics'][field]['value']={'numerator':'999','denominator':'1','floor_atoms':'999'}
    status,message=api('experiment-report',report)
    assert status==400
    checks.append(f'R1 ModelError prefix {field}: rejected400: {message}')

# R2 old legal input is rejected before execution/directory creation.
dest=OUT/'large-disallowed'
p=subprocess.run([str(BIN),'experiment',str(OLD/'large-legal-plan.json'),str(dest)],text=True,capture_output=True)
assert p.returncode!=0 and not dest.exists(),(p.returncode,p.stderr)
checks.append('R2 original 33MiB-hypothesis Plan: rejected before any output directory: '+p.stderr.strip())

# Independent late growth case: 12 cases/24 native runs and a 2MiB selector ID.
# Metadata is small enough, but observations/pairs repeat the key beyond 64MiB.
plan=load(SRC/'scenarios/experiments/builder.json')
template=copy.deepcopy(plan['cases'][0]);plan['cases']=[]
for seed in range(12):
    case=copy.deepcopy(template);case['seed']=str(seed)
    for scenario in case['variants'].values():scenario['seed']=str(seed)
    plan['cases'].append(case)
plan['metrics']['independent-metric-'+'m'*(2*1024*1024)]={'type':'InclusionRejectionAttempts'}
file=OUT/'late-growth-plan.json';file.write_text(json.dumps(plan))
dest=OUT/'late-growth-incomplete'
p=subprocess.run([str(BIN),'experiment',str(file),str(dest)],text=True,capture_output=True)
assert p.returncode!=0 and 'report exceeds 64 MiB' in p.stderr,(p.returncode,p.stderr)
assert load(dest/'manifest.json')['outcome']=='Incomplete'
assert sum(1 for _ in (dest/'rows.jsonl').open())==24
assert len(list((dest/'runs').iterdir()))==24
assert all((dest/'runs'/f'{i:04}'/'checkpoint.json').is_file() for i in range(24))
assert not (dest/'summary.json').exists() and not (dest/'manifest.complete.tmp').exists()
checks.append('R2 independent 2MiB MetricId / 24 runs: late report growth rejected, all24 streamed rows/checkpoints retained, manifest Incomplete')

# Re-export valid scenarios under this build for matching native checkpoints.
for kind,source in [('builder',SRC/'scenarios/experiments/builder.json'),('mixed-model-error',OLD/'mixed-model-error-plan.json')]:
    dest=OUT/kind
    p=subprocess.run([str(BIN),'experiment',str(source),str(dest)],text=True,capture_output=True)
    assert p.returncode==0,p.stderr
    report=load(dest/'manifest.json')
    assert api('experiment-report',report)==(200,report)
    assert report['analysis']==load(OLD/kind/'manifest.json')['analysis']
    for i,row in enumerate(report['rows']):
        cp=load(dest/row['path']/'checkpoint.json')
        assert api('experiment-checkpoint',{'report':report,'row':i,'checkpoint':cp})==(200,cp['run'])
    checks.append(f'Fix build: {kind} export, import and every selected native checkpoint pass')
(OUT/'results.json').write_text(json.dumps(checks,indent=2))
print('\n'.join(checks))
