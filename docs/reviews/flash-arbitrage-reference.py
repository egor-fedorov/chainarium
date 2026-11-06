"""Proposed DEC-27: independent integer expectations before policy implementation.

Two CPMMs (fee30 bps): A = 1000 X / 1000 Q, B = 1000 X / 1500 Q.
Trade borrowed Q on A, then all received X on B; provider fee5 bps, ceil.
No simulator imports. Existing native calls/costs are used, with no new opcode.
"""
import json


def output(reserve_in, reserve_out, amount):
    return amount * 9970 * reserve_out // (reserve_in * 10000 + amount * 9970)


def cycle(scale, price, own, minimum, market_move=0):
    principal = 100 * scale
    fee = (principal * 5 + 9999) // 10000
    first = output(1000 * scale, 1000 * scale, principal)
    moved_out = output(1000 * scale, 1500 * scale, market_move)
    bx, bq = 1000 * scale + market_move, 1500 * scale - moved_out
    second = output(bx, bq, first)
    # Envelope + FlashLoan + loan transfer + FlashCallback + ExecuteRoute
    # + two (swap call + input/output transfers) + settlement transfer + guard.
    units = 100 + 10 + 5 + 10 + 10 + 2 * (10 + 5 + 5) + 5 + 10
    assert units == 190
    limit = 300
    due = principal + fee
    fee_reserve = limit * price
    hypothetical_net = second - due - units * price
    if own < fee_reserve:
        status, used = 'InclusionRejected', 0
    elif own - fee_reserve + second < due:
        status, used = 'SettlementShortfall', units - 10
    elif hypothetical_net < minimum:
        status, used = 'NetGuardRevert', units
    else:
        status, used = 'Success', units
    success = status == 'Success'
    return dict(
        scale=scale, price=price, own_before=own, minimum=minimum,
        principal=principal, flash_fee=fee, due=due, resource_limit=limit,
        market_move_x=market_move, market_move_output_q=moved_out,
        route_first_x=first, route_last_q=second,
        projected_net=hypothetical_net, status=status, used=used,
        actual_fee=used * price,
        own_after=own + (second - due if success else 0) - used * price,
        provider_gain=fee if success else 0,
        first_pool_q_after=1000 * scale + (principal if success else 0),
        first_pool_x_after=1000 * scale - (first if success else 0),
        second_pool_x_after=bx + (first if success else 0),
        second_pool_q_after=bq - (second if success else 0),
    )


def cases():
    return {
        'small_free': cycle(1, 0, 0, 1),
        'small_paid_unprofitable': cycle(1, 1, 1000, 1),
        'scaled_paid': cycle(1000, 1, 1000, 1),
        'scaled_exact_minimum': cycle(1000, 1, 1000, 24104),
        'scaled_minimum_plus_one': cycle(1000, 1, 1000, 24105),
        'late_guard_revert': cycle(1000, 1, 1000, 20000, 50000),
        'late_settlement_shortfall': cycle(1000, 1, 1000, 1, 150000),
        'own_fee_reserve_shortfall': cycle(1000, 1, 299, 1),
    }


def strings(value):
    if isinstance(value, dict):
        return {key: strings(item) for key, item in value.items()}
    return str(value) if isinstance(value, int) else value


if __name__ == '__main__':
    results = cases()
    assert results['small_free']['own_after'] == 22
    assert results['scaled_paid']['own_after'] == 25104
    assert results['late_guard_revert']['own_after'] == 810
    assert results['late_settlement_shortfall']['own_after'] == 820
    print(json.dumps({
        'status': 'Proposed', 'decision': 'DEC-27',
        'model': 'bounded-flash-arbitrage-v1',
        'note': 'Direct literal-call outcomes; a failed policy trial submits nothing and charges no canonical fee. Late cases assume a successful preflight on the unchanged pools, then a price0 market-move prefix.',
        'cases': strings(results),
    }, ensure_ascii=False, indent=2))
