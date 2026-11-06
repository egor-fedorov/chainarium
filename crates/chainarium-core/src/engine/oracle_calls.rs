use super::*;
use crate::{
    credit::OracleRecord,
    twap::{FeedKind, QueryTrace},
};
impl Context<'_> {
    pub(super) fn oracle_call(&mut self, w: &mut World, c: &Call) -> Result<(), ExecutionError> {
        let (feed, sequence, update_id) = match c {
            Call::PublishOracle {
                feed,
                sequence,
                update_id,
                ..
            }
            | Call::PublishSpot {
                feed,
                sequence,
                update_id,
                ..
            }
            | Call::PublishTwap {
                feed,
                sequence,
                update_id,
                ..
            } => (feed, sequence, update_id),
            _ => return Err(Model("not an oracle call".into())),
        };
        let cfg = self.s.feeds.get(feed).ok_or("unknown feed")?.clone();
        if !cfg.publishers.contains(&self.tx.sender) {
            return Err(Protocol("unauthorized oracle publisher".into()));
        }
        let (price, observed_at) = match (&cfg.kind, c) {
            (
                FeedKind::External,
                Call::PublishOracle {
                    source_id,
                    price,
                    observed_at,
                    ..
                },
            ) => {
                if source_id != &cfg.source_id {
                    return Err(Protocol("oracle validity".into()));
                }
                (price.clone(), *observed_at)
            }
            (FeedKind::NativeSpot { source }, Call::PublishSpot { .. }) => {
                let current = w
                    .price_history
                    .get(source)
                    .and_then(|h| h.segments.back())
                    .and_then(|s| s.price.as_ref())
                    .ok_or("spot unavailable")?;
                (
                    current.price(&cfg.base_asset, &cfg.quote_asset)?,
                    w.timestamp,
                )
            }
            (FeedKind::NativeTwap { source, window }, Call::PublishTwap { end, .. }) => {
                let history = w
                    .price_history
                    .get(source)
                    .ok_or_else(|| Model("missing TWAP history".into()))?;
                let mut segments = vec![];
                let mut exact = None;
                let result = (|| -> Result<crate::amount::Price, ExecutionError> {
                    let average =
                        history.average(*end, *window, &mut || self.charge(1), &mut segments)?;
                    exact = Some(average.clone());
                    Ok(average.published_price(&cfg.base_asset, &cfg.quote_asset)?)
                })();
                let outcome = match &result {
                    Ok(_) => "Ready".into(),
                    Err(Protocol(e)) => e.clone(),
                    Err(Exhausted) => "ResourceExhausted".into(),
                    Err(Model(e)) => format!("ModelError: {e}"),
                };
                self.trace
                    .last_mut()
                    .ok_or_else(|| Model("missing TWAP trace".into()))?
                    .twap = Some(QueryTrace {
                    source: source.clone(),
                    end: *end,
                    window: *window,
                    segments,
                    exact_average: exact,
                    outcome,
                });
                (result?, *end)
            }
            _ => return Err(Protocol("oracle call does not match feed kind".into())),
        };
        if price.base() != cfg.base_asset
            || price.quote() != cfg.quote_asset
            || observed_at > w.timestamp
            || update_id.is_empty()
        {
            return Err(Protocol("oracle validity".into()));
        }
        let state = Arc::make_mut(&mut w.feeds)
            .get_mut(feed)
            .ok_or_else(|| Model("missing feed state".into()))?;
        if state.seen_updates.contains(update_id)
            || state
                .record
                .as_ref()
                .is_some_and(|r| *sequence <= r.sequence || observed_at < r.observed_at)
        {
            return Err(Protocol("oracle update out of order".into()));
        }
        state.record = Some(OracleRecord {
            price,
            source_id: cfg.source_id,
            observed_at,
            published_at: w.timestamp,
            sequence: *sequence,
            update_id: update_id.clone(),
        });
        state.seen_updates.insert(update_id.clone());
        self.events.push(Event {
            kind: "OraclePublished".into(),
            from: self.tx.sender.clone(),
            to: feed.clone(),
            asset: format!("sequence:{feed}"),
            amount: *sequence,
        });
        crate::credit::invariant(self.s, w).map_err(Model)?;
        Ok(())
    }
}
