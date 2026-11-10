import differential as d
from fractions import Fraction as F
import json,copy,subprocess
S,R,OUT=d.S,d.R,d.OUT
# Regenerate pre-implementation references independently of Rust.
for stem in ['flash-arbitrage','valuation']:
 actual=json.loads(subprocess.check_output(['python3',str(S/f'docs/reviews/{stem}-reference.py')]))
 expected=json.loads((S/f'docs/reviews/{stem}-reference.json').read_text())
 assert actual==expected,stem

def price(s,asset,t):
 p=s['valuation_policy']
 if asset=='Q':return F(1)
 f=p['reference_feeds'].get(asset)
 if f is None:return None
 candidates=[r for r in f['records'] if int(r['available_at'])<=t]
 if not candidates:return None
 r=max(candidates,key=lambda r:(int(r['available_at']),int(r['sequence'])))
 age=t-int(r['observed_at'])
 if age<0 or age>int(p['max_age_seconds']):return None
 return F(int(r['price']['numerator']),int(r['price']['denominator']))

def check(s,name):
 rr,vs=d.run(s,name); initial={}; inc={};out={}
 for index,(w,v) in enumerate(zip([rr['initial']]+[b['state'] for b in rr['blocks']],vs)):
  for p in v['portfolios']:
   ident=p['portfolio'];expected=d.components(s,w,p['accounts']);actual={tuple(c['key'][k] for k in ['account','kind','protocol','position','asset']):d.frac(c['quantity']) for c in p['current']['components']}
   assert expected==actual,(name,index,expected,actual)
   val=F(0);missing=False
   for key,qty in expected.items():
    px=price(s,key[-1],int(w['timestamp'])) if qty else F(0)
    if px is None:missing=True
    else:val+=qty*px
   mark=p['current']['mark']
   assert d.frac(mark['priced_part'])==val,(name,index,mark,val)
   assert (mark['value'] is None)==missing,(name,index,mark)
   if not missing:assert d.frac(mark['value'])==val
   if index==0:initial[ident]=None if missing else val; inc[ident]=F(0);out[ident]=F(0)
   for flow in v['capital_flows']:
    if flow['portfolio']!=ident:continue
    px=price(s,flow['asset'],int(v['timestamp']))
    group=inc if flow['direction']=='Inflow' else out
    if px is None:assert flow['mark']['value'] is None;group[ident]=None
    else:
     assert d.frac(flow['mark']['value'])==px*int(flow['amount'])
     if group[ident] is not None:group[ident]+=px*int(flow['amount'])
   unavailable=missing or any(x[ident] is None for x in [initial,inc,out])
   assert (p['pnl']['value'] is None)==unavailable
   if not unavailable:assert d.frac(p['pnl']['value'])==val-initial[ident]-inc[ident]+out[ident]
 return rr,vs
for name in ['capital','unpriced','flash','credit','clmm','book']:
 s=json.loads((S/f'scenarios/demos/sc-mtm-{name}.json').read_text());check(s,'extract-'+name)
s=json.loads((S/'scenarios/demos/sc-stable.json').read_text())
v=copy.deepcopy(json.loads((S/'scenarios/demos/sc-mtm-capital.json').read_text())['valuation_policy'])
v['portfolios']={'both':{'accounts':['alice','lp']},'a':{'accounts':['alice']},'l':{'accounts':['lp']}}
v['capital_transfers']=[];v['reference_feeds']['R']=v['reference_feeds'].pop('X')
v['reference_feeds']['R']['records']=[dict(available_at='0',observed_at='0',sequence='0',price=dict(base_asset='R',quote_asset='Q',numerator='2',denominator='3'))]
s['valuation_policy']=v;s['model_versions'].append('reference-mtm-v1');check(s,'extract-stable')
# Future/newest/stale records: a valid old quote cannot rescue latest bad quote.
s=json.loads((S/'scenarios/demos/sc-mtm-capital.json').read_text());s['blocks']=[[] for _ in range(12)]
s['accounts']['alice']['balances']={'X':'1','Q':'0'};p=s['valuation_policy'];p['capital_transfers']=[];p['max_age_seconds']='2'
p['reference_feeds']['X']['records']=[dict(available_at='0',observed_at='0',sequence='0',price=dict(base_asset='X',quote_asset='Q',numerator='1',denominator='2')),dict(available_at='1',observed_at='8',sequence='1',price=dict(base_asset='X',quote_asset='Q',numerator='2',denominator='3'))]
rr,vs=check(s,'latest-record-boundaries')
assert vs[1]['portfolios'][0]['current']['mark']['reasons']==['FutureObservation']
assert vs[8]['portfolios'][0]['current']['mark']['status']=='Priced'
assert vs[10]['portfolios'][0]['current']['mark']['status']=='Priced'
assert vs[11]['portfolios'][0]['current']['mark']['reasons']==['Stale']
# Literal transfer commits only if the whole VisibleTxId bundle commits.
s=json.loads((S/'scenarios/demos/sc-bundle-visible.json').read_text());s['scheduler']['rounds']=1
s['scheduler']['builder']={'type':'FixedItems','priority':[{'type':'Bundle','id':'visible-group'}]}
s['scheduler']['emissions'][0]['transaction']['program']=[{'type':'Transfer','to':'alice','asset':'Q','amount':'20'}]
b=s['scheduler']['bundles']['emissions'][0]['bundle'];b['members'].pop(1);b['members'][1]['transaction']['nonce']='0'
s['valuation_policy']=copy.deepcopy(json.loads((S/'scenarios/demos/sc-mtm-capital.json').read_text())['valuation_policy']);s['valuation_policy']['capital_transfers']=[{'portfolio':'owner','tx_id':'victim'}];s['model_versions'].append('reference-mtm-v1')
rr,vs=check(s,'visible-capital-commit');assert len(vs[1]['capital_flows'])==1
b['members'][1]['transaction']['nonce']='2';rr,vs=check(s,'visible-capital-rollback');assert len(vs[1]['capital_flows'])==0;assert rr['state']['accounts']==rr['initial']['accounts']
# Own alias rejection even before runtime resolves or chooses a group.
b['members'].append({'type':'Own','transaction':copy.deepcopy(s['scheduler']['emissions'][0]['transaction'])})
f=OUT/'own-capital-alias.json';f.write_text(json.dumps(s));r=subprocess.run([str(d.BIN),'run',str(f),str(OUT/'own-capital-alias-result')],capture_output=True,text=True)
assert r.returncode!=0 and 'ambiguous' in r.stderr,r.stderr
print('PASS: reference JSON regeneration; independent all-position quantities/valuation/PnL/flow prices including StableSwap; newest/future/stale edges; exact bundle commit/rollback and Own alias rejection.')
