"""SC-07 analytical expectations, using the accepted M2 contracts.

The stable leg uses the continuous quadratic solution (not the Rust integer
Newton algorithm). Its output is far from an integer boundary. This calculation
does not replace independent review of the executor or accounting.
"""
import json
from decimal import Decimal, localcontext

sale = 6 * 9970 * 1500 // (100 * 10000 + 6 * 9970)
assert sale == 84
with localcontext() as context:
    context.prec = 110
    x = y = Decimal(1000) * 10**18
    d = x + y
    xn = x + sale * 10**18
    b = xn + d / 200 - d
    c = d**3 / (4 * 200 * xn)
    yn = (-b + (b*b + 4*c).sqrt()) / 2
    continuous = (y - yn) * Decimal('0.9996') / 10**18
    stable = int(continuous)
    assert stable == 83
exit_output = stable * 9970 * 1000 // (1000 * 10000 + stable * 9970)
flash_due = 60 + (60 * 5 + 9999) // 10000
assert (exit_output, flash_due, exit_output - flash_due) == (76, 61, 15)
print(json.dumps(dict(
    principal='60', collateral='6', sale_output=str(sale),
    stable_continuous_output=str(continuous), stable_output=str(stable),
    exit_output=str(exit_output), flash_due=str(flash_due), surplus='15',
    debt_before='90', repaid='60', loss='30', equity_loss='2', depositor_loss='28',
), indent=2))
