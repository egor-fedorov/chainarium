use crate::{Amount, engine, model::*};
use rand_chacha::ChaCha12Rng;
use rand_core::{RngCore, SeedableRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value).map_err(|e| e.to_string())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Streams {
    root: Amount,
    positions: BTreeMap<String, Amount>,
}
impl Streams {
    pub fn new(root: Amount) -> Self {
        Self {
            root,
            positions: BTreeMap::new(),
        }
    }
    pub fn draw(&mut self, name: &str) -> Result<Amount, String> {
        let mut hash = Sha256::new();
        for part in ["chacha12-v1", &self.root.to_string(), name] {
            hash.update((part.len() as u64).to_be_bytes());
            hash.update(part.as_bytes());
        }
        let seed: [u8; 32] = hash.finalize().into();
        let pos = self.positions.get(name).copied().unwrap_or_default();
        let word_pos = pos
            .to_string()
            .parse::<u128>()
            .map_err(|_| "RNG stream exhausted")?;
        // ChaCha's word position is 68 bits. Never wrap/repeat a stream.
        if word_pos >= (1u128 << 68) - 2 {
            return Err("RNG stream exhausted".into());
        }
        let mut rng = ChaCha12Rng::from_seed(seed);
        rng.set_word_pos(word_pos);
        let n = Amount::from(rng.next_u64());
        self.positions
            .insert(name.into(), pos.checked_add(2.into())?);
        Ok(n)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub build_id: String,
    pub run_id: String,
    pub parent_checkpoint: Option<String>,
    pub scenario: Scenario,
    pub initial: World,
    pub state: World,
    pub blocks: Vec<Block>,
    rng: Streams,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    format: String,
    hash: String,
    run: Run,
}
impl Run {
    pub fn new(s: Scenario) -> Result<Self, String> {
        let initial = World::genesis(&s)?;
        let run_id = digest(&json(&s)?);
        let rng = Streams::new(s.seed);
        Ok(Self {
            build_id: crate::BUILD_ID.into(),
            run_id,
            parent_checkpoint: None,
            scenario: s,
            initial: initial.clone(),
            state: initial,
            blocks: vec![],
            rng,
        })
    }
    pub fn step(&mut self) -> Result<Option<&Block>, String> {
        let i = self.blocks.len();
        let Some(txs) = self.scenario.blocks.get(i) else {
            return Ok(None);
        };
        let decisions = crate::policy::decisions(&self.scenario, &self.state)?;
        let mut candidates = txs.clone();
        candidates.extend(decisions.iter().filter_map(|d| d.transaction.clone()));
        let mut block = engine::build_block(&self.scenario, &self.state, &candidates)
            .map_err(|e| format!("ModelError: {e}"))?;
        block.decisions = decisions;
        self.state = block.state.clone();
        self.blocks.push(block);
        Ok(self.blocks.last())
    }
    pub fn finish(&mut self) -> Result<(), String> {
        while self.step()?.is_some() {}
        Ok(())
    }
    pub fn checkpoint(&self) -> Result<Vec<u8>, String> {
        if self.state.block != Amount::from(self.blocks.len() as u64) {
            return Err("not a committed boundary".into());
        }
        self.state.invariant(&self.scenario)?;
        json(&Checkpoint {
            format: "chainarium-checkpoint-v1".into(),
            hash: digest(&json(self)?),
            run: self.clone(),
        })
    }
    pub fn restore(bytes: &[u8]) -> Result<Self, String> {
        let restore = || -> Result<Self, String> {
            let cp: Checkpoint = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
            if cp.format != "chainarium-checkpoint-v1" || cp.hash != digest(&json(&cp.run)?) {
                return Err("checkpoint version/checksum".into());
            }
            if cp.run.build_id != crate::BUILD_ID {
                return Err("checkpoint build mismatch".into());
            }
            cp.run.scenario.validate()?;
            // Re-execution validates ledger, protocol state, receipts and cursor, not only a checksum.
            let mut replay = Self::new(cp.run.scenario.clone())?;
            if cp.run.blocks.len() > replay.scenario.blocks.len() {
                return Err("checkpoint cursor".into());
            }
            for _ in &cp.run.blocks {
                replay.step()?;
            }
            if cp.run.initial != replay.initial
                || cp.run.blocks != replay.blocks
                || cp.run.state != replay.state
                || cp.run.rng != replay.rng
            {
                return Err("checkpoint replay mismatch".into());
            }
            Ok(cp.run)
        };
        restore().map_err(|e| format!("ModelError: {e}"))
    }
    pub fn fork(&self, label: &str) -> Result<Self, String> {
        let hash = digest(&self.checkpoint()?);
        let mut fork = self.clone();
        fork.run_id = digest(&json(&(self.run_id.clone(), hash.clone(), label))?);
        fork.parent_checkpoint = Some(hash);
        Ok(fork)
    }
    pub fn export(&self, path: &Path) -> Result<(), String> {
        fs::create_dir(path).map_err(|e| format!("create new export directory: {e}"))?;
        let write =
            |name: &str, bytes: &[u8]| fs::write(path.join(name), bytes).map_err(|e| e.to_string());
        write(
            "scenario.json",
            &serde_json::to_vec_pretty(&self.scenario).map_err(|e| e.to_string())?,
        )?;
        write("run.json", &json(self)?)?;
        write("checkpoint.json", &self.checkpoint()?)?;
        let manifest = serde_json::json!({"schema":"chainarium-run-m1-v1","run_id":self.run_id,"parent_checkpoint":self.parent_checkpoint,"scenario_sha256":digest(&json(&self.scenario)?),"model_versions":self.scenario.model_versions,"build":crate::BUILD_ID,"package_version":env!("CARGO_PKG_VERSION"),"seed":self.scenario.seed,"traces":"complete","valuation_policy":"not-implemented-no-PnL-claims","checkpoint_boundary":"committed-block-end"});
        write("manifest.json", &json(&manifest)?)?;
        let mut events = String::new();
        let mut csv = String::from("block,tx,status,used,fee,refund\n");
        for b in &self.blocks {
            for event in &b.system_events {
                events.push_str(&serde_json::to_string(&serde_json::json!({"block":b.number,"phase":"SystemTransition","event":event})).map_err(|e|e.to_string())?);
                events.push('\n');
            }
            for r in &b.receipts {
                for event in &r.events {
                    events.push_str(
                        &serde_json::to_string(
                            &serde_json::json!({"block":b.number,"tx":r.tx_id,"event":event}),
                        )
                        .map_err(|e| e.to_string())?,
                    );
                    events.push('\n');
                }
                let escape = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
                csv.push_str(&format!(
                    "{},{},{},{},{},{}\n",
                    b.number,
                    escape(&r.tx_id),
                    escape(&r.status),
                    r.used,
                    r.fee,
                    r.refund
                ));
            }
        }
        write("events.jsonl", events.as_bytes())?;
        write("transactions.csv", csv.as_bytes())?;
        let report = format!(
            "# {}\n\nNative DeFi demonstration; v1.0 is incomplete.\n\n{} committed blocks; {} receipts. Amounts are exact atoms. PnL is not computed.\n",
            self.scenario.name,
            self.blocks.len(),
            self.blocks.iter().map(|b| b.receipts.len()).sum::<usize>()
        );
        write("report.md", report.as_bytes())
    }
}
/// Persist a new checkpoint without overwriting user files. Hard-link publication is atomic on one filesystem.
pub fn save_checkpoint(path: &Path, run: &Run) -> Result<(), String> {
    use std::io::Write;
    if path.exists() {
        return Err("checkpoint already exists".into());
    }
    let tmp = path.with_extension("json.pending");
    let bytes = run.checkpoint()?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    // Atomically publish without replacing a concurrently created user file.
    fs::hard_link(&tmp, path).map_err(|e| e.to_string())?;
    fs::remove_file(tmp).map_err(|e| e.to_string())
}
