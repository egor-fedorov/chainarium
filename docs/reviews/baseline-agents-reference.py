"""Proposed DEC-26 expectations, before policy implementation.

Independent scalar ChaCha12 + integer accounting; no Chainarium imports.
The stream derivation follows Accepted DEC-10, not a new RNG algorithm.
"""
import hashlib
import json
import struct


def rotate(x, n):
    return ((x << n) | (x >> (32 - n))) & 0xffffffff


def quarter(s, a, b, c, d):
    s[a] = (s[a] + s[b]) & 0xffffffff
    s[d] = rotate(s[d] ^ s[a], 16)
    s[c] = (s[c] + s[d]) & 0xffffffff
    s[b] = rotate(s[b] ^ s[c], 12)
    s[a] = (s[a] + s[b]) & 0xffffffff
    s[d] = rotate(s[d] ^ s[a], 8)
    s[c] = (s[c] + s[d]) & 0xffffffff
    s[b] = rotate(s[b] ^ s[c], 7)


def block(key, counter):
    initial = list(struct.unpack('<4I', b'expand 32-byte k'))
    initial += list(struct.unpack('<8I', key))
    initial += [counter & 0xffffffff, counter >> 32, 0, 0]
    s = initial.copy()
    for _ in range(6):
        for indices in [(0, 4, 8, 12), (1, 5, 9, 13), (2, 6, 10, 14), (3, 7, 11, 15),
                        (0, 5, 10, 15), (1, 6, 11, 12), (2, 7, 8, 13), (3, 4, 9, 14)]:
            quarter(s, *indices)
    return [(x + y) & 0xffffffff for x, y in zip(s, initial)]


class Stream:
    def __init__(self, root, name):
        parts = [b'chacha12-v1', str(root).encode(), name.encode()]
        self.key = hashlib.sha256(b''.join(len(p).to_bytes(8, 'big') + p for p in parts)).digest()
        self.position = 0

    def draw(self):
        s = block(self.key, self.position // 16)
        i = self.position % 16
        self.position += 2
        return s[i] | (s[i + 1] << 32)

    def uniform(self, size):
        limit = (1 << 64) // size * size
        for _ in range(32):
            n = self.draw()
            if n < limit:
                return n % size
        raise ValueError('RngBudget')


def random_orders():
    stream = Stream(42, 'agent/trader')
    sender, recipient, builder, nonce = 1000, 0, 0, 0
    rows = []
    for round_ in range(1, 7):
        before = stream.position
        gate, choice = stream.uniform(1000000), stream.uniform(2)
        amount = [10, 20][choice]
        submit = gate < 500000
        if submit:
            sender -= amount + 115
            recipient += amount
            builder += 115
            nonce += 1
        rows.append(dict(round=str(round_), gate=str(gate), choice=str(choice), amount=str(amount),
                         submit=submit, position_before=str(before), position_after=str(stream.position),
                         sender_Q=str(sender), recipient_Q=str(recipient), builder_Q=str(builder), nonce=str(nonce)))
    return rows


def borrower():
    # Zero interest; exact target reconciliation, one ordinary call per round.
    q, x, collateral, debt, cash, builder, nonce = 1000, 6, 0, 0, 100, 0, 0
    rows = []
    for round_ in range(1, 7):
        target_c, target_d = (6, 80) if round_ < 4 else (0, 0)
        if debt > target_d:
            action, amount = 'Repay', min(debt - target_d, 80)
            q -= amount
            cash += amount
            debt -= amount
        elif collateral < target_c:
            action, amount = 'DepositCollateral', min(target_c - collateral, 6)
            x -= amount
            collateral += amount
        elif debt < target_d:
            action, amount = 'Borrow', min(target_d - debt, 80)
            q += amount
            cash -= amount
            debt += amount
        elif collateral > target_c:
            action, amount = 'WithdrawCollateral', min(collateral - target_c, 6)
            x += amount
            collateral -= amount
        else:
            action, amount = 'Hold', 0
        used = 0 if action == 'Hold' else 115
        q -= used
        builder += used
        nonce += used != 0
        assert cash + debt == 100
        rows.append(dict(round=str(round_), action=action, amount=str(amount), used=str(used),
                         sender_Q=str(q), sender_X=str(x), collateral=str(collateral), debt=str(debt),
                         cash=str(cash), claims='100', equity='0', builder_Q=str(builder), nonce=str(nonce)))
    assert (q, x, collateral, debt, cash, builder) == (540, 6, 0, 0, 100, 460)
    return rows


def reference():
    stream = Stream(42, 'agent/trader')
    vectors = [str(stream.draw()) for _ in range(12)]
    return dict(
        status='pre-implementation expectations for Proposed DEC-26',
        rng=dict(root='42', stream='agent/trader', first_12_u64=vectors,
                 sample='reject x >= floor(2^64/n)*n; return x mod n; at most 32 draws'),
        random_user=dict(probability_ppm='500000', templates_Q=['10', '20'], resource_price='1',
                         resource_limit='200', initial_sender_Q='1000', rounds=random_orders()),
        scripted_failure=dict(program='Transfer10 Q then false AssertCondition', used='125',
                              sender_Q='875', recipient_Q='0', builder_Q='125', nonce='1',
                              require_success='Skip; no canonical fee/nonce',
                              submit_failure='Submit; included ProtocolRevert, transfer rolled back'),
        borrower=dict(price_Q_per_X='20', ltv_bps='7500', lt_bps='8000', rate='0',
                      target_round_1=dict(collateral='6', debt='80'),
                      target_round_4=dict(collateral='0', debt='0'), rounds=borrower()),
        oracle=dict(spot_before_swap='20', swap_input_X='10', swap_output_Q='100',
                    pool_after_X='20', pool_after_Q='100', spot_after_swap='5',
                    publish_spot_used='110',
                    twap_constant_price='20', twap_window='2', interval='1',
                    round_1='NotReady; Skip, no fee/nonce',
                    round_2=dict(price='20', sequence='1', observed_at='2', published_at='2', used='111')),
        boundaries=dict(templates='1..16', schedule='0..1024 unique rounds', feeds='1..16 unique native feeds',
                        samples_per_eligible_round='two, even on Hold/BusyNonce',
                        draws_per_sample='32 maximum', raw_draws_per_round='4096 maximum for 64 agents',
                        simulation='at most one per selected nonempty intent',
                        overflow='Nonce/Deadline/Delivery/OracleSequence => Skip; stream exhaustion => ModelError',
                        rollback='ModelError rolls back RNG with the round; included protocol failure does not undo decisions'))


if __name__ == '__main__':
    print(json.dumps(reference(), indent=2))
