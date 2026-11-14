import copy
import json
from pathlib import Path
import resource
import signal
import subprocess
import urllib.request

ROOT=Path('/tmp/chainarium-dec29-evidence')
SRC=Path('/tmp/chainarium-dec29-independent')
BIN=SRC/'target/debug/chainarium'
events=[]
def load(p):return json.loads(Path(p).read_text())
def export(plan,label):
    file=ROOT/(label+'-plan.json');file.write_text(json.dumps(plan))
    if not (ROOT/label).exists():
        p=subprocess.run([str(BIN),'experiment',str(file),str(ROOT/label)],text=True,capture_output=True)
        assert p.returncode==0,p.stderr
    return load(ROOT/label/'manifest.json')
def api(route,data):
    req=urllib.request.Request('http://127.0.0.1:8765/api/'+route,
        data=json.dumps(data).encode(),headers={'Content-Type':'application/json'})
    with urllib.request.urlopen(req) as r:return json.loads(r.read())

# Change target draw cadence from 3 to 6 decisions; other agent remains identical.
plan=load(SRC/'scenarios/experiments/policy.json');plan['cases']=plan['cases'][:4]
for c in plan['cases']:
    for variant,s in c['variants'].items():
        s['accounts']['bob']['balances']['Q']='5000'
        s['baseline_policies']['recipient']=copy.deepcopy(s['baseline_policies']['trader'])
        p=s['baseline_policies']['recipient'];p['account']='bob';p['kind']['probability_ppm']=500000
        for program in p['kind']['programs']:program[0]['to']='builder'
        p=s['baseline_policies']['trader'];p['kind']['probability_ppm']=1000000
        p['kind']['cadence']['every']='2' if variant=='baseline' else '1'
r=export(plan,'foreign-rng')
for i in range(4):
    a=load(ROOT/'foreign-rng'/r['rows'][2*i]['path']/'run.json')
    b=load(ROOT/'foreign-rng'/r['rows'][2*i+1]['path']/'run.json')
    assert a['rng']['positions']['agent/trader']=='24' and b['rng']['positions']['agent/trader']=='12'
    assert a['rng']['positions']['agent/recipient']==b['rng']['positions']['agent/recipient']=='24'
    def selections(run):
        return [d['selection'] for block in run['blocks'] for d in block['scheduler']['baselines'] if d['agent']=='recipient']
    assert selections(a)==selections(b)
events.append('4 seeds: target draw count12 vs24; foreign agent24 draws and complete scalar selections identical')

# Variant-specific ModelError after a committed prefix; next variant completes.
plan=load(SRC/'scenarios/experiments/builder.json');plan['cases']=plan['cases'][:1]
plan['comparison']={'type':'AgentPolicy','targets':[{'family':'Reactive','agent':'trader'}]}
for variant,s in plan['cases'][0]['variants'].items():
    s['scheduler']['builder']={'type':'Fifo'}
    s['model_versions'].append('on-visible-program-v1')
    s['scheduler']['agents']['trader']['observe_public']=True
    s['scheduler']['agents']['trader']['observation_delay']='0'
    # The early transfer commits in block1; second emission triggers at block2.
    s['scheduler']['emissions'][1]['at']='12'
    s['scheduler']['reactive']={'trader':dict(account='alice',resource_limit='200',resource_price='0',
        reaction_delay=str(2**256-1) if variant=='alternative' else '0',lifetime='20',
        rules={'expensive':[{'type':'Transfer','to':'bob','asset':'Q','amount':'1'}]})}
r=export(plan,'mixed-model-error')
assert r['outcome']=='CompletedWithModelErrors'
a,b=r['rows']
assert a['status']['type']=='ModelError' and a['committed_blocks']=='1'
assert b['status']['type']=='Completed' and b['committed_blocks']=='3'
assert a['prefix_metrics']['pnl']['value']['numerator']=='-125'
assert all(v=={'status':'Unavailable','reasons':['ModelError']} for v in a['metrics'].values())
assert r['analysis']['paired']['alternative']['pnl']['eligible_n']=='0'
assert r['analysis']['variants']['baseline']['pnl']['eligible_n']=='1'
assert api('experiment-report',r)==r
for i,row in enumerate(r['rows']):
    cp=load(ROOT/'mixed-model-error'/row['path']/'checkpoint.json')
    assert api('experiment-checkpoint',{'report':r,'row':i,'checkpoint':cp})==cp['run']
events.append('Variant-specific overflow: preserved one-block PnL−125 prefix, excluded all final metrics, following baseline Completed; both native checkpoint matches pass')

# Actual filesystem error after a first streamed export, without test hooks.
plan=load(SRC/'scenarios/experiments/builder.json');plan['cases']=plan['cases'][:1]
for variant,s in plan['cases'][0]['variants'].items():
    s['scheduler']['builder']={'type':'Fifo' if variant=='alternative' else 'FeePriority'}
r=export(plan,'io-reference')
largest=[max(f.stat().st_size for f in (ROOT/'io-reference'/row['path']).iterdir()) for row in r['rows']]
assert largest[0]<largest[1],largest
limit=largest[0]
def cap_file():
    resource.setrlimit(resource.RLIMIT_FSIZE,(limit,limit))
    signal.signal(signal.SIGXFSZ,signal.SIG_IGN)
dest=ROOT/'io-incomplete'
p=subprocess.run([str(BIN),'experiment',str(ROOT/'io-reference-plan.json'),str(dest)],
    preexec_fn=cap_file,capture_output=True,text=True)
assert p.returncode!=0 and 'File too large' in p.stderr,(p.returncode,p.stderr)
assert load(dest/'manifest.json')['outcome']=='Incomplete'
assert len((dest/'rows.jsonl').read_text().splitlines())==1
assert (dest/'runs/0000/checkpoint.json').read_bytes()==(ROOT/'io-reference/runs/0000/checkpoint.json').read_bytes()
assert not (dest/'summary.json').exists()
events.append(f'Actual EFBIG at per-file cap{limit}: first native run+row preserved, second export stops, manifest Incomplete, no summary')
(ROOT/'runtime-independent-result.json').write_text(json.dumps({'checks':events,'io_error':p.stderr.strip(),'file_size_limit':limit},indent=2))
print('\n'.join(events))
