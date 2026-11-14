import copy
import hashlib
import json
from pathlib import Path
import subprocess

ROOT=Path('/tmp/chainarium-dec29-evidence')
SRC=Path('/tmp/chainarium-dec29-independent')
BIN=SRC/'target/debug/chainarium'
def load(p):return json.loads(Path(p).read_text())
def sha(v,sort=False):return hashlib.sha256(json.dumps(v,ensure_ascii=False,separators=(',',':'),sort_keys=sort).encode()).hexdigest()
def remove(v,p):
    parts=[x.replace('~1','/').replace('~0','~') for x in p.split('/')[1:]]
    for k in parts[:-1]:v=v[k]
    del v[parts[-1]]
def diff(a,b,path=''):
    if a==b:return []
    if isinstance(a,dict) and isinstance(b,dict):
        out=[]
        for k in sorted(a.keys()|b.keys()):
            p=path+'/'+k.replace('~','~0').replace('/','~1')
            if k not in a:out.append(dict(pointer=p,new=b[k]))
            elif k not in b:out.append(dict(pointer=p,old=a[k]))
            else:out+=diff(a[k],b[k],p)
        return out
    if isinstance(a,list) and isinstance(b,list):
        out=[]
        for i in range(max(len(a),len(b))):
            p=path+'/'+str(i)
            if i>=len(a):out.append(dict(pointer=p,new=b[i]))
            elif i>=len(b):out.append(dict(pointer=p,old=a[i]))
            else:out+=diff(a[i],b[i],p)
        return out
    return [dict(pointer=path,old=a,new=b)]
checks=[]
for kind in ['builder','policy','fractional-64','asymmetric-missing']:
    r=load(ROOT/kind/'manifest.json')
    assert hashlib.sha256((ROOT/kind/'plan.json').read_bytes()).hexdigest()==r['plan_sha256']
    for c in r['cases']:
        rows={row['variant']:row for row in r['rows'] if row['seed']==c['seed']}
        scenarios={v:load(ROOT/kind/row['path']/'scenario.json') for v,row in rows.items()}
        baseline=scenarios[r['baseline_variant']]
        for v,s in scenarios.items():
            assert sha(s)==c['variants'][v]['scenario_sha256']==rows[v]['run_id']
            common=copy.deepcopy(s);del common['name']
            paths=['/scheduler/builder'] if kind!='policy' else ['/baseline_policies/trader']
            for path in paths:remove(common,path)
            assert sha(common,True)==c['common_input_sha256']
            external=dict(blocks=s['blocks'],emissions=s.get('scheduler',{}).get('emissions'),
                bundle_emissions=s.get('scheduler',{}).get('bundles',{}).get('emissions'),
                reference_feeds=s['valuation_policy']['reference_feeds'])
            assert sha(external,True)==c['external_inputs_sha256']
            assert diff(baseline,s)==c['variants'][v]['differences']
    checks.append(f'{kind}: Plan/Scenario/common/external SHA256 provenance and complete structural diffs independently reproduced')

base=load(SRC/'scenarios/experiments/policy.json')
bad=[]
def candidate(label,mutate):
    p=copy.deepcopy(base);mutate(p);bad.append((label,p))
for cap in base['limits']:
    candidate('zero-'+cap,lambda p,cap=cap:p['limits'].__setitem__(cap,0))
    candidate('raise-'+cap,lambda p,cap=cap:p['limits'].__setitem__(cap,base['limits'][cap]+1))
candidate('lower-case-cap',lambda p:p['limits'].__setitem__('cases',19))
candidate('lower-run-cap',lambda p:p['limits'].__setitem__('runs',39))
candidate('lower-block-cap',lambda p:p['limits'].__setitem__('total_blocks',239))
candidate('lower-metric-cap',lambda p:p['limits'].__setitem__('metrics',1))
candidate('duplicate-seed',lambda p:p['cases'].__setitem__(19,copy.deepcopy(p['cases'][0])))
candidate('unexpected-environment',lambda p:p['cases'][0]['variants']['alternative']['chain'].__setitem__('interval','2'))
candidate('unexpected-account',lambda p:p['cases'][0]['variants']['alternative']['accounts']['alice']['balances'].__setitem__('Q','5001'))
candidate('model-order',lambda p:p['cases'][0]['variants']['alternative']['model_versions'].reverse())
candidate('treatment-by-seed',lambda p:p['cases'][19]['variants']['alternative']['baseline_policies']['trader']['kind'].__setitem__('probability_ppm',123))
candidate('metrics-selector',lambda p:p['metrics'].__setitem__('pnl',{'type':'PaidNativeFees','accounts':['alice']}))
candidate('missing-target',lambda p:p['cases'][0]['variants']['alternative'].__setitem__('baseline_policies',{}))
for label,p in bad:
    file=ROOT/('reject-'+label+'.json');file.write_text(json.dumps(p))
    dest=ROOT/('unexpected-output-'+label)
    result=subprocess.run([str(BIN),'experiment',str(file),str(dest)],text=True,capture_output=True)
    assert result.returncode!=0 and not dest.exists(),label
checks.append(f'{len(bad)} independent invalid-plan mutations rejected before creating any output directory')
(ROOT/'validation-independent-result.json').write_text(json.dumps(checks,indent=2))
print('\n'.join(checks))
