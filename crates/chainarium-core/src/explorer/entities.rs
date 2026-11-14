use super::*;
use crate::{Amount, Event};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Account,
    Asset,
    Agent,
    Amm,
    Clmm,
    Book,
    Lending,
    Oracle,
    PriceSource,
    Flash,
    Hook,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub kind: Kind,
    pub id: String,
}
fn entity(kind: Kind, id: &str) -> Entity {
    Entity {
        kind,
        id: id.into(),
    }
}
fn catalog(h: &History) -> Vec<Entity> {
    let s = &h.run.scenario;
    let mut all = vec![];
    for (kind, ids) in [
        (Kind::Account, s.accounts.keys().collect::<Vec<_>>()),
        (Kind::Asset, s.assets.keys().collect()),
        (Kind::Agent, s.agents.keys().collect()),
        (Kind::Amm, s.pools().map(|(id, _)| id).collect()),
        (Kind::Clmm, s.clmm_pools.keys().collect()),
        (Kind::Book, s.books.keys().collect()),
        (Kind::Lending, s.lending.keys().collect()),
        (Kind::Oracle, s.feeds.keys().collect()),
        (Kind::PriceSource, s.price_sources.keys().collect()),
        (Kind::Flash, s.flash_providers.keys().collect()),
        (Kind::Hook, s.hooks.keys().collect()),
    ] {
        all.extend(ids.into_iter().map(|id| entity(kind, id)));
    }
    all.sort();
    all
}
pub fn entities(h: &History, offset: usize) -> Result<Page<Entity>, String> {
    page(catalog(h), offset)
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AgentChoice {
    pub id: String,
    pub accounts: Vec<String>,
}
pub fn agents(h: &History, offset: usize) -> Result<Page<AgentChoice>, String> {
    page(
        h.run.scenario.agents.iter().map(|(id, a)| AgentChoice {
            id: id.clone(),
            accounts: a.controlled_accounts.iter().cloned().collect(),
        }),
        offset,
    )
}
pub fn entity_view(h: &History, block: usize, e: &Entity, offset: usize) -> Result<Value, String> {
    let s = &h.run.scenario;
    let w = h.world(block)?;
    let id = &e.id;
    if !catalog(h).contains(e) {
        return Err("unknown Explorer entity".into());
    }
    Ok(match e.kind {
        Kind::Account => {
            json!({"entity":e,"nonce":w.accounts[id].nonce,"balances":page(w.accounts[id].balances.iter().map(|(asset,amount)|json!({"asset":asset,"amount":amount})),offset)?})
        }
        Kind::Asset => json!({"entity":e,"config":s.assets[id]}),
        Kind::Agent => json!({"entity":e,"config":s.agents[id]}),
        Kind::Amm => {
            let p = s.pool(id).ok_or("unknown pool")?;
            json!({"entity":e,"model":if s.stable_pools.contains_key(id){"StableSwap"}else{"CPMM"},"config":p,"stable":s.stable_pools.get(id),"reserves":[{"asset":p.x,"amount":w.balance(&p.account,&p.x)},{"asset":p.y,"amount":w.balance(&p.account,&p.y)}],"shares":w.shares.get(id),"owners":w.lp_balances.get(id)})
        }
        Kind::Clmm => json!({"entity":e,"config":s.clmm_pools[id],"state":w.clmm.get(id)}),
        Kind::Book => json!({"entity":e,"config":s.books[id],"state":w.books.get(id)}),
        Kind::Lending => {
            json!({"entity":e,"config":s.lending[id],"cash":w.balance(&s.lending[id].cash_account,&s.lending[id].debt_asset),"state":w.lending.get(id)})
        }
        Kind::Oracle => json!({"entity":e,"config":s.feeds[id],"state":w.feeds.get(id)}),
        Kind::PriceSource => {
            json!({"entity":e,"config":s.price_sources[id],"state":w.price_history.get(id)})
        }
        Kind::Flash => {
            json!({"entity":e,"config":s.flash_providers[id],"cash":w.balance(&s.flash_providers[id].account,&s.flash_providers[id].asset)})
        }
        Kind::Hook => {
            json!({"entity":e,"config":s.hooks[id],"state":w.hooks.get(id),"balances":page(w.accounts[&s.hooks[id].account].balances.iter().map(|(asset,amount)|json!({"asset":asset,"amount":amount})),offset)?})
        }
    })
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Edge {
    pub from: Entity,
    pub to: Entity,
    pub relation: String,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Graph {
    pub nodes: Vec<Entity>,
    pub edges: Vec<Edge>,
    pub total_nodes: usize,
    pub total_edges: usize,
    pub truncated: bool,
}
pub fn graph(h: &History) -> Graph {
    let s = &h.run.scenario;
    let all: Vec<_> = catalog(h)
        .into_iter()
        .filter(|e| !matches!(e.kind, Kind::Account | Kind::Agent))
        .collect();
    let nodes: Vec<_> = all.iter().take(256).cloned().collect();
    let visible: BTreeSet<_> = nodes.iter().cloned().collect();
    let mut edges = vec![];
    let mut add = |from: Entity, to: Entity, relation: &str| {
        edges.push(Edge {
            from,
            to,
            relation: relation.into(),
        })
    };
    for (id, p) in s.pools() {
        for a in [&p.x, &p.y] {
            add(
                entity(Kind::Amm, id),
                entity(Kind::Asset, a),
                "trading pair",
            );
        }
    }
    for (id, p) in &s.clmm_pools {
        for a in [&p.x, &p.q] {
            add(
                entity(Kind::Clmm, id),
                entity(Kind::Asset, a),
                "trading pair",
            );
        }
    }
    for (id, p) in &s.books {
        for a in [&p.base_asset, &p.quote_asset] {
            add(
                entity(Kind::Book, id),
                entity(Kind::Asset, a),
                "trading pair",
            );
        }
    }
    for (id, p) in &s.lending {
        add(
            entity(Kind::Lending, id),
            entity(Kind::Asset, &p.debt_asset),
            "debt",
        );
        add(
            entity(Kind::Lending, id),
            entity(Kind::Asset, &p.collateral_asset),
            "collateral",
        );
        add(
            entity(Kind::Lending, id),
            entity(Kind::Oracle, &p.feed),
            "risk price",
        );
    }
    for (id, p) in &s.feeds {
        for a in [&p.base_asset, &p.quote_asset] {
            add(
                entity(Kind::Oracle, id),
                entity(Kind::Asset, a),
                "price units",
            );
        }
        if let Some(source) = p.kind.source() {
            add(
                entity(Kind::Oracle, id),
                entity(Kind::PriceSource, source),
                "native source",
            );
        }
    }
    for (id, p) in &s.price_sources {
        add(
            entity(Kind::PriceSource, id),
            entity(Kind::Amm, &p.pool),
            "observes pool",
        );
    }
    for (id, p) in &s.flash_providers {
        add(
            entity(Kind::Flash, id),
            entity(Kind::Asset, &p.asset),
            "loan asset",
        );
    }
    for (id, p) in &s.hooks {
        add(
            entity(Kind::Hook, id),
            entity(Kind::Amm, &p.pool),
            "bound hook",
        );
    }
    let total_edges = edges.len();
    let total_nodes = all.len();
    let edges: Vec<_> = edges
        .into_iter()
        .filter(|e| visible.contains(&e.from) && visible.contains(&e.to))
        .take(512)
        .collect();
    Graph {
        truncated: nodes.len() < total_nodes || edges.len() < total_edges,
        nodes,
        edges,
        total_nodes,
        total_edges,
    }
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ReceiptRow {
    pub index: usize,
    pub tx_id: String,
    pub sender: String,
    pub status: String,
    pub error: Option<String>,
    pub used: Amount,
    pub fee: Amount,
    pub calls: usize,
}
fn row(index: usize, r: &crate::Receipt) -> ReceiptRow {
    ReceiptRow {
        index,
        tx_id: r.tx_id.clone(),
        sender: r.sender.clone(),
        status: r.status.clone(),
        error: r.error.clone(),
        used: r.used,
        fee: r.fee,
        calls: r.trace.len(),
    }
}
pub fn receipts(h: &History, block: usize, offset: usize) -> Result<Page<ReceiptRow>, String> {
    h.world(block)?;
    page(
        block
            .checked_sub(1)
            .into_iter()
            .flat_map(|i| h.run.blocks[i].receipts.iter().enumerate())
            .map(|(i, r)| row(i, r)),
        offset,
    )
}
pub fn rejections(
    h: &History,
    block: usize,
    offset: usize,
) -> Result<Page<crate::Rejected>, String> {
    h.world(block)?;
    page(
        block
            .checked_sub(1)
            .into_iter()
            .flat_map(|i| h.run.blocks[i].rejected.iter())
            .cloned(),
        offset,
    )
}
pub fn system_events(h: &History, block: usize, offset: usize) -> Result<Page<Event>, String> {
    h.world(block)?;
    page(
        block
            .checked_sub(1)
            .into_iter()
            .flat_map(|i| h.run.blocks[i].system_events.iter())
            .cloned(),
        offset,
    )
}
fn receipt(h: &History, block: usize, index: usize) -> Result<&crate::Receipt, String> {
    block
        .checked_sub(1)
        .and_then(|i| h.run.blocks.get(i))
        .and_then(|b| b.receipts.get(index))
        .ok_or_else(|| "unknown Explorer receipt".into())
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct CallRow {
    pub index: usize,
    pub path: String,
    pub operation: String,
    pub callee: String,
    pub outcome: String,
    pub used: Amount,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ReceiptView {
    pub receipt: ReceiptRow,
    pub reserved: Amount,
    pub refund: Amount,
    pub fee_asset: String,
    pub charges: Vec<crate::Charge>,
    pub canonical_events: Page<Event>,
    pub calls: Page<CallRow>,
}
pub fn receipt_view(
    h: &History,
    block: usize,
    index: usize,
    event_offset: usize,
    call_offset: usize,
) -> Result<ReceiptView, String> {
    let r = receipt(h, block, index)?;
    Ok(ReceiptView {
        receipt: row(index, r),
        reserved: r.reserved,
        refund: r.refund,
        fee_asset: h.run.scenario.chain.fee_asset.clone(),
        charges: r.charges.clone(),
        canonical_events: page(r.events.clone(), event_offset)?,
        calls: page(
            r.trace.iter().enumerate().map(|(index, t)| CallRow {
                index,
                path: t.path.clone(),
                operation: t.operation.clone(),
                callee: t.callee.clone(),
                outcome: t.outcome.clone(),
                used: t.used,
            }),
            call_offset,
        )?,
    })
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct CallView {
    pub index: usize,
    pub committed: bool,
    pub diagnostic: Value,
    pub attempt_events: Page<Event>,
}
pub fn call_view(
    h: &History,
    block: usize,
    index: usize,
    call: usize,
    event_offset: usize,
) -> Result<CallView, String> {
    let r = receipt(h, block, index)?;
    let t = r.trace.get(call).ok_or("unknown Explorer call")?;
    let mut diagnostic = serde_json::to_value(t).map_err(|e| e.to_string())?;
    diagnostic
        .as_object_mut()
        .ok_or("invalid native trace")?
        .remove("events");
    Ok(CallView {
        index: call,
        committed: r.status == "Success",
        diagnostic,
        attempt_events: page(t.events.clone(), event_offset)?,
    })
}
