"""Analytical expectations for Proposed DEC-20, before implementation.

Uses Python integers and Fraction only; no executor imports. This calculation
pins example outcomes, not an independent review of the future implementation.
Run this file to regenerate heterogeneous-route-reference.json.
"""
import json
from fractions import Fraction
from math import isqrt


def ceil(value):
    return -(-value.numerator // value.denominator)


def fee(quote):
    return ceil(Fraction(quote * 100, 10000))


def cpmm(amount, reserve):
    return reserve * amount // (reserve + amount)


def strings(values):
    return {key: str(value) for key, value in values.items()}


lot = 10
sell_budget = 35
sell_lots = min(sell_budget // lot, 2)
sell_gross = sell_lots * 100
sell_fee = fee(sell_gross)
sell_output = sell_gross - sell_fee
sell = strings(dict(
    budget_X=sell_budget, spent_X=sell_lots * lot,
    residual_X=sell_budget - sell_lots * lot,
    gross_Q=sell_gross, fee_Q=sell_fee, book_output_Q=sell_output,
    final_R=cpmm(sell_output, 1000),
))
sell['cpmm_reserves_Q_R'] = ['1000', '1000']
assert (sell['spent_X'], sell['residual_X'], sell['final_R']) == ('20', '15', '165')

buy_budget, buy_limit = 448, 110
buy_lots = buy_budget // (buy_limit + fee(buy_limit))
# Two maker orders, fees rounded separately for each actual match.
buy_matches = [2 * 100, (buy_lots - 2) * 110]
buy_gross = sum(buy_matches)
buy_fee = sum(map(fee, buy_matches))
buy = strings(dict(
    budget_Q=buy_budget, lots=buy_lots, gross_Q=buy_gross, fee_Q=buy_fee,
    spent_Q=buy_gross + buy_fee, residual_Q=buy_budget - buy_gross - buy_fee,
    output_X=buy_lots * lot,
))
assert (buy['lots'], buy['spent_Q'], buy['residual_Q'], buy['output_X']) == ('4', '425', '23', '40')

q96, liquidity = 2**96, 1000000
lower = isqrt(q96 * q96 * 10000**100 // 10001**100)
upper = isqrt(q96 * q96 * 10001**100 // 10000**100)
net_x = ceil(Fraction(liquidity * q96 * (q96 - lower), q96 * lower))
gross_x = ceil(Fraction(net_x * 10000, 9970))
output_q = liquidity * (q96 - lower) // q96
clmm = strings(dict(
    budget_X=10000, spent_X=gross_x, residual_X=10000 - gross_x,
    clmm_output_Q=output_q, final_R=cpmm(output_q, 10000),
))
clmm['cpmm_reserves_Q_R'] = ['10000', '10000']
assert (clmm['spent_X'], clmm['clmm_output_Q'], clmm['final_R']) == ('5029', '4987', '3327')

lp = strings(dict(
    target_block=1, lower_tick=-100, upper_tick=100, liquidity=liquidity,
    mint_X=ceil(Fraction(liquidity * q96 * (upper - q96), upper * q96)),
    mint_Q=ceil(Fraction(liquidity * (q96 - lower), q96)),
))
lp['after_target'] = 'Hold, including outside the range'
assert lp['mint_X'] == lp['mint_Q'] == '4988'

quote_tick, lots_each, spread = 2, 2, 5
prices = [Fraction(10), Fraction(12)]
anchors = [int(price * lot / quote_tick) for price in prices]
quotes = [(anchor - spread, anchor + spread) for anchor in anchors]
locks = [(lots_each * lot, lots_each * bid * quote_tick) for bid, _ in quotes]
mm = strings(dict(
    lot_X=lot, quote_tick_Q=quote_tick, lots_each_side=lots_each,
    spread_ticks=spread, initial_oracle_Q_per_X=prices[0], new_oracle_Q_per_X=prices[1],
    initial_anchor_ticks=anchors[0], new_anchor_ticks=anchors[1],
))
mm.update(
    initial_bid_ask_ticks=list(map(str, quotes[0])), new_bid_ask_ticks=list(map(str, quotes[1])),
    initial_locked_X_Q=list(map(str, locks[0])), new_locked_X_Q=list(map(str, locks[1])),
)
assert anchors == [50, 60] and quotes == [(45, 55), (55, 65)]
assert locks == [(20, 180), (20, 220)]

print(json.dumps(dict(
    status='pre-implementation expectations for Proposed DEC-20',
    book_sell_then_cpmm=sell, book_buy_budget=buy, clmm_then_cpmm=clmm,
    passive_clmm_lp=lp, market_maker_reprice=mm,
), indent=2))
