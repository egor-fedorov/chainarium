"""Proposed DEC-28: exact MTM expectations before implementing the evaluator.

Pure Python Fraction, no simulator imports. Quantities are synthetic valuation
components, including the already accepted DEC-19 CLMM claim fixture. They do
not assert a new execution path or implement a production portfolio extractor.
"""
from fractions import Fraction as F
import json


def wire(value):
    value = F(value)
    return dict(numerator=str(value.numerator), denominator=str(value.denominator),
                floor_atoms=str(value.numerator // value.denominator))


def evaluate(components, prices, bits=32768):
    total = F(0)
    missing = set()
    for asset, quantity in components:
        if not quantity:
            continue
        price = F(1) if asset == 'Q' else prices.get(asset)
        # Check gross nonzero exposure before any offset between assets/debts.
        if price is None:
            missing.add(asset)
            continue
        term = quantity * price
        if max(abs(term.numerator).bit_length(), term.denominator.bit_length()) > bits:
            return dict(status='Unpriced', reason='ArithmeticLimit', value=None, priced_part=None)
        total += term
        if max(abs(total.numerator).bit_length(), total.denominator.bit_length()) > bits:
            return dict(status='Unpriced', reason='ArithmeticLimit', value=None, priced_part=None)
    return dict(status='Unpriced' if missing else 'Priced',
                reason='MissingPrice' if missing else None,
                missing_assets=sorted(missing),
                value=None if missing else wire(total), priced_part=wire(total))


def capital_pnl(initial, end, incoming=(), outgoing=()):
    incoming, outgoing = sum(incoming, F(0)), sum(outgoing, F(0))
    return dict(initial=wire(initial), end=wire(end), inflows=wire(incoming),
                outflows=wire(outgoing), pnl=wire(end-initial-incoming+outgoing))


def price_at(updates, at, max_age):
    visible = [r for r in updates if r['available_at'] <= at]
    if not visible:
        return dict(status='Unpriced', reason='NoAvailableRecord')
    record = max(visible, key=lambda r: (r['available_at'], r['sequence']))
    if record['observed_at'] > at:
        return dict(status='Unpriced', reason='FutureObservation')
    age = at-record['observed_at']
    if age > max_age:
        return dict(status='Unpriced', reason='Stale', age=str(age))
    return dict(status='Priced', value=wire(record['price']), age=str(age),
                sequence=str(record['sequence']))


def cases():
    maximum = (1 << 256)-1
    lp = [('X', F(100*1102, 1100)), ('Q', F(100*1099, 1100))]
    stable = [('Q', F(7*101, 100)), ('R', F(7*99, 100))]
    prices = {'X': F(1, 2)}
    freshness = [dict(available_at=0, observed_at=0, sequence=1, price=F(2)),
                 dict(available_at=62, observed_at=59, sequence=2, price=F(3))]
    result = {
        'two_wallet_halves': evaluate([('X', F(1)), ('X', F(1))], prices),
        'negative_half': evaluate([('X', F(-1))], prices),
        'zero_without_price': evaluate([('X', F(0))], {}),
        'missing_with_priced_part': evaluate([('Q', F(7)), ('X', F(1))], {}),
        'missing_gross_offset': evaluate([('X', F(1)), ('X', F(-1))], {}),
        'two_half_inflows': capital_pnl(F(0), F(1), [F(1, 2), F(1, 2)]),
        'flow_time_price': capital_pnl(F(100), F(130), [F(2)*10]),
        'rounding_is_presentation': capital_pnl(F(1, 2), F(1), [F(1, 2)]),
        'failed_capital_transfer': capital_pnl(F(1000), F(885)),
        'borrow_is_not_capital': capital_pnl(F(100+6*20), F(180+6*20-80)),
        'interest_is_not_capital': capital_pnl(F(220), F(180+6*20-90)),
        'flash_cycle_with_self_owned_pools': dict(wallet_cycle_net='24104',
            portfolio=capital_pnl(F(1000+1000000+1500000+2000000),
                                  F(25104+1100000+1375656+2000000))),
        'post_loss_deposit': dict(claim_atoms=str(30*70//100), reserve_cash='10',
                                  cash_bound='10', mark=evaluate([('Q', F(7+30*70//100))], {}),
                                  change=capital_pnl(F(37), F(28))),
        'fractional_cpmm_claim': dict(mark=evaluate(lp, {'X': F(2)}),
                                      burn_without_execution_cost=wire((100*1102//1100)*2+(100*1099//1100))),
        'fractional_stable_claim': evaluate(stable, {'R': F(2, 3)}),
        'clmm_principal_and_collectable_fees': dict(mark=evaluate([('X', F(5984+2)), ('Q', F(3991))], {'X': F(1)}),
                                                    excluded_rounding_surplus={'X':'2','Q':'1'}),
        'resting_sell_order': evaluate([('Q', F(7)), ('X', F(3*10))], {'X': F(2)}),
        'resting_buy_order': evaluate([('Q', F(7+3*50*2))], {}),
        'freshness_60_61_and_delayed_record': {str(t): price_at(freshness, t, 60) for t in [60,61,62]},
        'future_observation': {str(t): price_at([dict(available_at=0, observed_at=10, sequence=1, price=F(3))],t,60) for t in [9,10]},
        'no_available_record': price_at([dict(available_at=10, observed_at=0, sequence=1, price=F(2))],0,60),
        'historical_unpriced_flow': dict(current=evaluate([('X',F(1))],{'X':F(10)}),
            flow_price_at_execution=price_at([dict(available_at=10, observed_at=0, sequence=1, price=F(10))],0,60),
            pnl=dict(status='Unpriced',reason='HistoricalFlowUnpriced')),
        'wide_signed_report': evaluate([('X', F(maximum)), ('Q', F(maximum))], {'X': F(maximum)}),
        'bounded_exact_arithmetic': evaluate([('X',F(1)),('Y',F(1)),('R',F(1))],
            {'X':F(1,(1<<255)-19),'Y':F(1,(1<<254)-127),'R':F(1,(1<<253)-59)}, bits=512),
    }
    assert result['two_wallet_halves']['value'] == wire(1)
    assert result['negative_half']['value'] == wire(F(-1,2))
    assert result['negative_half']['value']['floor_atoms'] == '-1'
    assert result['two_half_inflows']['pnl'] == wire(0)
    assert result['rounding_is_presentation']['pnl'] == wire(0)
    assert result['flow_time_price']['pnl'] == wire(10)
    assert result['failed_capital_transfer']['pnl'] == wire(-115)
    assert result['borrow_is_not_capital']['pnl'] == wire(0)
    assert result['interest_is_not_capital']['pnl'] == wire(-10)
    assert result['flash_cycle_with_self_owned_pools']['portfolio']['pnl'] == wire(-240)
    assert result['post_loss_deposit']['mark']['value'] == wire(28)
    assert result['fractional_cpmm_claim']['mark']['value'] == wire(F(3303,11))
    assert result['fractional_cpmm_claim']['burn_without_execution_cost'] == wire(299)
    assert result['fractional_stable_claim']['value'] == wire(F(1169,100))
    assert result['clmm_principal_and_collectable_fees']['mark']['value'] == wire(9977)
    assert result['resting_sell_order']['value'] == wire(67)
    assert result['resting_buy_order']['value'] == wire(307)
    assert result['freshness_60_61_and_delayed_record']['61']['reason'] == 'Stale'
    assert result['bounded_exact_arithmetic']['reason'] == 'ArithmeticLimit'
    return result


if __name__ == '__main__':
    print(json.dumps(dict(status='Proposed', decision='DEC-28', model='reference-mtm-v1',
                         cases=cases()), ensure_ascii=False, indent=2))
