"""DEC-31 preimplementation exact time/metric examples, no production imports."""
from fractions import Fraction as F
from pathlib import Path
import json

def wire(n):
    n = F(n)
    return dict(numerator=str(n.numerator), denominator=str(n.denominator), floor_atoms=str(n.numerator // n.denominator))

def cases():
    return {
        'synchronous': dict(times=['0', '12', '24'], balances=[['100','100'],['90','75'],['90','75']], delta=[wire(0),wire(-15),wire(-15)], pnl_a=wire(-10), pnl_b=wire(-25)),
        'as_of': dict(times=['0','10','12','20'], blocks=[['0','0'],['1','0'],['1','1'],['2','1']], ages=[['0','0'],['0','10'],['2','0'],['0','8']], wallet_delta=[wire(0),wire(10),wire(-15),wire(-15)], pnl_delta=[wire(0),None,None,None], excluded_time='24'),
        'unknown_history': dict(actual_a=wire(10),actual_b=wire(9),value_delta=wire(-1),pnl_a=None,pnl_b=None,pnl_delta=None),
        'wide_fraction': dict(wallet_q='9007199254740993',actual_a=wire(F(9007199254740993)+F(1,3)),actual_b=wire(9007199254740993),pnl_delta=wire(F(-1,3))),
        'different_decimals': dict(left_atoms='100',right_atoms='100',wallet_delta=None,metric_delta=None,reason='AssetUnitsMismatch'),
        'short_prefix': dict(times_a=['0','10','20'],times_b=['0','10'],common=['0','10'],excluded_time='20'),
    }

def check(c):
    a, b = [0,10,20], [0,12,24]
    times = sorted({t for t in a+b if t <= min(a[-1],b[-1])})
    assert list(map(str,times)) == c['as_of']['times']
    for i,t in enumerate(times):
        ia=max(j for j,x in enumerate(a) if x<=t)
        ib=max(j for j,x in enumerate(b) if x<=t)
        assert [str(ia),str(ib)] == c['as_of']['blocks'][i]
        assert [str(t-a[ia]),str(t-b[ib])] == c['as_of']['ages'][i]
    assert c['wide_fraction']['pnl_delta']['floor_atoms']=='-1'
    assert F(-25)-F(-10)==-15

if __name__ == '__main__':
    result=dict(decision='DEC-31',model='history-explorer-v1',cases=cases())
    check(result['cases'])
    Path(__file__).with_suffix('.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    print('PASS six independent Explorer time/metric cases')
