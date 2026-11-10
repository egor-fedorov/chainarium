import copy, json, math, random, subprocess, pathlib
from fractions import Fraction as F
R=pathlib.Path('/tmp/dec27-dec28-independent-review')
S=R/'source'
BIN=S/'target/debug/chainarium'
OUT=R/'differential'
OUT.mkdir(exist_ok=True)
rng=random.Random(2728)

def run(s,name):
    inp=OUT/(name+'.json'); inp.write_text(json.dumps(s))
    out=OUT/name
    if not out.exists():
        p=subprocess.run([str(BIN),'run',str(inp),str(out)],capture_output=True,text=True)
        assert p.returncode==0,(name,p.stderr)
    return json.loads((out/'run.json').read_text()), [json.loads(l) for l in (out/'valuations.jsonl').read_text().splitlines()] if (out/'valuations.jsonl').exists() else None

def frac(v):
    q=F(int(v['numerator']),int(v['denominator']))
    assert str(q.numerator)==v['numerator'] and str(q.denominator)==v['denominator']
    assert str(q.numerator//q.denominator)==v['floor_atoms']
    return q

def quote(x,y,n,fee): return n*(10000-fee)*y//(x*10000+n*(10000-fee))
base=json.loads((S/'scenarios/demos/sc-flash-arb-success.json').read_text())
for case in range(160):
    s=copy.deepcopy(base)
    reserves=[]
    for aid,pid in [('pool','cpmm'),('pool-b','second')]:
        x,y=rng.randint(10,10**7),rng.randint(10,10**7)
        fee=rng.choice([0,1,5,30,100,9999])
        s['accounts'][aid]['balances']={'X':str(x),'Q':str(y)}
        s['protocols'][pid]['fee_bps']=fee
        reserves.append((x,y,fee))
    p=s['scheduler']['flash_arbitrage']['policies']['trader']
    n=rng.randint(1,1000000); price=rng.choice([0,1,5]); limit=rng.choice([179,180,189,190,191,300]); own=rng.randint(0,2000)
    minimum=rng.randint(1,1000); fb=rng.choice([0,1,5,9999]); cash=rng.choice([n-1,n,n+1,10**8])
    p.update(principal=str(n),resource_price=str(price),resource_limit=str(limit),minimum_net_profit=str(minimum))
    s['flash_providers']['loan']['fee_bps']=fb
    s['accounts']['loan']['balances']['Q']=str(cash)
    s['accounts']['alice']['balances']['Q']=str(own)
    fee=(n*fb+9999)//10000; expected=[]
    if cash>=n and own>=price*limit:
        for ai,bi,path in [(0,1,['cpmm','second']),(1,0,['second','cpmm'])]:
            x,y,f=reserves[ai]; bx,by,bf=reserves[bi]
            first=quote(y,x,n,f); last=quote(bx,by,first,bf)
            net=last-n-fee-190*price
            if first and last and limit>=190 and own-limit*price+last>=n+fee and net>=minimum:
                expected.append((net,path,first,last))
    rr,_=run(s,f'flash-{case:03}')
    d=rr['blocks'][0]['scheduler']['flash_arbitrage']['decisions'][0]
    if expected:
        best=sorted(expected,key=lambda x:(-x[0],x[1]))[0]
        assert d['action']=='Submit',(case,best,d)
        c=d['candidates'][d['selected']]
        assert int(c['net'])==best[0] and [h['venue'] for h in c['hops']]==best[1],(case,c,best)
        assert int(rr['state']['accounts']['alice']['balances']['Q'])==own+best[0]
        assert int(rr['state']['accounts']['loan']['balances']['Q'])==cash+fee
        assert int(rr['blocks'][0]['receipts'][0]['used'])==190
    else:
        assert d['action']=='Skip',(case,d)
        assert rr['state']['accounts']==rr['initial']['accounts'],case

base=json.loads((S/'scenarios/demos/sc-mtm-capital.json').read_text())
for case in range(100):
    s=copy.deepcopy(base); s['blocks']=[]; s['valuation_policy']['capital_transfers']=[]
    s['valuation_policy']['portfolios']={'both':{'accounts':['alice','bob']},'single':{'accounts':['alice']}}
    feed=s['valuation_policy']['reference_feeds']['X']
    a,b=rng.randrange(1,1<<180),rng.randrange(1,1<<180)
    feed['records']=[dict(available_at='0',observed_at='0',sequence='0',price=dict(base_asset='X',quote_asset='Q',numerator=str(a),denominator=str(b)))]
    totals={}
    for owner in ['alice','bob']:
        x,q=rng.randrange(1<<180),rng.randrange(1<<180)
        s['accounts'][owner]['balances']={'X':str(x),'Q':str(q)}
        totals[owner]=F(x*a,b)+q
    rr,v=run(s,f'wallet-{case:03}')
    by={p['portfolio']:p for p in v[0]['portfolios']}
    assert frac(by['both']['current']['mark']['value'])==sum(totals.values())
    assert frac(by['single']['current']['mark']['value'])==totals['alice']
    assert all(frac(p['pnl']['value'])==0 for p in by.values())

# Independent extraction from canonical exports, using Fraction and integer CLMM math.
def balance(w,a,x):return int(w['accounts'][a]['balances'].get(x,0))
def sqrt_tick(t):
    n,d=(10001**abs(t),10000**abs(t)) if t>=0 else (10000**abs(t),10001**abs(t))
    return math.isqrt(((1<<192)*n)//d)
def signed(v):return int(v['magnitude'])*(-1 if v['negative'] else 1)
def components(s,w,owners):
    result={}
    def put(a,k,p,pos,asset,n):result[(a,k,p,str(pos),asset)]=F(n)
    for a in owners:
        for x,n in w['accounts'][a]['balances'].items():put(a,'Wallet','','',x,int(n))
    pools={**s['protocols'],**{k:v['pool'] for k,v in s.get('stable_pools',{}).items()}}
    for pid,shares in w['lp_balances'].items():
        p=pools[pid]
        for a,n in shares.items():
            if a in owners:
                for x in [p['x'],p['y']]:put(a,'StableLP' if pid in s.get('stable_pools',{}) else 'CpmmLP',pid,'',x,F(int(n)*balance(w,p['account'],x),int(w['shares'][pid])) if int(n) else 0)
    for pid,c in w.get('clmm',{}).items():
        cfg=s['clmm_pools'][pid]
        for key,p in c['positions'].items():
            if p['owner'] not in owners:continue
            lo,hi=sqrt_tick(p['lower']),sqrt_tick(p['upper']); cur=max(lo,min(hi,int(c['sqrt_price']))); liq=int(p['liquidity'])
            claims=[liq*(1<<96)*(hi-cur)//(hi*cur),liq*(cur-lo)//(1<<96)]
            for i,asset in enumerate([cfg['x'],cfg['q']]):
                fees=int(p['owed'][i])
                if liq:
                    growth=int(c['fee_growth'][i]); lower=int(c['ticks'][str(p['lower'])]['outside'][i]); upper=int(c['ticks'][str(p['upper'])]['outside'][i])
                    below=lower if c['current_tick']>=p['lower'] else growth-lower
                    above=upper if c['current_tick']<p['upper'] else growth-upper
                    fees+=liq*(growth-below-above-signed(p['last_inside'][i]))//(1<<128)
                put(p['owner'],'ClmmClaim',pid,key,asset,claims[i]+fees)
    for pid,c in w.get('books',{}).items():
        cfg=s['books'][pid]
        for key,p in c['orders'].items():
            if p['owner'] not in owners:continue
            sell=p['side']=='Sell'; asset=cfg['base_asset'] if sell else cfg['quote_asset']
            qty=int(p['remaining_lots'])*(int(cfg['lot_atoms']) if sell else int(p['price_ticks'])*int(cfg['quote_tick_atoms']))
            put(p['owner'],'BookEscrow',pid,key,asset,qty)
    for pid,c in w.get('lending',{}).items():
        cfg=s['lending'][pid]
        for a,n in c['deposit_shares'].items():
            if a in owners:put(a,'DepositClaim',pid,'',cfg['debt_asset'],int(n)*int(c['claims'])//int(c['total_shares']) if int(n) else 0)
        for a,pos in c['positions'].items():
            if a in owners:
                put(a,'Collateral',pid,'',cfg['collateral_asset'],balance(w,cfg['positions'][a],cfg['collateral_asset']))
                put(a,'Debt',pid,'',cfg['debt_asset'],-int(pos['debt']))
    return result
checked=0
for name in ['capital','unpriced','flash','credit','clmm','book']:
    s=json.loads((S/f'scenarios/demos/sc-mtm-{name}.json').read_text()); rr,vs=run(s,'extract-'+name)
    for w,v in zip([rr['initial']]+[b['state'] for b in rr['blocks']],vs):
        for p in v['portfolios']:
            expected=components(s,w,p['accounts'])
            actual={tuple(c['key'][k] for k in ['account','kind','protocol','position','asset']):frac(c['quantity']) for c in p['current']['components']}
            assert expected==actual,(name,w['block'],expected,actual)
            checked+=len(actual)
print('PASS: 160 independent two-CPMM flash scenarios; 100 180-bit exact wallet portfolios; '+str(checked)+' extracted components across all six MTM demos.')
