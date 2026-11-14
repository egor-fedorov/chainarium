//! DEC-31: immutable native history inspection. No execution or financial model.
mod compare;
mod diff;
mod entities;
use crate::{Amount, Run, World, run::digest};
pub use compare::*;
pub use diff::*;
pub use entities::*;
use serde::Serialize;
use std::sync::Arc;

pub const MODEL: &str = "history-explorer-v1";
pub const MAX_CHECKPOINT: usize = 64 * 1024 * 1024;
pub const MAX_REPORT: usize = 8 * 1024 * 1024;
pub const MAX_BLOCKS: usize = 10000;
pub const PAGE: usize = 64;

/// Bounded response serialization also applies to CLI/downloads, not just HTTP.
pub fn report_bytes(value: &impl Serialize) -> Result<Vec<u8>, String> {
    struct Writer(Vec<u8>);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > MAX_REPORT.saturating_sub(self.0.len()) {
                return Err(std::io::Error::other("Explorer report exceeds 8 MiB"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer(vec![]);
    serde_json::to_writer(&mut writer, value).map_err(|e| e.to_string())?;
    Ok(writer.0)
}
#[derive(Clone)]
pub struct History {
    pub run: Arc<Run>,
    pub key: String,
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Description {
    pub key: String,
    pub run_id: String,
    pub build_id: String,
    pub name: String,
    pub blocks: usize,
    pub through: Amount,
    pub planned_blocks: usize,
    pub traces: String,
}
impl History {
    /// For a controller's already verified live Run. Never advances that Run.
    pub fn capture(run: Run) -> Result<Self, String> {
        if run.blocks.len() > MAX_BLOCKS {
            return Err("Explorer prefix exceeds 10000 blocks".into());
        }
        let bytes = run.checkpoint()?;
        if bytes.len() > MAX_CHECKPOINT {
            return Err("Explorer checkpoint exceeds 64 MiB".into());
        }
        let h = Self {
            run: Arc::new(run),
            key: digest(&bytes),
        };
        report_bytes(&h.description())?; // Must succeed before a controller replaces its slot.
        Ok(h)
    }
    pub fn restore(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_CHECKPOINT {
            return Err("Explorer checkpoint exceeds 64 MiB".into());
        }
        // Check the viewing bound before the native deterministic replay.
        let raw: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        if raw
            .pointer("/run/blocks")
            .and_then(|b| b.as_array())
            .is_none_or(|b| b.len() > MAX_BLOCKS)
        {
            return Err("invalid Explorer checkpoint prefix".into());
        }
        drop(raw);
        Self::capture(Run::restore(bytes)?)
    }
    pub fn description(&self) -> Description {
        Description {
            key: self.key.clone(),
            run_id: self.run.run_id.clone(),
            build_id: self.run.build_id.clone(),
            name: self.run.scenario.name.clone(),
            blocks: self.run.blocks.len(),
            through: self.run.state.timestamp,
            planned_blocks: self.run.scenario.rounds(),
            traces: "complete".into(),
        }
    }
    pub fn world(&self, block: usize) -> Result<&World, String> {
        if block == 0 {
            Ok(&self.run.initial)
        } else {
            self.run
                .blocks
                .get(block - 1)
                .map(|b| &b.state)
                .ok_or_else(|| "unknown committed Explorer block".into())
        }
    }
    pub fn as_of(&self, time: Amount) -> Result<usize, String> {
        if time > self.run.state.timestamp {
            return Err("time exceeds committed Explorer prefix".into());
        }
        Ok(self.run.blocks.partition_point(|b| b.timestamp <= time))
    }
    pub fn ensure_key(&self, key: &str) -> Result<(), String> {
        if self.key == key {
            Ok(())
        } else {
            Err("stale Explorer snapshot key".into())
        }
    }
    pub fn observation(
        &self,
        block: usize,
        agent: &str,
        account: &str,
    ) -> Result<crate::scheduler::AgentObservation, String> {
        let world = self.world(block)?;
        let empty = crate::scheduler::State::default();
        let scheduler = block
            .checked_sub(1)
            .and_then(|i| self.run.blocks[i].scheduler.as_ref())
            .map(|r| &r.state)
            .unwrap_or(&empty);
        scheduler.observation(&self.run.scenario, world, agent, account)
    }
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Page<T> {
    pub total: usize,
    pub offset: usize,
    pub next: Option<usize>,
    pub items: Vec<T>,
}
pub fn page<T>(items: impl IntoIterator<Item = T>, offset: usize) -> Result<Page<T>, String> {
    let items: Vec<_> = items.into_iter().collect();
    let total = items.len();
    if offset > total {
        return Err("Explorer page offset exceeds total".into());
    }
    let end = total.min(offset.saturating_add(PAGE));
    Ok(Page {
        total,
        offset,
        next: (end < total).then_some(end),
        items: items.into_iter().skip(offset).take(PAGE).collect(),
    })
}
