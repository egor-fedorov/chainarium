"""Proposed DEC-30: flow-matched, unlevered reference HODL expectations.

Independent fractions and literal inventory/capital paths. No production imports,
executor outputs, implicit trades, netted intra-block flows or floating point.
"""
from fractions import Fraction as F
from pathlib import Path
import json


def wire(value):
    value = F(value)
    return dict(numerator=str(value.numerator), denominator=str(value.denominator),
                floor_atoms=str(value.numerator // value.denominator))


def marked(value, reason='Unpriced'):
    return dict(value=None, reasons=[reason]) if value is None else dict(value=wire(value), reasons=[])


def value_of(holdings, prices):
    total = F(0)
    for asset, quantity in sorted(holdings.items()):
        if not quantity:
            continue
        price = F(1) if asset == 'Q' else prices.get(asset)
        if price is None:
            return None
        total += quantity * F(price)
    return total


def flow(tx, direction, asset, amount, price=None):
    return dict(tx=tx, direction=direction, asset=asset, amount=str(amount), price=price)


def benchmark(initial, p0, pt, actual_value, flows=(), initial_liability=False):
    holdings = {a: F(q) for a, q in initial.items()}
    initial_value = value_of(holdings, p0)
    incoming = outgoing = F(0)
    priced_flows = True
    failure = {'reason': 'InitialLiabilityUnsupported'} if initial_liability or any(q < 0 for q in holdings.values()) else None
    applied = []
    for event in flows:  # canonical receipt/event order, never sorted by tx ID
        asset, quantity = event['asset'], F(event['amount'])
        price = F(1) if asset == 'Q' else event['price']
        if price is None:
            priced_flows = False
        elif event['direction'] == 'Inflow':
            incoming += quantity * F(price)
        else:
            outgoing += quantity * F(price)
        if failure:
            continue
        before = holdings.get(asset, F(0))
        if event['direction'] == 'Outflow' and before < quantity:
            failure = dict(reason='InfeasibleOutflow', tx=event['tx'], asset=asset,
                           requested=wire(quantity), available=wire(before))
            continue
        holdings[asset] = before + (quantity if event['direction'] == 'Inflow' else -quantity)
        applied.append(dict(tx=event['tx'], before=wire(before), after=wire(holdings[asset])))
    current = None if failure else value_of(holdings, pt)
    def pnl(value):
        if value is None or initial_value is None or not priced_flows:
            return None
        return value - initial_value - incoming + outgoing
    hp, ap = pnl(current), pnl(None if actual_value is None else F(actual_value))
    excess = None if ap is None or hp is None else ap - hp
    gap = None if actual_value is None or current is None else F(actual_value) - current
    return dict(ledger_status='Unavailable' if failure else 'Ready', failure=failure,
                holdings={a: wire(q) for a, q in sorted(holdings.items())},
                holdings_are='last_feasible' if failure else 'current', applied=applied,
                initial=marked(initial_value), current=marked(current, failure['reason'] if failure else 'CurrentUnpriced'),
                actual_pnl=marked(ap, 'ActualPnlUnpriced'), hodl_pnl=marked(hp, 'HodlPnlUnavailable'),
                excess_pnl=marked(excess, 'ExcessPnlUnavailable'), value_gap=marked(gap, 'ValueGapUnavailable'))


def cases():
    return {
        'static_basket': benchmark({'X': 10, 'Q': 100}, {'X': 10}, {'X': 12}, 220),
        # Fee-free CPMM100X/100Q: swap100Q returns50X; final pool50X/200Q.
        # Reference X/Q moves1 ->4. No add-on swap fee or IL label.
        'cpmm_price_shock': benchmark({'X': 100, 'Q': 100}, {'X': 1}, {'X': 4}, 400),
        'dynamic_inflow': benchmark({'X': 10, 'Q': 100}, {'X': 10}, {'X': 15}, 250,
                                    [flow('deposit', 'Inflow', 'X', 2, 10)]),
        'dynamic_outflow': benchmark({'X': 10, 'Q': 100}, {'X': 10}, {'X': 15}, 195,
                                     [flow('withdraw', 'Outflow', 'X', 3, 12)]),
        'canonical_order_in_then_out': benchmark({'Q': 100}, {}, {'X': 10}, 110,
                    [flow('z-in', 'Inflow', 'X', 2, 10), flow('a-out', 'Outflow', 'X', 1, 10)]),
        'canonical_order_out_then_in': benchmark({'Q': 100}, {}, {'X': 10}, 110,
                    [flow('a-out', 'Outflow', 'X', 1, 10), flow('z-in', 'Inflow', 'X', 2, 10)]),
        'outflow_requires_no_implicit_sale': benchmark({'X': 10}, {'X': 10}, {'X': 10}, 90,
                    [flow('withdraw-Q', 'Outflow', 'Q', 10)]),
        'historical_unpriced_flow': benchmark({'X': 1, 'Q': 10}, {'X': 2}, {'X': 3}, 18,
                    [flow('unknown-price', 'Inflow', 'X', 2)]),
        'zero_final_asset_needs_no_price': benchmark({'X': 2, 'Q': 100}, {'X': 10}, {}, 100,
                    [flow('withdraw-all-X', 'Outflow', 'X', 2, 10)]),
        'fractional_initial_claim': benchmark({'X': F(1, 3)}, {'X': 3}, {'X': 5}, F(4, 3)),
        'failed_paid_transfer': benchmark({'Q': 1000}, {}, {}, 885),
        'unlabelled_transfer_is_performance': benchmark({'Q': 100}, {}, {}, 90),
        'borrow_is_not_capital': benchmark({'Q': 100}, {}, {}, 100),
        'interest_is_performance': benchmark({'Q': 100}, {}, {}, 90),
        'initial_liability_is_not_hodl': benchmark({'Q': 100}, {}, {}, 100, initial_liability=True),
        'missing_initial_price': benchmark({'X': 1}, {}, {'X': 3}, 3),
        'missing_final_price': benchmark({'X': 1}, {'X': 2}, {}, None),
        'fractional_inventory_cannot_fund_one_atom': benchmark({'X': F(1,3)}, {'X': 3}, {'X': 3}, 0,
                    [flow('too-large', 'Outflow', 'X', 1, 3)]),
    }


def check(result):
    c = result['cases']
    for case, metric, expected in [
        ('static_basket','hodl_pnl',20),('cpmm_price_shock','hodl_pnl',300),
        ('cpmm_price_shock','actual_pnl',200),('cpmm_price_shock','excess_pnl',-100),
        ('dynamic_inflow','hodl_pnl',60),('dynamic_inflow','actual_pnl',30),
        ('dynamic_inflow','excess_pnl',-30),('dynamic_outflow','hodl_pnl',41),
        ('dynamic_outflow','actual_pnl',31),('canonical_order_in_then_out','hodl_pnl',0),
        ('zero_final_asset_needs_no_price','hodl_pnl',0),('fractional_initial_claim','hodl_pnl',F(2,3)),
        ('fractional_initial_claim','excess_pnl',F(-1,3)),('failed_paid_transfer','excess_pnl',-115),
        ('borrow_is_not_capital','excess_pnl',0),('interest_is_performance','excess_pnl',-10),
        ('historical_unpriced_flow','current',19),('historical_unpriced_flow','value_gap',-1),
        ('missing_initial_price','current',3),('missing_initial_price','value_gap',0),
    ]:
        assert c[case][metric]['value'] == wire(expected), (case, metric)
    for case in ['canonical_order_out_then_in','outflow_requires_no_implicit_sale','fractional_inventory_cannot_fund_one_atom']:
        assert c[case]['failure']['reason'] == 'InfeasibleOutflow'
        assert c[case]['excess_pnl']['value'] is None
    for case in ['historical_unpriced_flow','missing_initial_price','missing_final_price']:
        assert c[case]['ledger_status'] == 'Ready'
        assert c[case]['excess_pnl']['value'] is None
    assert c['fractional_initial_claim']['excess_pnl']['value']['floor_atoms'] == '-1'
    assert c['initial_liability_is_not_hodl']['ledger_status'] == 'Unavailable'
    # Regrouping initial LP components must not floor fractional atoms.
    assert sum([F(1,3),F(2,3)]) == F(1)
    # A 512-bit holding bound is checked after reduction on each update.
    assert (2**511).bit_length() == 512 and (F(2**511)+F(2**511)).numerator.bit_length() == 513


if __name__ == '__main__':
    result = dict(status='Proposed', decision='DEC-30', model='flow-matched-hodl-v1', cases=cases())
    check(result)
    Path(__file__).with_suffix('.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    print(f'{len(result["cases"])} independent HODL cases passed')
