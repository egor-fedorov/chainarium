"""Pre-implementation analytical expectations for Proposed DEC-16.

Continuous invariant root by bisection + quadratic y solution. This deliberately
uses a different numerical method from the proposed integer Newton solver.
It is a mathematical reference, not an independent code review or deployment
compatibility claim. Run from repository root to print the pinned values.
"""
import json
from decimal import Decimal, localcontext, ROUND_FLOOR

cases = [
    ('balanced', 1000, 1000, 100, 4, 6, 6, 99860150),
    ('depeg', 100, 10000, 100, 4, 6, 6, 554752748),
    ('unequal-decimals', 1000, 1000, 100, 4, 6, 8, 9986015082),
]
results = []
with localcontext() as context:
    context.prec = 110
    for name, rx, ry, dx, fee, decx, decy, expected in cases:
        x, y, a = [Decimal(v) * 10**18 for v in (rx, ry, dx)]
        ann = Decimal(200)  # n=2, A=100, Ann=n*A
        lo, hi = Decimal(0), x+y
        for _ in range(500):
            d = (lo+hi)/2
            residual = (ann-1)*d + d**3/(4*x*y) - ann*(x+y)
            if residual > 0:
                hi = d
            else:
                lo = d
        invariant = (lo+hi)/2
        xn = x+a
        b = xn+invariant/ann-invariant
        c = invariant**3/(4*ann*xn)
        yn = (-b+(b*b+4*c).sqrt())/2
        out = (y-yn)*(10000-fee)/10000 / Decimal(10)**(18-decy)
        raw = int(out.to_integral_value(rounding=ROUND_FLOOR))
        assert raw == expected
        results.append(dict(case=name, decimals=[decx, decy],
                            balances_atoms=[str(rx*10**decx), str(ry*10**decy)],
                            input_atoms=str(dx*10**decx), A=100, fee_bps=fee,
                            continuous_D=str(invariant), continuous_output_atoms=str(out),
                            expected_output_atoms=str(expected)))
print(json.dumps(results, indent=2))
