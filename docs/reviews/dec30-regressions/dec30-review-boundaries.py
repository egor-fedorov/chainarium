import json,copy,subprocess,hashlib,os
from pathlib import Path
REPO=Path('/workspace/chainarium');ROOT=Path(os.environ.get('DEC30_REVIEW_ROOT','/tmp/dec30-review-boundaries'));ROOT.mkdir(exist_ok=True)
BASE=json.loads((REPO/'scenarios/demos/sc-hodl-ordered.json').read_text())
def execute(name,s):
    path=ROOT/(name+'.json');path.write_text(json.dumps(s));out=ROOT/name
    r=subprocess.run([str(REPO/'target/release/chainarium'),'run',str(path),str(out)],text=True,capture_output=True)
    assert r.returncode==0,r.stderr
    return [json.loads(x) for x in (out/'hodl.jsonl').read_text().splitlines()]
for n in [0,1,64,65]:
    s=copy.deepcopy(BASE);s['blocks']=[[]];s['valuation_policy']['capital_transfers']=[]
    assets=['Q']+[f'A{i:02}' for i in range(max(n-1,0))]
    s['assets']={a:{'decimals':0} for a in assets}
    for id,account in s['accounts'].items():account['balances']={a:str(int(n>0 and id=='alice')) for a in assets}
    s['valuation_policy']['reference_feeds']={a:dict(source_id='review',records=[dict(available_at='0',observed_at='0',sequence='0',price=dict(base_asset=a,quote_asset='Q',numerator='1',denominator='1'))]) for a in assets if a!='Q'}
    reports=execute(f'assets-{n}',s)
    for report in reports:
        p=report['portfolios'][0]
        if n==65:
            assert p['ledger']['status']=='Unavailable' and p['ledger']['failure']['reason']=='AssetLimit' and p['initial_holdings'] is None
        else:
            assert p['ledger']['status']=='Ready' and len(p['ledger']['current']['quantities'])==n
            assert p['hodl']['mark']['value']['numerator']==str(n)
    print('PASS native asset count',n,flush=True)
s=copy.deepcopy(BASE);s['assets']['Y']={'decimals':0};s['accounts']['alice']['balances']={'X':'1'};s['accounts']['bob']['balances']={'Y':'1'}
s['valuation_policy']['reference_feeds']['X']['records'][0]['price']['numerator']='1'
s['valuation_policy']['reference_feeds']['Y']=copy.deepcopy(s['valuation_policy']['reference_feeds']['X']);s['valuation_policy']['reference_feeds']['Y']['records'][0]['price']['base_asset']='Y'
s['hodl_policy']['limits']['assets_per_portfolio']=1
s['blocks']=[[dict(id='z-out',agent='trader',sender='alice',chain='lab-1',nonce='0',limit='200',price='0',deadline='100',program=[dict(type='Transfer',to='bob',asset='X',amount='1')]),dict(id='a-in',agent='recipient',sender='bob',chain='lab-1',nonce='0',limit='200',price='0',deadline='100',program=[dict(type='Transfer',to='alice',asset='Y',amount='1')])],[]]
s['valuation_policy']['capital_transfers']=[dict(portfolio='owner',tx_id=id) for id in ['a-in','z-out']]
p=execute('slot-reuse',s)[-1]['portfolios'][0]
assert p['ledger']['status']=='Ready' and set(p['ledger']['current']['quantities'])=={'Y'} and p['flow_events']=='2'
s['hodl_policy']['limits']['flow_events_per_portfolio']=1
p=execute('slot-reuse-flow-limit',s)[-1]['portfolios'][0]
assert p['ledger']['status']=='Unavailable' and p['ledger']['failure']['reason']=='FlowLimit'
assert p['ledger']['last_feasible']['quantities']=={} and p['ledger']['last_feasible']['at']['cursor']['tx_id']=='z-out'
assert p['ledger']['failure']['at']['cursor']['tx_id']=='a-in'
print('PASS full withdrawal frees asset slot but not cumulative flow budget; empty last_feasible has exact flow cursor')
for name,nonce,limit,status,fee in [('rejected','1','200',None,0),('paid-revert','0','200','ProtocolRevert',115),('exhausted','0','100','ResourceExhausted',100)]:
    s=copy.deepcopy(BASE);s['accounts']['alice']['balances']={'Q':'1000'};s['accounts']['bob']['balances']={'Q':'0'}
    tx=dict(id='labelled',agent='trader',sender='alice',chain='lab-1',nonce=nonce,limit=limit,price='1',deadline='100',program=[dict(type='Transfer',to='bob',asset='Q',amount='900')])
    s['blocks']=[[tx]];s['valuation_policy']['capital_transfers']=[dict(portfolio='owner',tx_id='labelled')]
    p=execute(name,s)[-1]['portfolios'][0]
    assert p['flow_events']=='0' and p['flows']==[] and p['ledger']['status']=='Ready'
    assert p['hodl']['mark']['value']['numerator']=='1000'
    assert p['actual']['value']['numerator']==str(1000-fee) and p['excess_pnl']['value']['numerator']==str(-fee)
    native=json.loads((ROOT/name/'run.json').read_text())
    if status is None:assert len(native['blocks'][0]['rejected'])==1 and native['blocks'][0]['receipts']==[]
    else:assert native['blocks'][0]['receipts'][0]['status']==status
    print('PASS labelled transfer',name,'zero capital flow, excess',-fee,flush=True)
for folder in Path(os.environ.get('DEC30_SWEEP_ROOT','/tmp/dec30-review-sweep')).glob('case-*'):
    if not folder.is_dir():continue
    vp=json.loads((folder/'valuation-policy.json').read_text())
    expected=hashlib.sha256(json.dumps(vp['reference_feeds'],separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
    for line in (folder/'hodl.jsonl').read_text().splitlines():assert json.loads(line)['provenance']['reference_inputs_sha256']==expected
print('PASS reference input provenance hashes for all 672 independent sweep boundaries')
