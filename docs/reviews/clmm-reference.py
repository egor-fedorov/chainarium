"""Pre-implementation fixture for accepted DEC-19, not a compatibility claim.

Exact Fraction/isqrt rules pin the integer contract. Decimal continuous math is
an additional analytical cross-check, not a review of the future Rust code.
"""
import json
from decimal import Decimal, localcontext
from fractions import Fraction
from math import isqrt

Q96, Q128 = 2**96, 2**128

def tick(t):
    n, d = (10001**t, 10000**t) if t >= 0 else (10000**-t, 10001**-t)
    return isqrt(Q96*Q96*n//d)

def ceil(v):
    return -(-v.numerator//v.denominator)

def floor(v):
    return v.numerator//v.denominator

liquidity = 1000000
lower, upper = tick(-100), tick(100)
mint_x = ceil(Fraction(liquidity*Q96*(upper-Q96), upper*Q96))
mint_y = ceil(Fraction(liquidity*(Q96-lower), Q96))
gross, fee = 1000, 3
after = ceil(Fraction(liquidity*Q96*Q96, liquidity*Q96+(gross-fee)*Q96))
output = floor(Fraction(liquidity*(Q96-after), Q96))
burn_x = floor(Fraction(liquidity*Q96*(upper-after), upper*after))
burn_y = floor(Fraction(liquidity*(after-lower), Q96))
growth = fee*Q128//liquidity
collect_x = liquidity*growth//Q128
assert (mint_x, mint_y, output, burn_x, burn_y, collect_x) == (4988,4988,996,5984,3991,2)
with localcontext() as context:
    context.prec = 110
    continuous_output = Decimal(liquidity)*(1-Decimal(liquidity)/(liquidity+gross-fee))
    continuous_deposit = Decimal(liquidity)*(1-Decimal('1.0001')**Decimal(-50))
    assert int(continuous_output) == output
    assert ceil(Fraction(continuous_deposit)) == mint_x == mint_y
result = dict(
    tick_minus_100=str(lower), tick_100=str(upper), initial_sqrt=str(Q96),
    liquidity=str(liquidity), mint_x=str(mint_x), mint_y=str(mint_y),
    gross_input=str(gross), input_fee=str(fee), sqrt_after=str(after),
    output=str(output), fee_growth_x128=str(growth), collect_x=str(collect_x),
    burn_x=str(burn_x), burn_y=str(burn_y),
    surplus_x=str(mint_x+gross-burn_x-collect_x), surplus_y=str(mint_y-output-burn_y),
    continuous_output=str(continuous_output), continuous_deposit=str(continuous_deposit),
)
assert (result['surplus_x'],result['surplus_y']) == ('2','1')
# Three adjacent positions; two exact boundary crossings followed by a partial
# step. Values are pinned before the Rust matching/fee-growth implementation.
p, remaining = Q96, 16000
steps = []
for liquidity, boundary in [(1000000,-100),(2000000,-200),(3000000,-300)]:
    target = tick(boundary)
    net = ceil(Fraction(liquidity*Q96*(p-target),p*target))
    available = remaining*9970//10000
    if available >= net:
        after = target
        gross = ceil(Fraction(net*10000,9970))
    else:
        after = ceil(Fraction(liquidity*Q96*p,liquidity*Q96+available*p))
        net = ceil(Fraction(liquidity*Q96*(p-after),p*after))
        gross = remaining
    output = floor(Fraction(liquidity*(p-after),Q96))
    steps.append(dict(target_tick=boundary,liquidity=str(liquidity),
                      sqrt_before=str(p),sqrt_after=str(after),net=str(net),
                      gross=str(gross),fee=str(gross-net),output=str(output),
                      crossed=after==target))
    remaining -= gross
    p = after
assert remaining == 0
assert [x['gross'] for x in steps] == ['5029','10106','865']
assert [x['output'] for x in steps] == ['4987','9924','844']
assert [x['fee'] for x in steps] == ['16','31','3']
result['crossing'] = dict(input='16000',output='15755',steps=steps)
print(json.dumps(result,indent=2))
