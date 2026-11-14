import copy,json,subprocess,hashlib,os
from pathlib import Path
from fractions import Fraction as F
REPO=Path('/workspace/chainarium');ROOT=Path(os.environ.get('DEC30_REVIEW_ROOT','/tmp/dec30-review-native'));ROOT.mkdir(exist_ok=True)
BIN=REPO/'target/release/chainarium'
def cmd(*args,ok=True):
    r=subprocess.run([str(BIN),*map(str,args)],capture_output=True,text=True)
    assert (r.returncode==0)==ok,(args,r.stderr)
    return r
def execute(name,s):
    path=ROOT/(name+'.json');path.write_text(json.dumps(s));out=ROOT/name;cmd('run',path,out)
    return path,out,json.loads((out/'run.json').read_text())
def value(m):return None if m['value'] is None else F(int(m['value']['numerator']),int(m['value']['denominator']))
s=json.loads((REPO/'scenarios/demos/sc-hodl-ordered.json').read_text());s['name']='independent mark arithmetic recovery';s['blocks']=[[],[]]
s['valuation_policy']['capital_transfers']=[]
s['assets']={a:{'decimals':0} for a in ['Q','X','Y','Z']}
for id,a in s['accounts'].items():a['balances']={x:str(int(id=='alice' and x!='Q')) for x in s['assets']}
s['valuation_policy']['reference_feeds']={}
denoms=[(1<<e)-1 for e in [193,197,199]]
overflow=sum((F(1,d) for d in denoms),F(0));assert overflow.denominator.bit_length()>512
for a,d in zip(['X','Y','Z'],denoms):
    s['valuation_policy']['reference_feeds'][a]=dict(source_id='review',records=[dict(available_at=str(t),observed_at=str(t),sequence=str(t),price=dict(base_asset=a,quote_asset='Q',numerator='1',denominator=str(d if t==1 else 1))) for t in range(3)])
s['hodl_policy']['limits']['rational_bits']=512
path,out,native=execute('arithmetic-recovery',s);bs=[json.loads(x) for x in (out/'hodl.jsonl').read_text().splitlines()]
assert value(bs[0]['portfolios'][0]['hodl']['mark'])==3
p=bs[1]['portfolios'][0]
assert p['ledger']['status']=='Ready' and p['hodl']['mark']['value'] is None and 'ArithmeticLimit' in p['hodl']['mark']['reasons']
assert value(p['actual'])==overflow and p['excess_pnl']['value'] is None
p=bs[2]['portfolios'][0];assert p['ledger']['status']=='Ready' and value(p['hodl']['mark'])==3 and value(p['excess_pnl'])==0
print('PASS native 589-bit mark overflow at t1, ledger Ready, full recovery at t2',flush=True)

for name in ['sc-agent-random','sc-hodl-infeasible','sc-hodl-credit','sc-hodl-ordered','sc-hodl-lp']:
    s=json.loads((REPO/f'scenarios/demos/{name}.json').read_text())
    if 'hodl_policy' not in s:
        template=json.loads((REPO/'scenarios/demos/sc-hodl-ordered.json').read_text())
        s['hodl_policy']=copy.deepcopy(template['hodl_policy']);s['valuation_policy']=copy.deepcopy(template['valuation_policy']);s['valuation_policy']['capital_transfers']=[]
        s['model_versions']+=['reference-mtm-v1','flow-matched-hodl-v1']
    path,out,full=execute(name+'-on',s)
    control=copy.deepcopy(s);control.pop('hodl_policy');control['model_versions'].remove('flow-matched-hodl-v1')
    _,offout,off=execute(name+'-off',control)
    for key in ['initial','state','blocks','rng','scheduler']:
        assert full.get(key)==off.get(key),(name,key)
    assert not (offout/'hodl.jsonl').exists()
    changed=copy.deepcopy(s);changed['hodl_policy']['limits'].update(assets_per_portfolio=1,flow_events_per_portfolio=1,rational_bits=512)
    changed['valuation_policy']['reference_feeds']={}
    _,_,other=execute(name+'-changed',changed)
    for key in ['initial','state','blocks','rng','scheduler']:
        assert full.get(key)==other.get(key),(name,key)
    for boundary in range(len(full['blocks'])+1):
        cp=ROOT/f'{name}-{boundary}.checkpoint.json';cmd('checkpoint',path,boundary,cp)
        resumed=ROOT/f'{name}-resumed-{boundary}';cmd('resume',cp,resumed)
        assert {f.name for f in out.iterdir()}=={f.name for f in resumed.iterdir()}
        for f in out.iterdir():assert f.read_bytes()==(resumed/f.name).read_bytes(),(name,boundary,f.name)
    print('PASS native noninterference and every checkpoint',name,flush=True)

plan=json.loads((REPO/'scenarios/experiments/builder.json').read_text());plan['cases']=plan['cases'][:1]
policy=json.loads((REPO/'scenarios/demos/sc-hodl-ordered.json').read_text())['hodl_policy']
for variant in plan['cases'][0]['variants'].values():variant['hodl_policy']=copy.deepcopy(policy);variant['model_versions'].append('flow-matched-hodl-v1')
path=ROOT/'paired.json';path.write_text(json.dumps(plan));cmd('experiment',path,ROOT/'paired')
manifest=json.loads((ROOT/'paired/manifest.json').read_text())
for f in (ROOT/'paired/runs').iterdir():
    assert (f/'hodl.jsonl').exists() and (f/'hodl-policy.json').exists()
    cp=f/'checkpoint.json'; restored=ROOT/('paired-restored-'+f.name);cmd('resume',cp,restored)
    assert (f/'hodl.jsonl').read_bytes()==(restored/'hodl.jsonl').read_bytes()
bad=copy.deepcopy(plan);bad['cases'][0]['variants']['alternative']['hodl_policy']['limits']['flow_events_per_portfolio']=1
bp=ROOT/'paired-mismatch.json';bp.write_text(json.dumps(bad));r=cmd('experiment',bp,ROOT/'paired-mismatch',ok=False)
assert 'undeclared experiment difference' in r.stderr and not (ROOT/'paired-mismatch').exists()
print('PASS DEC29 native HODL exports, checkpoint equality, changed common policy rejected before output')
