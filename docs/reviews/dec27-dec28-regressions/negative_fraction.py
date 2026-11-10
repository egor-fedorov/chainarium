import differential as d
import json,copy
s=json.loads((d.S/'scenarios/demos/sc-05-credit.json').read_text());s['blocks']=s['blocks'][:1]
s['assets']['R']={'decimals':0}
for a in s['accounts'].values():a['balances']['R']=a['balances']['Q'];a['balances']['Q']='0'
s['lending']['credit']['debt_asset']='R';s['feeds']['feed']['quote_asset']='R';s['feeds']['feed']['initial']['price']['quote_asset']='R'
s['blocks'][0][1]['program'].append(dict(type='Transfer',to='alice',asset='R',amount='80'))
p=copy.deepcopy(json.loads((d.S/'scenarios/demos/sc-mtm-capital.json').read_text())['valuation_policy']);p['capital_transfers']=[];p['portfolios']={'borrower':{'accounts':['bob']}}
p['reference_feeds']={asset:dict(source_id='independent-'+asset,records=[dict(available_at='0',observed_at='0',sequence='0',price=dict(base_asset=asset,quote_asset='Q',numerator='1',denominator=str(denom)))]) for asset,denom in [('X',12),('R',80)]}
s['valuation_policy']=p;s['model_versions'].append('reference-mtm-v1')
r,vs=d.run(s,'negative-half-live-position');v=vs[-1]['portfolios'][0]
assert v['current']['mark']['value']==dict(numerator='-1',denominator='2',floor_atoms='-1'),v
assert v['pnl']['value']==dict(numerator='-1',denominator='1',floor_atoms='-1')
s['blocks'][0][1]['program'].pop();p['reference_feeds'].pop('R')
r,vs=d.run(s,'gross-offset-live-position');v=vs[-1]['portfolios'][0]
assert v['current']['mark']['status']=='Unpriced' and 'MissingPrice' in v['current']['mark']['reasons']
assert v['current']['mark']['priced_part']==dict(numerator='1',denominator='2',floor_atoms='0')
print('PASS: live lending portfolio V=-1/2, floor=-1; gross offset +80R/-80R remains Unpriced with missing price.')
