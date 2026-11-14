use super::{ApiError, Shared, bad, current};
use axum::{
    Extension, Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::header,
    response::IntoResponse,
    routing::{get, post},
};
use chainarium_core::{
    Amount,
    explorer::{self as e, History},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Slot {
    A,
    B,
}
#[derive(Clone, Default)]
struct Store {
    a: Option<History>,
    b: Option<History>,
}
type Cache = Arc<Mutex<Store>>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Keys {
    a: Option<String>,
    b: Option<String>,
}
impl Store {
    fn status(&self) -> Value {
        json!({"a":self.a.as_ref().map(History::description),"b":self.b.as_ref().map(History::description)})
    }
    fn get(&self, slot: Slot) -> Result<&History, String> {
        match slot {
            Slot::A => &self.a,
            Slot::B => &self.b,
        }
        .as_ref()
        .ok_or_else(|| "Explorer slot is empty".into())
    }
    fn insert(&mut self, slot: Slot, h: History) -> Result<Vec<u8>, String> {
        let mut next = self.clone();
        match slot {
            Slot::A => next.a = Some(h),
            Slot::B => next.b = Some(h),
        }
        let bytes = e::report_bytes(&next.status())?;
        *self = next;
        Ok(bytes)
    }
    fn checked(&self, keys: &Keys) -> Result<Self, String> {
        for (h, key) in [(&self.a, &keys.a), (&self.b, &keys.b)] {
            match (h, key) {
                (None, None) => {}
                (Some(h), Some(k)) => h.ensure_key(k)?,
                _ => return Err("stale Explorer snapshot keys".into()),
            }
        }
        Ok(self.clone())
    }
    fn pair(&self) -> Result<(Slot, &History, Option<&History>), String> {
        match (&self.a, &self.b) {
            (Some(a), b) => Ok((Slot::A, a, b.as_ref())),
            (None, Some(b)) => Ok((Slot::B, b, None)),
            _ => Err("load an Explorer history first".into()),
        }
    }
}
fn lock(c: &Cache) -> Result<std::sync::MutexGuard<'_, Store>, ApiError> {
    c.lock().map_err(|_| bad("Explorer controller unavailable"))
}
fn body(bytes: Vec<u8>) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "application/json")], bytes)
}
async fn status(Extension(c): Extension<Cache>) -> Result<impl IntoResponse, ApiError> {
    Ok(body(e::report_bytes(&lock(&c)?.status()).map_err(bad)?))
}
async fn pin(
    State(live): State<Shared>,
    Extension(c): Extension<Cache>,
    Path(slot): Path<Slot>,
) -> Result<impl IntoResponse, ApiError> {
    let run = current(&live)?
        .as_ref()
        .ok_or_else(|| bad("no live run"))?
        .clone();
    let history = History::capture(run).map_err(bad)?;
    let bytes = lock(&c)?.insert(slot, history).map_err(bad)?;
    Ok(body(bytes))
}
async fn import(
    Extension(c): Extension<Cache>,
    Path(slot): Path<Slot>,
    bytes: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let history = History::restore(&bytes).map_err(bad)?;
    let bytes = lock(&c)?.insert(slot, history).map_err(bad)?;
    Ok(body(bytes))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Key {
    key: String,
}
async fn checkpoint(
    Extension(c): Extension<Cache>,
    Path(slot): Path<Slot>,
    Query(key): Query<Key>,
) -> Result<impl IntoResponse, ApiError> {
    let h = lock(&c)?.get(slot).map_err(bad)?.clone();
    h.ensure_key(&key.key).map_err(bad)?;
    Ok(body(h.run.checkpoint().map_err(bad)?))
}
#[derive(Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
enum View {
    Index {},
    Compare {
        time: Amount,
    },
    Balances {
        time: Amount,
        account: String,
        #[serde(default)]
        offset: usize,
    },
    Entities {
        slot: Slot,
        #[serde(default)]
        offset: usize,
    },
    Entity {
        slot: Slot,
        block: usize,
        entity: e::Entity,
        #[serde(default)]
        offset: usize,
    },
    Graph {
        slot: Slot,
    },
    Agents {
        slot: Slot,
        #[serde(default)]
        offset: usize,
    },
    Observation {
        slot: Slot,
        block: usize,
        agent: String,
        account: String,
    },
    Receipts {
        slot: Slot,
        block: usize,
        #[serde(default)]
        offset: usize,
    },
    Rejections {
        slot: Slot,
        block: usize,
        #[serde(default)]
        offset: usize,
    },
    SystemEvents {
        slot: Slot,
        block: usize,
        #[serde(default)]
        offset: usize,
    },
    Receipt {
        slot: Slot,
        block: usize,
        index: usize,
        #[serde(default)]
        event_offset: usize,
        #[serde(default)]
        call_offset: usize,
    },
    Call {
        slot: Slot,
        block: usize,
        index: usize,
        call: usize,
        #[serde(default)]
        event_offset: usize,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    keys: Keys,
    view: View,
}
fn value(v: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}
fn inspect(s: &Store, view: View) -> Result<Value, String> {
    let (primary, a, b) = s.pair()?;
    let data = match view {
        View::Index {} => value(e::index(a, b)?)?,
        View::Compare { time } => value(e::compare(a, b, time)?)?,
        View::Balances {
            time,
            account,
            offset,
        } => value(e::balances(a, b, time, &account, offset)?)?,
        View::Entities { slot, offset } => value(e::entities(s.get(slot)?, offset)?)?,
        View::Entity {
            slot,
            block,
            entity,
            offset,
        } => e::entity_view(s.get(slot)?, block, &entity, offset)?,
        View::Graph { slot } => value(e::graph(s.get(slot)?))?,
        View::Agents { slot, offset } => value(e::agents(s.get(slot)?, offset)?)?,
        View::Observation {
            slot,
            block,
            agent,
            account,
        } => value(s.get(slot)?.observation(block, &agent, &account)?)?,
        View::Receipts {
            slot,
            block,
            offset,
        } => value(e::receipts(s.get(slot)?, block, offset)?)?,
        View::Rejections {
            slot,
            block,
            offset,
        } => value(e::rejections(s.get(slot)?, block, offset)?)?,
        View::SystemEvents {
            slot,
            block,
            offset,
        } => value(e::system_events(s.get(slot)?, block, offset)?)?,
        View::Receipt {
            slot,
            block,
            index,
            event_offset,
            call_offset,
        } => value(e::receipt_view(
            s.get(slot)?,
            block,
            index,
            event_offset,
            call_offset,
        )?)?,
        View::Call {
            slot,
            block,
            index,
            call,
            event_offset,
        } => value(e::call_view(
            s.get(slot)?,
            block,
            index,
            call,
            event_offset,
        )?)?,
    };
    Ok(json!({"primary":primary,"data":data}))
}
async fn query(
    Extension(c): Extension<Cache>,
    Json(input): Json<Request>,
) -> Result<impl IntoResponse, ApiError> {
    let snapshot = lock(&c)?.checked(&input.keys).map_err(bad)?;
    let view = inspect(&snapshot, input.view).map_err(bad)?;
    Ok(body(e::report_bytes(&view).map_err(bad)?))
}
pub(super) fn routes() -> Router<Shared> {
    Router::new()
        .route("/api/explorer", get(status))
        .route("/api/explorer/{slot}/pin", post(pin))
        .route(
            "/api/explorer/{slot}/checkpoint",
            get(checkpoint)
                .post(import)
                .layer(DefaultBodyLimit::max(e::MAX_CHECKPOINT)),
        )
        .route("/api/explorer/query", post(query))
        .route(
            "/explorer.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../../../web/explorer.js"),
                )
            }),
        )
        .layer(Extension(Arc::new(Mutex::new(Store::default()))))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run() -> chainarium_core::Run {
        chainarium_core::Run::new(
            chainarium_core::Scenario::parse(include_bytes!(
                "../../../scenarios/explorer/synchronous-a.json"
            ))
            .unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn slots_are_independent_from_live_run_and_stale_queries_reject() {
        let mut live = run();
        let mut store = Store::default();
        let a = History::capture(live.clone()).unwrap();
        store.insert(Slot::A, a.clone()).unwrap();
        let keys = Keys {
            a: Some(a.key.clone()),
            b: None,
        };
        assert!(store.checked(&keys).is_ok());
        live.step().unwrap();
        let b = History::capture(live.clone()).unwrap();
        store.insert(Slot::B, b.clone()).unwrap();
        assert_eq!(store.get(Slot::A).unwrap().run.blocks.len(), 0);
        assert!(store.checked(&keys).is_err());
        let keys = Keys {
            a: Some(a.key),
            b: Some(b.key),
        };
        assert!(store.checked(&keys).is_ok());
        let before = live.checkpoint().unwrap();
        inspect(&store, View::Compare { time: 0.into() }).unwrap();
        assert_eq!(live.checkpoint().unwrap(), before);
        let old = store.status();
        let failed = History::restore(b"{}").and_then(|h| store.insert(Slot::A, h));
        assert!(failed.is_err());
        assert_eq!(store.status(), old);
    }
    #[test]
    fn only_b_is_identified_honestly_and_invalid_requests_preserve_slots() {
        let mut store = Store::default();
        store
            .insert(Slot::B, History::capture(run()).unwrap())
            .unwrap();
        let result = inspect(&store, View::Compare { time: 0.into() }).unwrap();
        assert_eq!(result["primary"], "B");
        assert!(result["data"]["b"].is_null());
        let before = store.status();
        assert!(
            inspect(
                &store,
                View::Entity {
                    slot: Slot::A,
                    block: 0,
                    entity: e::Entity {
                        kind: e::Kind::Account,
                        id: "alice".into()
                    },
                    offset: 0
                }
            )
            .is_err()
        );
        assert_eq!(store.status(), before);
        assert!(
            serde_json::from_value::<Request>(
                json!({"keys":{"a":null,"b":null},"view":{"type":"Compare","time":0}})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<Request>(
                json!({"keys":{"a":null,"b":null},"view":{"type":"Index","surprise":true}})
            )
            .is_err()
        );
    }
}
