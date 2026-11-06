# Независимый review M1–M3b

Объект: `4d5b6bb..42651ee`. Владелец разрешил одного отдельного read-only
reviewer в чате 2026-11-06. Проверка проводится в отдельном контексте по §23.4,
на копии зафиксированного commit; DEC-20 в эту область не входит.
Ниже — подтверждённые findings и исправления. Итоговая перепроверка reviewer
ещё выполняется; этот документ не объявляет полную v1.0 принятой.

## P1: CLMM inside с разной историей границ

На SC-02: mint [0,100], L=1000000; swap 6000 Q до tick200 с partial=true;
mint [-100,0], L=1000000. После второго действия current_tick=100 и Q growth
`5444517870735015415413993718908291`. Новая нижняя граница получает outside=G,
существующая верхняя — 0; inside=G−G−(G−0)=−G. Unsigned вычитание приводило
к ModelError и отмене всего блока при допустимом mint.

Исправление: inside/checkpoint имеют знак и checked U256 модуль, а разность
для живой позиции обязана быть неотрицательной. Global/outside counters,
fee allocation, округление и отсутствие wrap сохранены. Регрессия проверяет
mint, отсутствие исторических fees, обратный crossing, collect и burn.
Дополнительная последовательность из 600 mint/swap/collect/burn проверяет backing.
CLMM suite: 17 tests passed.

## P2: входящий перевод маскировал убыточный stale arbitrage

На SC-03 Bob выполняет swap 200 Q через cpmm и переводит Alice 100 Q перед
транзакцией политики. Политика ожидает +23 Q из старого состояния, но реально
получает 88 Q за 100 Q; старый guard сравнивал 10088 с 10000+1 и разрешал убыток
12 Q относительно фактического баланса 10100 Q на входе транзакции.

Исправление: `AssertTransactionNetGain` берёт baseline на входе execute до
fee reserve. Остаётся последним root call в fee asset, учитывает used units,
refund и fee recipient. Явный `AssertNetBalanceDelta` сохранён для его исходной
семантики. Регрессия теперь получает ProtocolRevert, сохраняет входящий перевод
и откатывает обе торговые ноги. Composition: 20 passed, StableSwap: 10 passed;
включены существующие проверки ненулевой native fee и fee recipient=sender.

## P2: lending withdrawal с нулевой выплатой сжигал shares

После двух блоков SC-05: claims=60, total_shares=75. Withdraw 1 share с
min_assets=0 раньше сжигал share за floor(60/75)=0 токенов. Это нарушает явный
запрет нулевой экономической суммы в DEC-14.

Исправление: нулевой redemption отклоняется до burn/transfer. Регрессия проверяет
ProtocolRevert, неизменные shares/claims/balance и отсутствие canonical events.
Credit suite: 19 tests passed; сравнение состояния учитывает системный accrual
на границе блока, который не откатывается вместе с транзакцией.
