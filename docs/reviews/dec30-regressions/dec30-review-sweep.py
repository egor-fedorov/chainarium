import copy, json, random, subprocess, hashlib, os
from fractions import Fraction as F
from pathlib import Path

REPO=Path('/workspace/chainarium')
ROOT=Path(os.environ.get('DEC30_REVIEW_ROOT','/tmp/dec30-review-sweep'))
ROOT.mkdir(exist_ok=True)
BIN=REPO/'target/release/chainarium'
BASE=json.loads((REPO/'scenarios/demos/sc-hodl-ordered.json').read_text())
COUNTS=dict(cases=0,boundaries=0,flows=0,infeasible=0,asset_limit=0,flow_limit=0,unpriced=0)

def run(*args):
    p=subprocess.run([str(BIN),*map(str,args)],capture_output=True,text=True)
    assert p.returncode==0,(args,p.stderr)

def q(x):
    return None if x is None else F(int(x['numerator']),int(x['denominator']))

def check_wire(obj):
    if isinstance(obj,dict):
        if set(obj)=={'numerator','denominator','floor_atoms'}:
            n=q(obj)
            assert obj==dict(numerator=str(n.numerator),denominator=str(n.denominator),floor_atoms=str(n.numerator//n.denominator))
        else:
            for x in obj.values(): check_wire(x)
    elif isinstance(obj,list):
        for x in obj: check_wire(x)

def px(s,a,t):
    if a=='Q': return F(1)
    p=s['valuation_policy']
    records=[r for r in p['reference_feeds'].get(a,{}).get('records',[]) if int(r['available_at'])<=t]
    if not records:return None
    r=records[-1]
    if not 0<=t-int(r['observed_at'])<=int(p['max_age_seconds']):return None
    return F(int(r['price']['numerator']),int(r['price']['denominator']))

def val(s,hold,t):
    total=F(0)
    for a,n in hold.items():
        if not n:continue
        p=px(s,a,t)
        if p is None:return None
        total+=n*p
    return total

def math(fn,*xs):return None if None in xs else fn(*xs)

def wallet(w,owners):
    hold={}
    for owner in owners:
        for a,n in w['accounts'][owner]['balances'].items():hold[a]=hold.get(a,F(0))+int(n)
    return {a:n for a,n in hold.items() if n}

def check(s,folder):
    native=json.loads((folder/'run.json').read_text())
    reports=[json.loads(x) for x in (folder/'hodl.jsonl').read_text().splitlines()]
    check_wire(reports)
    labels={(b['portfolio'],b['tx_id']) for b in s['valuation_policy']['capital_transfers']}
    hp=s['hodl_policy']; states={}
    for id in hp['portfolios']:
        owners=s['valuation_policy']['portfolios'][id]['accounts']
        hold=wallet(native['initial'],owners)
        fail='AssetLimit' if len(hold)>hp['limits']['assets_per_portfolio'] else None
        states[id]=dict(hold=hold,fail=fail,count=0,initial=val(s,hold,0),incoming=F(0),outgoing=F(0),last=None if fail else dict(type='Boundary',block='0',timestamp='0'),last_hold=hold.copy())
    for i,report in enumerate(reports):
        COUNTS['boundaries']+=1
        block=native['blocks'][i-1] if i else None
        w=block['state'] if block else native['initial']; t=int(w['timestamp'])
        expected={id:[] for id in states}
        if block:
            for ri,r in enumerate(block['receipts']):
                if r['status']!='Success':continue
                for ei,e in enumerate(r['events']):
                    if e['kind']!='Transfer' or int(e['amount'])==0:continue
                    for id,st in states.items():
                        if (id,r['tx_id']) not in labels:continue
                        owners=s['valuation_policy']['portfolios'][id]['accounts']
                        assert (e['from'] in owners)!=(e['to'] in owners)
                        incoming=e['to'] in owners;a=e['asset'];amount=F(int(e['amount']))
                        cursor=dict(block=str(i),timestamp=str(t),receipt_index=ri,event_index=ei,tx_id=r['tx_id'])
                        p=px(s,a,t);f='incoming' if incoming else 'outgoing'
                        st[f]=math(lambda x,y:x+y,st[f],math(lambda x:x*amount,p))
                        st['count']+=1;COUNTS['flows']+=1
                        before=st['hold'].get(a,F(0))
                        if st['fail']:
                            effect=dict(status='Skipped',reason=st['fail'])
                        else:
                            after=before+amount if incoming else before-amount
                            fail='FlowLimit' if st['count']>hp['limits']['flow_events_per_portfolio'] else 'InfeasibleOutflow' if after<0 else 'AssetLimit' if after and a not in st['hold'] and len(st['hold'])==hp['limits']['assets_per_portfolio'] else None
                            if fail:
                                st['fail']=fail;st['failed_cursor']=cursor
                                effect=dict(status='Failed',reason=fail)
                            else:
                                if after:st['hold'][a]=after
                                else:st['hold'].pop(a,None)
                                st['last']=dict(type='Flow',cursor=cursor);st['last_hold']=st['hold'].copy()
                                effect=dict(status='Applied',before=before,after=after)
                        expected[id].append((cursor,effect))
        for p in report['portfolios']:
            id=p['portfolio']; st=states[id]
            actual=val(s,wallet(w,s['valuation_policy']['portfolios'][id]['accounts']),t)
            hold=None if st['fail'] else val(s,st['hold'],t)
            ap=math(lambda v,v0,inc,out:v-v0-inc+out,actual,st['initial'],st['incoming'],st['outgoing'])
            hpnl=math(lambda v,v0,inc,out:v-v0-inc+out,hold,st['initial'],st['incoming'],st['outgoing'])
            for field,n in [('actual',actual),('initial',st['initial']),('actual_pnl',ap),('hodl_pnl',hpnl),('inflows',st['incoming']),('outflows',st['outgoing']),('excess_pnl',math(lambda a,b:a-b,ap,hpnl)),('value_gap',math(lambda a,b:a-b,actual,hold))]:
                assert q(p[field]['value'])==n,(folder,i,id,field,p[field],n)
            assert q(p['hodl']['mark']['value'])==hold
            if hold is None:COUNTS['unpriced']+=1
            assert p['flow_events']==str(st['count'])
            assert len(p['flows'])==len(expected[id])
            for f,(cursor,effect) in zip(p['flows'],expected[id]):
                assert f['cursor']==cursor
                e=f['effect'].copy()
                if e['status']=='Applied':e.update(before=q(e['before']),after=q(e['after']))
                assert e==effect
            l=p['ledger']
            if st['fail']:
                assert l['status']=='Unavailable' and l['failure']['reason']==st['fail']
                COUNTS[{'AssetLimit':'asset_limit','FlowLimit':'flow_limit','InfeasibleOutflow':'infeasible'}[st['fail']]]+=1
                if 'failed_cursor' in st:assert l['failure']['at']==dict(type='Flow',cursor=st['failed_cursor'])
                if st['last'] is None:assert l['last_feasible'] is None and p['initial_holdings'] is None
                else:
                    assert l['last_feasible']['at']==st['last']
                    assert {a:q(x) for a,x in l['last_feasible']['quantities'].items()}==st['last_hold']
            else:
                st['last']=dict(type='Boundary',block=str(i),timestamp=str(t));st['last_hold']=st['hold'].copy()
                assert l['status']=='Ready' and l['current']['at']==st['last']
                assert {a:q(x) for a,x in l['current']['quantities'].items()}==st['hold']
        provenance=report['provenance']
        assert provenance['build_id']==native['build_id'] and provenance['run_id']==native['run_id']
        assert provenance['policy_sha256']==hashlib.sha256((folder/'hodl-policy.json').read_bytes()).hexdigest()
        assert provenance['valuation_policy_sha256']==hashlib.sha256((folder/'valuation-policy.json').read_bytes()).hexdigest()
    COUNTS['cases']+=1

for index in range(96):
    rng=random.Random(30000+index);s=copy.deepcopy(BASE);s['name']=f'dec30 independent {index}';s['seed']=str(index)
    s['assets']={a:{'decimals':0} for a in ['Q','X','Y','Z']}
    for name,account in s['accounts'].items():
        account['balances']={a:str(10000 if a=='Q' and name in ['alice','bob'] else rng.choice([0,0,1,3,7]) if name in ['alice','bob'] else 0) for a in s['assets']}
    vp=s['valuation_policy'];vp['portfolios']={'a':{'accounts':['alice']},'b':{'accounts':['bob']},'both':{'accounts':['alice','bob']}}
    vp['max_age_seconds']=str(rng.choice([0,1,2,60]));vp['reference_feeds']={};vp['capital_transfers']=[]
    for a in ['X','Y','Z']:
        if rng.randrange(8)==0:continue
        records=[]
        for seq,avail in enumerate([0,2,4,6]):
            if rng.randrange(7)==0:continue
            records.append(dict(available_at=str(avail),observed_at=str(max(0,avail+rng.choice([-2,0,0,1]))),sequence=str(seq),price=dict(base_asset=a,quote_asset='Q',numerator=str(rng.randrange(1,25)),denominator=str(rng.choice([1,2,3,7,11])))))
        vp['reference_feeds'][a]=dict(source_id='independent-'+a,records=records)
    s['hodl_policy']['portfolios']=['both','b','a']
    s['hodl_policy']['limits'].update(assets_per_portfolio=4 if index%3 else rng.choice([1,2,3]),flow_events_per_portfolio=4096 if index%4 else rng.choice([1,3,8]),rational_bits=512)
    s['blocks']=[];nonces={'alice':0,'bob':0}
    for bi in range(6):
        txs=[]
        for j in range(4):
            sender=rng.choice(['alice','bob']);to='bob' if sender=='alice' else 'alice'
            txid=f'{24-bi*4-j:02}-order';asset=rng.choice(list(s['assets']))
            tx=dict(id=txid,agent='trader' if sender=='alice' else 'recipient',sender=sender,chain='lab-1',nonce=str(nonces[sender]),limit='200',price=str(rng.choice([0,0,1])),deadline='100',program=[dict(type='Transfer',to=to,asset=asset,amount=str(rng.randrange(1,12)))])
            nonces[sender]+=1;txs.append(tx)
            for id in ['a','b']:
                if rng.randrange(4):vp['capital_transfers'].append(dict(portfolio=id,tx_id=txid))
        s['blocks'].append(txs)
    path=ROOT/f'case-{index:03}.json';path.write_text(json.dumps(s));folder=ROOT/f'case-{index:03}'
    run('run',path,folder);check(s,folder)
    if index%16==0:print(index,COUNTS,flush=True)
print('PASS',COUNTS)
(ROOT/'results.json').write_text(json.dumps(COUNTS,indent=2))
