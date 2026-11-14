"""DEC-30: independent Fraction inventory/price/flow checks on native exports.

Usage: python3 scripts/check-hodl.py output/new-directory
Requires the current release binary. No imports from production or test helpers.
Initial claim extraction is DEC-28's input boundary; everything after it is
rebuilt from native receipts, explicit bindings and reference records, not HODL.
"""
from copy import deepcopy
from fractions import Fraction as F
from pathlib import Path
import hashlib
import json
import subprocess
import sys

ROOT = Path(sys.argv[1])
ROOT.mkdir(parents=True, exist_ok=False)
BINARY = Path('target/release/chainarium').resolve()


def command(*args):
    subprocess.run([str(BINARY), *map(str, args)], check=True, capture_output=True, text=True)


def wire(n):
    n = F(n)
    return dict(numerator=str(n.numerator), denominator=str(n.denominator),
                floor_atoms=str(n.numerator // n.denominator))


def exact(q):
    if q is None:
        return None
    assert all(isinstance(q[k], str) for k in ['numerator', 'denominator', 'floor_atoms'])
    n = F(int(q['numerator']), int(q['denominator']))
    assert q == wire(n), q
    return n


def bounded(q, bits):
    if max(abs(q.numerator).bit_length(), q.denominator.bit_length()) > bits:
        raise ArithmeticError('ArithmeticLimit')
    return q


def plus(a, b):
    return None if a is None or b is None else a + b


def difference(a, b):
    return None if a is None or b is None else a - b


def price(p, asset, t):
    if asset == p['numeraire']:
        return F(1)
    records = [r for r in p['reference_feeds'].get(asset, {}).get('records', []) if int(r['available_at']) <= t]
    if not records:
        return None
    r = records[-1]
    if not 0 <= t - int(r['observed_at']) <= int(p['max_age_seconds']):
        return None
    return F(int(r['price']['numerator']), int(r['price']['denominator']))


def mark(holdings, p, t, bits):
    total = F(0)
    for asset, quantity in sorted(holdings.items()):
        if not quantity:
            continue
        px = price(p, asset, t)
        if px is None:
            return None
        try:
            total = bounded(total + bounded(quantity * px, bits), bits)
        except ArithmeticError:
            return None
    return total


def check(folder):
    run = json.loads((folder / 'run.json').read_text())
    s = run['scenario']
    hp, vp = s['hodl_policy'], s['valuation_policy']
    limits, bits = hp['limits'], hp['limits']['rational_bits']
    reports = [json.loads(l) for l in (folder / 'hodl.jsonl').read_text().splitlines()]
    mtms = [json.loads(l) for l in (folder / 'valuations.jsonl').read_text().splitlines()]
    assert len(reports) == len(run['blocks']) + 1
    manifest = json.loads((folder / 'manifest.json').read_text())
    assert json.loads((folder / 'hodl-policy.json').read_text()) == hp
    assert manifest['hodl_policy']['policy_sha256'] == hashlib.sha256((folder / 'hodl-policy.json').read_bytes()).hexdigest()
    assert manifest['hodl_policy']['valuation_policy_sha256'] == hashlib.sha256((folder / 'valuation-policy.json').read_bytes()).hexdigest()
    state = {}
    for id in hp['portfolios']:
        initial = next(p for p in mtms[0]['portfolios'] if p['portfolio'] == id)
        holdings, failure = {}, None
        if 'ComponentLimit' in initial['current']['mark']['reasons']:
            failure = 'ComponentLimit'
        for c in initial['current']['components']:
            if failure:
                break
            qty, asset = exact(c['quantity']), c['key']['asset']
            if qty is None:
                failure = 'InitialQuantityUnavailable'
            elif qty < 0 or (c['key']['kind'] == 'Debt' and qty):
                failure = 'InitialLiabilityUnsupported'
            elif qty:
                try:
                    total = bounded(bounded(qty, bits) + holdings.get(asset, F(0)), bits)
                    if asset not in holdings and len(holdings) == limits['assets_per_portfolio']:
                        failure = 'AssetLimit'
                    else:
                        holdings[asset] = total
                except ArithmeticError:
                    failure = 'ArithmeticLimit'
        state[id] = dict(holdings=holdings, failure=failure, last=None,
                         initial=exact(initial['initial']['value']), incoming=F(0), outgoing=F(0), count=0)
        if not failure:
            state[id]['last'] = dict(at=dict(type='Boundary', block='0', timestamp='0'), quantities={a: wire(q) for a, q in holdings.items()})
    seen = set()
    for i, report in enumerate(reports):
        assert report['provenance'] == manifest['hodl_policy']
        assert report['block'] == str(i)
        block = run['blocks'][i - 1] if i else None
        t = int(block['timestamp']) if block else 0
        assert report['timestamp'] == str(t)
        expected_flows = {id: [] for id in state}
        if block:
            for ri, receipt in enumerate(block['receipts']):
                if receipt['status'] != 'Success':
                    continue
                for ei, event in enumerate(receipt['events']):
                    if event['kind'] != 'Transfer' or not int(event['amount']):
                        continue
                    for binding in vp['capital_transfers']:
                        id = binding['portfolio']
                        key = (id, receipt['tx_id'])
                        if id not in state or key in seen or receipt['tx_id'] != binding['tx_id']:
                            continue
                        seen.add(key)
                        st, asset, amount = state[id], event['asset'], F(int(event['amount']))
                        owners = vp['portfolios'][id]['accounts']
                        incoming = event['to'] in owners
                        assert incoming != (event['from'] in owners)
                        cursor = dict(block=str(i), timestamp=str(t), receipt_index=ri, event_index=ei, tx_id=receipt['tx_id'])
                        before = st['holdings'].get(asset, F(0))
                        px = price(vp, asset, t)
                        field = 'incoming' if incoming else 'outgoing'
                        st[field] = plus(st[field], None if px is None else px * amount)
                        st['count'] += 1
                        if st['failure']:
                            effect = dict(status='Skipped', reason=st['failure'])
                        else:
                            reason = None
                            if st['count'] > limits['flow_events_per_portfolio']:
                                reason = 'FlowLimit'
                            elif not incoming and before < amount:
                                reason = 'InfeasibleOutflow'
                            else:
                                try:
                                    after = bounded(before + (amount if incoming else -amount), bits)
                                    if after and asset not in st['holdings'] and len(st['holdings']) == limits['assets_per_portfolio']:
                                        reason = 'AssetLimit'
                                except ArithmeticError:
                                    reason = 'ArithmeticLimit'
                            if reason:
                                st['failure'] = reason
                                st['failure_detail'] = dict(reason=reason, at=dict(type='Flow', cursor=cursor), asset=asset, requested=wire(amount), available=wire(before))
                                effect = dict(status='Failed', reason=reason)
                            else:
                                if after:
                                    st['holdings'][asset] = after
                                else:
                                    st['holdings'].pop(asset, None)
                                effect = dict(status='Applied', before=wire(before), after=wire(after))
                                st['last'] = dict(at=dict(type='Flow', cursor=cursor), quantities={a: wire(q) for a, q in st['holdings'].items()})
                        expected_flows[id].append((cursor, effect))
        for p in report['portfolios']:
            id = p['portfolio']
            st = state[id]
            actual = next(a for a in mtms[i]['portfolios'] if a['portfolio'] == id)
            assert p['actual'] == actual['current']['mark']
            assert p['actual_pnl'] == actual['pnl']
            assert p['inflows'] == actual['inflows'] and p['outflows'] == actual['outflows']
            assert [(f['cursor'], f['effect']) for f in p['flows']] == expected_flows[id]
            assert p['flow_events'] == str(st['count'])
            if st['failure']:
                assert p['ledger']['status'] == 'Unavailable'
                assert p['ledger']['failure']['reason'] == st['failure']
                if 'failure_detail' in st:
                    assert p['ledger']['failure'] == st['failure_detail']
                assert p['ledger']['last_feasible'] == st['last']
                assert p['hodl']['assets'] == {}
                hold = None
            else:
                st['last']['at'] = dict(type='Boundary', block=str(i), timestamp=str(t))
                assert p['ledger'] == dict(status='Ready', current=st['last'])
                hold = mark(st['holdings'], vp, t, bits)
                assert {a: exact(v['quantity']) for a, v in p['hodl']['assets'].items()} == st['holdings']
            hpnl = plus(difference(difference(hold, st['initial']), st['incoming']), st['outgoing'])
            assert exact(p['hodl']['mark']['value']) == hold, (folder, i, id)
            assert exact(p['hodl_pnl']['value']) == hpnl, (folder, i, id)
            assert exact(p['excess_pnl']['value']) == difference(exact(actual['pnl']['value']), hpnl)
            assert exact(p['value_gap']['value']) == difference(exact(actual['current']['mark']['value']), hold)
    return len(reports)


boundaries = 0
for scenario in sorted(Path('scenarios/demos').glob('sc-hodl-*.json')):
    target = ROOT / scenario.stem
    command('run', scenario, target)
    boundaries += check(target)
    print('PASS', scenario.stem, flush=True)

# 64 independently materialized price/flow cases, including stale/future prices,
# successful and infeasible canonical order, fractional values and lowered caps.
for i in range(64):
    kind = ['capital', 'ordered', 'infeasible', 'unpriced'][i % 4]
    s = json.loads(Path(f'scenarios/demos/sc-hodl-{kind}.json').read_text())
    s['name'] = f'HODL independent sweep {i}'
    s['seed'] = str(i)
    s['accounts']['alice']['balances']['Q'] = str(100 + i)
    for j, record in enumerate(s['valuation_policy']['reference_feeds']['X']['records']):
        record['price']['numerator'] = str((i + 3) * (j + 1))
        record['price']['denominator'] = str([2, 3, 5, 7, 11, 13, 17, 19][i // 8])
        if i % 13 == 0:
            record['observed_at'] = str(int(record['available_at']) + 2)
    if i % 11 == 0:
        s['hodl_policy']['limits']['assets_per_portfolio'] = 1
    if i % 7 == 0:
        s['hodl_policy']['limits']['flow_events_per_portfolio'] = 1
    s['hodl_policy']['limits']['rational_bits'] = 512
    path = ROOT / f'sweep-{i:02}.json'
    path.write_text(json.dumps(s))
    target = ROOT / f'sweep-{i:02}'
    command('run', path, target)
    boundaries += check(target)
print(f'PASS: seven HODL demos + 64 Fraction price/flow cases; {boundaries} boundaries, canonical receipts, inventory feasibility, prices, capital, PnL, excess, gap and provenance')
