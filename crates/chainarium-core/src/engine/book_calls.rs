use super::*;
use crate::book::{Fill, MAX_ORDERS, Order, OrderTrace, Side, TimeInForce};

impl Context<'_> {
    pub(super) fn book_call(&mut self, w: &mut World, call: &Call) -> Result<(), ExecutionError> {
        let (book, place) = match call {
            Call::PlaceOrder { book, .. } => (book, true),
            Call::CancelOrder { book, .. } => (book, false),
            _ => return Err(Model("invalid book dispatch".into())),
        };
        let cfg = self.s.books.get(book).ok_or("unknown order book")?.clone();
        let sender = self.tx.sender.clone();
        let ti = self.trace.len() - 1;
        if !place {
            let Call::CancelOrder { order_id, .. } = call else {
                unreachable!()
            };
            let order = w.books[book]
                .orders
                .get(order_id)
                .ok_or("unknown order id")?
                .clone();
            if order.owner != sender {
                return Err(Protocol("order owner required".into()));
            }
            let refund = cfg.locked(order.side, order.price_ticks, order.remaining_lots)?;
            let asset = cfg.reserve_asset(order.side);
            self.trace[ti].book = Some(OrderTrace {
                book: book.clone(),
                order_id: *order_id,
                side: order.side,
                price_ticks: order.price_ticks,
                requested_lots: order.remaining_lots,
                remaining_lots: order.remaining_lots,
                filled_lots: Amount::ZERO,
                base_filled: Amount::ZERO,
                taker_quote: Amount::ZERO,
                gross_quote: Amount::ZERO,
                fee: Amount::ZERO,
                reserve_asset: asset.into(),
                reserved: refund,
                refund: Amount::ZERO,
                resting_lots: order.remaining_lots,
                status: "Cancelling".into(),
                fills: vec![],
            });
            Arc::make_mut(&mut w.books)
                .get_mut(book)
                .unwrap()
                .remove(*order_id)
                .map_err(Model)?;
            self.transfer(w, &cfg.account, &sender, asset, refund)?;
            let trace = self.trace[ti].book.as_mut().unwrap();
            trace.refund = refund;
            trace.resting_lots = Amount::ZERO;
            trace.status = "Cancelled".into();
            self.events.push(Event {
                kind: "BookCancelled".into(),
                from: sender,
                to: book.clone(),
                asset: format!("LOTS:{book}"),
                amount: order.remaining_lots,
            });
            return Ok(());
        }
        let Call::PlaceOrder {
            side,
            price_ticks,
            lots,
            tif,
            ..
        } = call
        else {
            unreachable!()
        };
        if lots.is_zero() || price_ticks.is_zero() {
            return Err(Protocol("zero order lots/price".into()));
        }
        // Validate the limit price even for sells that would otherwise never
        // multiply it (e.g. an unmatched IOC).
        cfg.quote_per_lot(*price_ticks)?;
        lots.checked_mul(cfg.lot_atoms)?;
        let reserved = cfg.reservation(*side, *price_ticks, *lots)?;
        let asset = cfg.reserve_asset(*side);
        let id = w.books[book].next_id;
        let next_id = id.checked_add(1.into())?;
        let mut remaining = *lots;
        let mut unspent = reserved;
        self.trace[ti].book = Some(OrderTrace {
            book: book.clone(),
            order_id: id,
            side: *side,
            price_ticks: *price_ticks,
            requested_lots: *lots,
            remaining_lots: *lots,
            filled_lots: Amount::ZERO,
            base_filled: Amount::ZERO,
            taker_quote: Amount::ZERO,
            gross_quote: Amount::ZERO,
            fee: Amount::ZERO,
            reserve_asset: asset.into(),
            reserved,
            refund: Amount::ZERO,
            resting_lots: Amount::ZERO,
            status: "Unfilled".into(),
            fills: vec![],
        });
        Arc::make_mut(&mut w.books).get_mut(book).unwrap().next_id = next_id;
        self.transfer(w, &sender, &cfg.account, asset, reserved)?;
        let mut matches = 0;
        while !remaining.is_zero() {
            let Some(maker_id) = w.books[book].best(*side) else {
                break;
            };
            let maker = w.books[book]
                .orders
                .get(&maker_id)
                .ok_or_else(|| Model("missing maker".into()))?
                .clone();
            let crosses = match side {
                Side::Buy => maker.price_ticks <= *price_ticks,
                Side::Sell => maker.price_ticks >= *price_ticks,
            };
            if !crosses {
                break;
            }
            if matches >= MAX_ORDERS {
                return Err(Model("book match bound".into()));
            }
            matches += 1;
            let charged = self.charge(1);
            self.charges.last_mut().unwrap().phase = "BookMatch".into();
            charged?;
            if maker.owner == sender {
                return Err(Protocol("SelfTrade".into()));
            }
            let quantity = remaining.min(maker.remaining_lots);
            let base = quantity.checked_mul(cfg.lot_atoms)?;
            let gross = quantity.checked_mul(cfg.quote_per_lot(maker.price_ticks)?)?;
            let fee = cfg.fee(gross)?;
            let taker_quote = match side {
                Side::Buy => gross.checked_add(fee)?,
                Side::Sell => gross.checked_sub(fee)?,
            };
            let spent = match side {
                Side::Buy => taker_quote,
                Side::Sell => base,
            };
            unspent = unspent.checked_sub(spent).map_err(Model)?;
            match side {
                Side::Buy => {
                    self.transfer(w, &cfg.account, &sender, &cfg.base_asset, base)?;
                    self.transfer(w, &cfg.account, &maker.owner, &cfg.quote_asset, gross)?;
                }
                Side::Sell => {
                    self.transfer(w, &cfg.account, &maker.owner, &cfg.base_asset, base)?;
                    if !taker_quote.is_zero() {
                        self.transfer(w, &cfg.account, &sender, &cfg.quote_asset, taker_quote)?;
                    }
                }
            }
            if !fee.is_zero() {
                self.transfer(w, &cfg.account, &cfg.fee_account, &cfg.quote_asset, fee)?;
            }
            remaining = remaining.checked_sub(quantity).map_err(Model)?;
            let maker_remaining = maker.remaining_lots.checked_sub(quantity).map_err(Model)?;
            let state = Arc::make_mut(&mut w.books).get_mut(book).unwrap();
            if maker_remaining.is_zero() {
                state.remove(maker_id).map_err(Model)?;
            } else {
                state.orders.get_mut(&maker_id).unwrap().remaining_lots = maker_remaining;
            }
            let trace = self.trace[ti].book.as_mut().unwrap();
            trace.remaining_lots = remaining;
            trace.filled_lots = lots.checked_sub(remaining).map_err(Model)?;
            trace.base_filled = trace.base_filled.checked_add(base)?;
            trace.taker_quote = trace.taker_quote.checked_add(taker_quote)?;
            trace.gross_quote = trace.gross_quote.checked_add(gross)?;
            trace.fee = trace.fee.checked_add(fee)?;
            trace.status = if remaining.is_zero() {
                "Filled"
            } else {
                "Partial"
            }
            .into();
            trace.fills.push(Fill {
                maker_id,
                taker_id: id,
                maker: maker.owner.clone(),
                taker: sender.clone(),
                price_ticks: maker.price_ticks,
                lots: quantity,
                base,
                gross_quote: gross,
                fee,
                taker_quote,
                maker_remaining,
            });
            self.events.push(Event {
                kind: "BookMatch".into(),
                from: maker.owner,
                to: sender.clone(),
                asset: cfg.base_asset.clone(),
                amount: base,
            });
        }
        if *tif == TimeInForce::FOK && !remaining.is_zero() {
            return Err(Protocol("FOK not fully fillable".into()));
        }
        if *tif == TimeInForce::GTC && !remaining.is_zero() {
            let state = Arc::make_mut(&mut w.books).get_mut(book).unwrap();
            if state.orders.len() >= MAX_ORDERS {
                return Err(Protocol("book capacity".into()));
            }
            let locked = cfg.locked(*side, *price_ticks, remaining)?;
            unspent = unspent.checked_sub(locked).map_err(Model)?;
            state.insert(
                id,
                Order {
                    owner: sender.clone(),
                    side: *side,
                    price_ticks: *price_ticks,
                    remaining_lots: remaining,
                },
            );
            self.trace[ti].book.as_mut().unwrap().resting_lots = remaining;
            self.events.push(Event {
                kind: "BookRested".into(),
                from: sender.clone(),
                to: book.clone(),
                asset: format!("LOTS:{book}"),
                amount: remaining,
            });
        }
        if !unspent.is_zero() {
            self.transfer(w, &cfg.account, &sender, asset, unspent)?;
        }
        self.trace[ti].book.as_mut().unwrap().refund = unspent;
        Ok(())
    }
}
