"""Independent DEC-29 checks, source snapshot 2bc4465; no repository mutations."""
import copy
from fractions import Fraction as F
import hashlib
import json
from pathlib import Path
import random
import subprocess

ROOT = Path('/tmp/chainarium-dec29-evidence')
SRC = Path('/tmp/chainarium-dec29-independent')
BIN = SRC/'target/debug/chainarium'
base = json.loads((SRC/'scenarios/experiments/builder.json').read_text())
events = []

def cmd(*args):
    p=subprocess.run([str(BIN), *map(str,args)], text=True, capture_output=True)
    assert p.returncode==0,(args,p.stderr)
    return p
def load(p): return json.loads(Path(p).read_text())
def wire(x):
    x=F(x)
    return dict(numerator=str(x.numerator), denominator=str(x.denominator), floor_atoms=str(x.numerator//x.denominator))
def available(x): return {'status':'Available','value':wire(x)}
def stats(xs, bits=32768):
    def bounded(x):
        if abs(x.numerator).bit_length()>bits or x.denominator.bit_length()>bits:
            raise OverflowError()
        return x
    def av(fn):
        try: return available(fn())
        except OverflowError: return {'status':'Unavailable','reasons':['ArithmeticLimit']}
    if not xs:
        missing={'status':'Unavailable','reasons':['NoPricedObservations']}
        return {**{k:missing for k in ['minimum','maximum','mean','median','p05','p95','sample_variance']},'negative':'0','zero':'0','positive':'0'}
    n=len(xs); ys=sorted(xs)
    def mean():
        s=F(0)
        for x in xs: s=bounded(s+x)
        return bounded(s/n)
    def var():
        m=mean(); total=F(0)
        for x in xs:
            d=bounded(x-m)
            total=bounded(total+bounded(d*d))
        return bounded(total/(n-1))
    return dict(minimum=av(lambda:bounded(ys[0])), maximum=av(lambda:bounded(ys[-1])),
        mean=av(mean), median=av(lambda:bounded(ys[n//2]) if n%2 else bounded(bounded(ys[n//2-1]+ys[n//2])/2)),
        p05=av(lambda:bounded(ys[(5*n+99)//100-1])), p95=av(lambda:bounded(ys[(95*n+99)//100-1])),
        sample_variance=av(var) if n>1 else {'status':'Unavailable','reasons':['InsufficientSample']},
        negative=str(sum(x<0 for x in xs)),zero=str(sum(x==0 for x in xs)),positive=str(sum(x>0 for x in xs)))
def export(plan,label):
    path=ROOT/(label+'-plan.json'); path.write_text(json.dumps(plan))
    if not (ROOT/label).exists(): cmd('experiment',path,ROOT/label)
    return load(ROOT/label/'manifest.json')
def records(p):
    return [dict(available_at='0',observed_at='0',sequence='0',price=dict(base_asset='X',quote_asset='Q',numerator='1',denominator='1')),
        dict(available_at='10',observed_at='10',sequence='1',price=dict(base_asset='X',quote_asset='Q',numerator=str(p.numerator),denominator=str(p.denominator)))]
def new_case(seed,p):
    case=copy.deepcopy(base['cases'][0]); case['seed']=str(seed)
    for s in case['variants'].values():
        s['seed']=str(seed); s['protocols']={}; s['accounts']['alice']['balances']['X']='10'
        s['scheduler']['rounds']=1
        s['valuation_policy']['reference_feeds']['X']['records']=records(p)
        for i,e in enumerate(s['scheduler']['emissions']):
            e['transaction']['program'][0].update(asset='X',amount=str(i+1))
    return case

# Native + replay equivalence for every demo row, all exported files.
for kind in ['builder','policy']:
    report=load(ROOT/kind/'manifest.json')
    for row in report['rows']:
        source=ROOT/kind/row['path']
        for mode in ['run','resume']:
            target=ROOT/f'{kind}-{mode}-{row["path"].split("/")[-1]}'
            if not target.exists(): cmd(mode,source/('scenario.json' if mode=='run' else 'checkpoint.json'),target)
            assert {f.name:f.read_bytes() for f in source.iterdir()} == {f.name:f.read_bytes() for f in target.iterdir()}
    for variant in report['analysis']['variants']:
        for metric in report['metrics']:
            xs=[F(int(r['metrics'][metric]['value']['numerator']),int(r['metrics'][metric]['value']['denominator'])) for r in report['rows'] if r['variant']==variant]
            assert report['analysis']['variants'][variant][metric]['statistics']==stats(xs)
    events.append(f'{kind}: {len(report["rows"])} native exports equal standalone + replay byte-for-byte; own Fraction statistics pass')

# 64 cases, distinct final rational prices, exact positive/negative/zero effects.
rng=random.Random(291114)
prices=[F(1,3),F(200),F(1),F(125,9)]+[F(rng.randrange(1,1000),rng.randrange(1,80)) for _ in range(60)]
plan=copy.deepcopy(base); plan['cases']=[new_case(i,p) for i,p in enumerate(prices)]
report=export(plan,'fractional-64')
expected={'baseline':[9*p-125 for p in prices],'alternative':[8*p-240 for p in prices]}
for row in report['rows']:
    assert row['metrics']['pnl']==available(expected[row['variant']][int(row['seed'])]),row
for variant,xs in expected.items():
    assert report['analysis']['variants'][variant]['pnl']['statistics']==stats(xs)
deltas=[-p-115 for p in prices]
assert report['analysis']['paired']['alternative']['pnl']['statistics']==stats(deltas)
events.append('64 new cases / 128 runs: independent final-value arithmetic and every statistic pass; negative and zero rows retained')

# Real asymmetric missing cohorts from zero/nonzero end inventory, plus no-op zero.
plan=copy.deepcopy(base); plan['cases']=[]
expect={'baseline':[],'alternative':[]}
for seed in range(5):
    c=new_case(seed,F(1)); plan['cases'].append(c)
    for s in c['variants'].values():
        s['accounts']['alice']['balances']['X']='1'
        s['valuation_policy']['max_age_seconds']='0'
        if seed in [1,2]: s['valuation_policy']['reference_feeds']['X']['records']=records(F(1))[:1]
        if seed==4: s['scheduler']['emissions']=[]
        else:
            for i,e in enumerate(s['scheduler']['emissions']):
                q=(i==1) if seed!=2 else (i==0)
                e['transaction']['program'][0].update(asset='Q' if q else 'X',amount='10' if q else '1')
    expect['baseline'].append([F(-116),F(-116),None,F(-116),F(0)][seed])
    expect['alternative'].append([F(-240),None,F(-231),F(-240),F(0)][seed])
report=export(plan,'asymmetric-missing')
for row in report['rows']:
    expected=expect[row['variant']][int(row['seed'])]
    assert row['metrics']['pnl']['status']==('Available' if expected is not None else 'Unavailable')
    if expected is not None: assert row['metrics']['pnl']==available(expected)
for variant,xs in expect.items():
    cohort=report['analysis']['variants'][variant]['pnl']
    assert cohort['eligible_n']=='4' and cohort['completed_n']=='5'
    assert cohort['statistics']==stats([x for x in xs if x is not None])
c=report['analysis']['paired']['alternative']['pnl']
assert c['eligible_seeds']==['0','3','4']
assert c['statistics']==stats([F(-124),F(-124),F(0)])
assert c['excluded'][0]['reasons'].keys()=={'variant'} and c['excluded'][1]['reasons'].keys()=={'baseline'}
assert report['rows'][-1]['outcomes']['included_failure_fraction']['reasons']==['NoIncludedReceipts']
events.append('Real zero-inventory / Unpriced cohorts: numeric-seed join, both exclusion sides, zero and no-included receipts pass')

# Independent bounded arithmetic oracle: pairwise-prime price denominators force
# aggregate overflow at 512 bits, while individual observations remain available.
def prime(n):
    return n>1 and all(n%d for d in range(2,int(n**0.5)+1))
primes=[n for n in range(1009,3000) if prime(n)][:64]
plan=copy.deepcopy(base); plan['limits']['rational_bits']=512
prices=[F(i+1,p) for i,p in enumerate(primes)]
plan['cases']=[new_case(i,p) for i,p in enumerate(prices)]
report=export(plan,'rational-cap')
for variant,coefficient,constant in [('baseline',9,125),('alternative',8,240)]:
    xs=[coefficient*p-constant for p in prices]
    c=report['analysis']['variants'][variant]['pnl']
    assert c['eligible_n']=='64' and c['statistics']==stats(xs,512)
    assert c['statistics']['mean']['reasons']==['ArithmeticLimit']
assert report['analysis']['paired']['alternative']['pnl']['statistics']==stats([-p-115 for p in prices],512)
events.append('512-bit summary cap: independent per-step Fraction bound agrees; all 128 native observations preserved')

# Included fee / receipt accounting with successful, reverted, exhausted receipts.
plan=copy.deepcopy(base); plan['cases']=plan['cases'][:1]
for s in plan['cases'][0]['variants'].values():
    s['chain']['block_limit']='2000'
    t=copy.deepcopy(s['scheduler']['emissions'][0])
    emissions=[]
    for i in range(5):
        e=copy.deepcopy(t); e['at']=str(i+1);e['producer_sequence']=str(i+1)
        tx=e['transaction'];tx['id']=f'independent-{i}';tx['nonce']=str(i if i<4 else 9)
        tx['program'][0]['amount']='90000' if i==2 else '1'
        if i==3: tx['limit']='110'
        emissions.append(e)
    s['scheduler']['emissions']=emissions
report=export(plan,'mixed-receipts')
for row in report['rows']:
    assert row['outcomes']['included']=={'Success':'2','ProtocolRevert':'1','ResourceExhausted':'1'}
    assert row['outcomes']['included_failure_fraction']==available(F(1,2))
    assert row['metrics']['fees']==available(455)
    assert row['metrics']['pnl']==available(-457)
    native=load(ROOT/'mixed-receipts'/row['path']/'run.json')
    assert int(row['outcomes']['inclusion_rejection_attempts'])==sum(len(b['rejected']) for b in native['blocks'])
events.append('Native mixed receipts: 2 Success + 1 ProtocolRevert + 1 ResourceExhausted, fraction 1/2, fee455, PnL-457; rejection count matches native records')

(ROOT/'independent-checks-result.json').write_text(json.dumps(events,indent=2))
print('\n'.join(events))
