pub type Allowances = BTreeMap<String, BTreeMap<String, BTreeMap<String, Amount>>>;
use crate::Amount;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub const SCHEMA: &str = "chainarium-scenario-m1-v1";
pub const MODELS: [&str; 5] = [
    "native-execution-v1",
    "native-cost-v1",
    "cpmm-v1",
    "scheduler-m1-v1",
    "chacha12-v1",
];
pub const M2_MODELS: [&str; 3] = ["cpmm-lp-v1", "bounded-route-v1", "strict-flash-provider-v1"];
pub const MAX_INPUT: usize = 1024 * 1024;
pub const MAX_SCENARIO: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub decimals: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Account {
    pub balances: BTreeMap<String, Amount>,
    pub nonce: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Pool {
    pub account: String,
    pub x: String,
    pub y: String,
    pub fee_bps: u16,
    pub owner: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FlashProvider {
    pub account: String,
    pub asset: String,
    pub fee_bps: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub controlled_accounts: BTreeSet<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Chain {
    pub id: String,
    pub fee_asset: String,
    pub fee_account: String,
    pub block_limit: Amount,
    pub interval: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    pub id: String,
    pub agent: String,
    pub sender: String,
    pub chain: String,
    pub nonce: Amount,
    pub limit: Amount,
    pub price: Amount,
    pub deadline: Amount,
    pub program: Vec<Call>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Call {
    LiquidateAndSell {
        market: String,
        borrower: String,
        max_repay: Amount,
        min_collateral: Amount,
        hops: Vec<crate::routing::heterogeneous::VenueHop>,
        min_out: Amount,
    },
    ExecuteRoute {
        asset_in: String,
        amount: Amount,
        hops: Vec<crate::routing::heterogeneous::VenueHop>,
        min_out: Amount,
        residual_policy: crate::routing::heterogeneous::ResidualPolicy,
    },
    MintPosition {
        pool: String,
        lower: i32,
        upper: i32,
        liquidity: Amount,
        max_x: Amount,
        max_q: Amount,
    },
    BurnPosition {
        pool: String,
        position_id: Amount,
        liquidity: Amount,
        min_x: Amount,
        min_q: Amount,
    },
    CollectPosition {
        pool: String,
        position_id: Amount,
    },
    ClmmSwap {
        pool: String,
        asset_in: String,
        amount: Amount,
        sqrt_price_limit: Amount,
        min_out: Amount,
        allow_partial: bool,
    },
    PlaceOrder {
        book: String,
        side: crate::book::Side,
        price_ticks: Amount,
        lots: Amount,
        tif: crate::book::TimeInForce,
    },
    CancelOrder {
        book: String,
        order_id: Amount,
    },
    Transfer {
        to: String,
        asset: String,
        amount: Amount,
    },
    Approve {
        spender: String,
        asset: String,
        amount: Amount,
    },
    TransferFrom {
        from: String,
        to: String,
        asset: String,
        amount: Amount,
    },
    SwapExactIn {
        pool: String,
        asset_in: String,
        amount: Amount,
        min_out: Amount,
    },
    SwapOutput {
        pool: String,
        asset_in: String,
        output_of: String,
        min_out: Amount,
    },
    AddLiquidity {
        pool: String,
        max_x: Amount,
        max_y: Amount,
        min_shares: Amount,
    },
    RemoveLiquidity {
        pool: String,
        shares: Amount,
        min_x: Amount,
        min_y: Amount,
    },
    FlashLoan {
        provider: String,
        receiver: String,
        amount: Amount,
        callback: Vec<Call>,
    },
    AssertNetBalanceDelta {
        asset: String,
        baseline: Amount,
        minimum: Amount,
    },
    AssertTransactionNetGain {
        asset: String,
        minimum: Amount,
    },
    AssertBalanceDelta {
        account: String,
        asset: String,
        baseline: Amount,
        minimum: Amount,
    },
    Supply {
        market: String,
        amount: Amount,
        min_shares: Amount,
    },
    Withdraw {
        market: String,
        shares: Amount,
        min_assets: Amount,
    },
    DepositCollateral {
        market: String,
        amount: Amount,
    },
    WithdrawCollateral {
        market: String,
        amount: Amount,
    },
    SetCollateralUsage {
        market: String,
        enabled: bool,
    },
    Borrow {
        market: String,
        amount: Amount,
    },
    Repay {
        market: String,
        borrower: String,
        amount: Amount,
    },
    Liquidate {
        market: String,
        borrower: String,
        amount: Amount,
        min_collateral: Amount,
    },
    PublishSpot {
        feed: String,
        sequence: Amount,
        update_id: String,
    },
    PublishTwap {
        feed: String,
        end: Amount,
        sequence: Amount,
        update_id: String,
    },
    PublishOracle {
        feed: String,
        source_id: String,
        price: crate::amount::Price,
        observed_at: Amount,
        sequence: Amount,
        update_id: String,
    },
    AssertCondition {
        condition: bool,
        message: String,
    },
    AssertBalance {
        account: String,
        asset: String,
        minimum: Amount,
    },
    Sequence {
        calls: Vec<Call>,
    },
}
impl Call {
    pub fn label(&self) -> &'static str {
        match self {
            Self::ExecuteRoute { .. } => "ExecuteRoute",
            Self::LiquidateAndSell { .. } => "LiquidateAndSell",
            Self::Transfer { .. } => "Transfer",
            Self::Approve { .. } => "Approve",
            Self::TransferFrom { .. } => "TransferFrom",
            Self::MintPosition { .. } => "MintPosition",
            Self::BurnPosition { .. } => "BurnPosition",
            Self::CollectPosition { .. } => "CollectPosition",
            Self::ClmmSwap { .. } => "ClmmSwap",
            Self::PlaceOrder { .. } => "PlaceOrder",
            Self::CancelOrder { .. } => "CancelOrder",
            Self::SwapExactIn { .. } => "SwapExactIn",
            Self::SwapOutput { .. } => "SwapOutput",
            Self::AddLiquidity { .. } => "AddLiquidity",
            Self::RemoveLiquidity { .. } => "RemoveLiquidity",
            Self::FlashLoan { .. } => "FlashLoan",
            Self::AssertBalanceDelta { .. } => "AssertBalanceDelta",
            Self::AssertNetBalanceDelta { .. } => "AssertNetBalanceDelta",
            Self::AssertTransactionNetGain { .. } => "AssertTransactionNetGain",
            Self::Supply { .. } => "Supply",
            Self::Withdraw { .. } => "Withdraw",
            Self::DepositCollateral { .. } => "DepositCollateral",
            Self::WithdrawCollateral { .. } => "WithdrawCollateral",
            Self::SetCollateralUsage { .. } => "SetCollateralUsage",
            Self::Borrow { .. } => "Borrow",
            Self::Repay { .. } => "Repay",
            Self::Liquidate { .. } => "Liquidate",
            Self::PublishOracle { .. } => "PublishOracle",
            Self::PublishSpot { .. } => "PublishSpot",
            Self::PublishTwap { .. } => "PublishTwap",
            Self::AssertCondition { .. } => "AssertCondition",
            Self::AssertBalance { .. } => "AssertBalance",
            Self::Sequence { .. } => "Sequence",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hodl_policy: Option<crate::hodl::Policy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valuation_policy: Option<crate::valuation::Policy>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub baseline_policies: BTreeMap<String, crate::policy::agents::Policy>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hooks: BTreeMap<String, crate::hooks::Config>,
    pub schema_version: String,
    pub model_versions: Vec<String>,
    pub name: String,
    pub seed: Amount,
    pub assets: BTreeMap<String, Asset>,
    pub accounts: BTreeMap<String, Account>,
    pub protocols: BTreeMap<String, Pool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub books: BTreeMap<String, crate::book::Config>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub clmm_pools: BTreeMap<String, crate::clmm::Config>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub stable_pools: BTreeMap<String, crate::stable::StablePool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub flash_providers: BTreeMap<String, FlashProvider>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub policies: BTreeMap<String, crate::policy::ArbitragePolicy>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lp_policies: BTreeMap<String, crate::policy::baselines::LpPolicy>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mm_policies: BTreeMap<String, crate::policy::baselines::MmPolicy>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lending: BTreeMap<String, crate::credit::LendingConfig>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub feeds: BTreeMap<String, crate::credit::FeedConfig>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub price_sources: BTreeMap<String, crate::twap::SourceConfig>,
    pub agents: BTreeMap<String, Agent>,
    pub chain: Chain,
    pub blocks: Vec<Vec<Transaction>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler: Option<crate::scheduler::Config>,
}
impl Scenario {
    pub fn rounds(&self) -> usize {
        self.scheduler
            .as_ref()
            .map_or(self.blocks.len(), |c| c.rounds)
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_SCENARIO {
            return Err("scenario exceeds 64 MiB".into());
        }
        let s: Self = serde_json::from_slice(bytes).map_err(|e| format!("input: {e}"))?;
        s.validate()?;
        Ok(s)
    }
    pub fn pools(&self) -> impl Iterator<Item = (&String, &Pool)> {
        self.protocols
            .iter()
            .chain(self.stable_pools.iter().map(|(id, cfg)| (id, &cfg.pool)))
    }
    pub fn pool(&self, id: &str) -> Option<&Pool> {
        self.protocols
            .get(id)
            .or_else(|| self.stable_pools.get(id).map(|p| &p.pool))
    }
    pub fn protected(&self, account: &str) -> bool {
        self.hooks.values().any(|h| h.account == account)
            || self.pools().any(|(_, p)| p.account == account)
            || self.books.values().any(|p| p.account == account)
            || self.clmm_pools.values().any(|p| p.account == account)
            || self.flash_providers.values().any(|p| p.account == account)
            || self
                .lending
                .values()
                .any(|p| p.cash_account == account || p.positions.values().any(|v| v == account))
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA
            || MODELS
                .iter()
                .filter(|m| **m != "scheduler-m1-v1")
                .any(|m| !self.model_versions.iter().any(|v| v == m))
            || self.model_versions.iter().any(|m| {
                !MODELS.contains(&m.as_str())
                    && !M2_MODELS.contains(&m.as_str())
                    && !crate::credit::CREDIT_MODELS.contains(&m.as_str())
                    && m != crate::stable::MODEL
                    && m != crate::twap::MODEL
                    && m != crate::book::MODEL
                    && m != crate::clmm::MODEL
                    && m != crate::routing::heterogeneous::MODEL
                    && m != crate::policy::baselines::LP_MODEL
                    && m != crate::policy::baselines::MM_MODEL
                    && m != crate::policy::liquidator::MODEL
                    && m != crate::liquidation::MODEL
                    && m != crate::hooks::MODEL
                    && m != crate::policy::agents::MODEL
                    && m != crate::policy::flash_arbitrage::MODEL
                    && m != crate::valuation::MODEL
                    && m != crate::hodl::MODEL
                    && m != crate::scheduler::backrun::MODEL
                    && !crate::scheduler::MODELS.contains(&m.as_str())
                    && !crate::scheduler::bundles::MODELS.contains(&m.as_str())
            })
            || self.model_versions.iter().collect::<BTreeSet<_>>().len()
                != self.model_versions.len()
        {
            return Err("unsupported schema/model versions".into());
        }
        if self.assets.is_empty()
            || self.assets.keys().any(|id| id.is_empty())
            || self.agents.keys().any(|id| id.is_empty())
            || self.chain.id.is_empty()
        {
            return Err("invalid assets/decimals".into());
        }
        if self.chain.interval.is_zero()
            || self.chain.block_limit < 100.into()
            || !self.assets.contains_key(&self.chain.fee_asset)
            || !self.accounts.contains_key(&self.chain.fee_account)
            || self.protected(&self.chain.fee_account)
        {
            return Err("invalid chain/fee account".into());
        }
        for (id, a) in &self.accounts {
            if id.is_empty() || a.balances.keys().any(|k| !self.assets.contains_key(k)) {
                return Err("unknown asset/account".into());
            }
        }
        // The declared initial supply is an Amount too. This bounds every future
        // balance including fee reserves/refunds and prevents settlement overflow.
        for asset in self.assets.keys() {
            self.accounts
                .values()
                .try_fold(Amount::ZERO, |sum, a| {
                    sum.checked_add(a.balances.get(asset).copied().unwrap_or_default())
                })
                .map_err(|_| format!("initial supply exceeds U256: {asset}"))?;
        }
        let mut protected = BTreeSet::new();
        for (id, p) in &self.protocols {
            if id.is_empty()
                || p.x == p.y
                || !self.assets.contains_key(&p.x)
                || !self.assets.contains_key(&p.y)
                || p.fee_bps >= 10000
                || !self.accounts.contains_key(&p.account)
                || !self.accounts.contains_key(&p.owner)
                || self.protected(&p.owner)
                || !protected.insert(&p.account)
            {
                return Err("invalid CPMM genesis".into());
            }
            for asset in [&p.x, &p.y] {
                if self.accounts[&p.account]
                    .balances
                    .get(asset)
                    .copied()
                    .unwrap_or_default()
                    .is_zero()
                {
                    return Err("empty genesis reserve".into());
                }
            }
        }
        crate::stable::validate(self, &mut protected)?;
        if !self.flash_providers.is_empty()
            && !self
                .model_versions
                .iter()
                .any(|v| v == "strict-flash-provider-v1")
        {
            return Err("missing flash model version".into());
        }
        for (id, p) in &self.flash_providers {
            if id.is_empty()
                || self.protocols.contains_key(id)
                || self.stable_pools.contains_key(id)
                || p.fee_bps >= 10000
                || !self.accounts.contains_key(&p.account)
                || !self.assets.contains_key(&p.asset)
                || !protected.insert(&p.account)
            {
                return Err("invalid flash provider".into());
            }
        }
        crate::credit::validate(self, &mut protected)?;
        crate::twap::validate(self)?;
        crate::book::validate(self, &mut protected)?;
        crate::clmm::validate(self, &mut protected)?;
        crate::hooks::validate(self, &mut protected)?;
        for a in self.agents.values() {
            for id in &a.controlled_accounts {
                if !self.accounts.contains_key(id) || self.protected(id) {
                    return Err("invalid controlled account".into());
                }
            }
        }
        for (agent, p) in &self.policies {
            if (p.route_limits.is_none()
                && !self.model_versions.iter().any(|v| v == "bounded-route-v1"))
                || !self
                    .agents
                    .get(agent)
                    .is_some_and(|a| a.controlled_accounts.contains(&p.account))
                || !self.assets.contains_key(&p.asset)
                || p.input.is_zero()
                || p.resource_limit < 100.into()
            {
                return Err("invalid arbitrage policy".into());
            }
            let budget = p.resource_limit.checked_mul(p.resource_price)?;
            p.input.checked_add(budget)?;
            p.min_profit.checked_add(budget)?;
        }
        crate::policy::baselines::validate(self)?;
        crate::scheduler::validate(self)?;
        crate::policy::agents::validate(self)?;
        crate::valuation::validate(self)?;
        crate::hodl::validate(self)?;
        let mut ids = BTreeSet::new();
        for block in &self.blocks {
            for tx in block {
                self.validate_tx(tx)?;
                if tx.id.starts_with("policy:") || !ids.insert(&tx.id) {
                    return Err("duplicate TxId".into());
                }
            }
        }
        Ok(())
    }
    pub fn validate_tx(&self, tx: &Transaction) -> Result<(), String> {
        if tx.id.is_empty()
            || tx.chain != self.chain.id
            || tx.limit < 100.into()
            || self.protected(&tx.sender)
            || !self
                .agents
                .get(&tx.agent)
                .is_some_and(|a| a.controlled_accounts.contains(&tx.sender))
        {
            return Err("invalid envelope or unauthorized sender".into());
        }
        tx.limit.checked_mul(tx.price)?;
        tx.nonce.checked_add(1.into())?;
        let mut nodes = 0;
        fn check(
            s: &Scenario,
            calls: &[Call],
            depth: usize,
            n: &mut usize,
            prefix: &str,
            outputs: &mut BTreeMap<String, String>,
        ) -> Result<(), String> {
            if depth > 32 || calls.len() > 256 {
                return Err("program depth/width limit".into());
            }
            for (i, call) in calls.iter().enumerate() {
                *n += 1;
                if *n > 1024 {
                    return Err("program node limit".into());
                }
                let path = if prefix.is_empty() {
                    i.to_string()
                } else {
                    format!("{prefix}.{i}")
                };
                let account = |id: &str| -> Result<(), String> {
                    if s.accounts.contains_key(id) {
                        Ok(())
                    } else {
                        Err(format!("unknown account: {id}"))
                    }
                };
                let asset = |id: &str| -> Result<(), String> {
                    if s.assets.contains_key(id) {
                        Ok(())
                    } else {
                        Err(format!("unknown asset: {id}"))
                    }
                };
                match call {
                    Call::MintPosition { pool, .. }
                    | Call::BurnPosition { pool, .. }
                    | Call::CollectPosition { pool, .. }
                    | Call::ClmmSwap { pool, .. } => {
                        let cfg = s.clmm_pools.get(pool).ok_or("unknown CLMM pool")?;
                        if let Call::ClmmSwap { asset_in, .. } = call
                            && asset_in != &cfg.x
                            && asset_in != &cfg.q
                        {
                            return Err("CLMM asset not in pool".into());
                        }
                    }
                    Call::PlaceOrder { book, .. } | Call::CancelOrder { book, .. } => {
                        if !s.books.contains_key(book) {
                            return Err("unknown order book".into());
                        }
                    }
                    Call::AssertNetBalanceDelta { asset: a, .. }
                    | Call::AssertTransactionNetGain { asset: a, .. } => {
                        // The refund projection is exact only as the final root call.
                        if depth != 1 || i + 1 != calls.len() || a != &s.chain.fee_asset {
                            return Err(
                                "net balance assertion must be final root call in fee asset".into(),
                            );
                        }
                    }
                    Call::Supply { market, .. }
                    | Call::Withdraw { market, .. }
                    | Call::DepositCollateral { market, .. }
                    | Call::WithdrawCollateral { market, .. }
                    | Call::SetCollateralUsage { market, .. }
                    | Call::Borrow { market, .. }
                    | Call::Repay { market, .. }
                    | Call::Liquidate { market, .. } => {
                        if !s.lending.contains_key(market) {
                            return Err("unknown lending market".into());
                        }
                        if let Call::Repay { borrower, .. } | Call::Liquidate { borrower, .. } =
                            call
                            && !s.lending[market].positions.contains_key(borrower)
                        {
                            return Err("unknown borrower position".into());
                        }
                    }
                    Call::PublishOracle { feed, .. }
                    | Call::PublishSpot { feed, .. }
                    | Call::PublishTwap { feed, .. } => {
                        if !s.feeds.contains_key(feed) {
                            return Err("unknown oracle feed".into());
                        }
                    }
                    Call::Transfer { to, asset: a, .. } => {
                        account(to)?;
                        asset(a)?;
                    }
                    Call::TransferFrom {
                        from, to, asset: a, ..
                    } => {
                        account(from)?;
                        account(to)?;
                        asset(a)?;
                    }
                    Call::Approve {
                        spender, asset: a, ..
                    } => {
                        account(spender)?;
                        asset(a)?;
                    }
                    Call::AssertBalance {
                        account: a,
                        asset: b,
                        ..
                    }
                    | Call::AssertBalanceDelta {
                        account: a,
                        asset: b,
                        ..
                    } => {
                        account(a)?;
                        asset(b)?;
                    }
                    Call::SwapExactIn { pool, asset_in, .. }
                    | Call::SwapOutput { pool, asset_in, .. } => {
                        let p = s.pool(pool).ok_or("unknown pool reference")?;
                        if asset_in != &p.x && asset_in != &p.y {
                            return Err("swap asset not in venue".into());
                        }
                    }
                    Call::AddLiquidity { pool, .. } | Call::RemoveLiquidity { pool, .. } => {
                        if s.pool(pool).is_none() {
                            return Err("unknown LP pool".into());
                        }
                    }
                    Call::FlashLoan {
                        provider, receiver, ..
                    } => {
                        if !s.flash_providers.contains_key(provider) {
                            return Err("unknown flash provider".into());
                        }
                        account(receiver)?;
                    }
                    _ => {}
                }
                match call {
                    Call::SwapExactIn { pool, .. } | Call::SwapOutput { pool, .. } => {
                        crate::hooks::count_children(s, pool, true, depth, n)?
                    }
                    Call::AddLiquidity { pool, .. } | Call::RemoveLiquidity { pool, .. } => {
                        crate::hooks::count_children(s, pool, false, depth, n)?
                    }
                    Call::ExecuteRoute { hops, .. } | Call::LiquidateAndSell { hops, .. } => {
                        let nested = if matches!(call, Call::LiquidateAndSell { .. }) {
                            2
                        } else {
                            1
                        };
                        for hop in hops {
                            if let crate::routing::heterogeneous::VenueHop::Amm { venue } = hop {
                                crate::hooks::count_children(s, venue, true, depth + nested, n)?;
                            }
                        }
                    }
                    _ => {}
                }
                match call {
                    Call::Sequence { calls } => check(s, calls, depth + 1, n, &path, outputs)?,
                    Call::LiquidateAndSell {
                        market,
                        borrower,
                        hops,
                        ..
                    } => {
                        if !s
                            .model_versions
                            .iter()
                            .any(|m| m == crate::liquidation::MODEL)
                        {
                            return Err("missing liquidation sale model".into());
                        }
                        let cfg = s.lending.get(market).ok_or("unknown lending market")?;
                        if !cfg.positions.contains_key(borrower) {
                            return Err("unknown borrower position".into());
                        }
                        let output = crate::routing::heterogeneous::validate_hops(
                            s,
                            &cfg.collateral_asset,
                            hops,
                        )?;
                        if output != cfg.debt_asset {
                            return Err("liquidation sale must end in debt asset".into());
                        }
                        *n += 2 + hops.len();
                        if *n > 1024 || depth + 2 > 32 {
                            return Err("liquidation sale child node/depth limit".into());
                        }
                    }
                    Call::ExecuteRoute { asset_in, hops, .. } => {
                        let output =
                            crate::routing::heterogeneous::validate_hops(s, asset_in, hops)?;
                        *n += hops.len();
                        if *n > 1024 || depth + 1 > 32 {
                            return Err("route child node/depth limit".into());
                        }
                        // Only the declared root result is referenceable, not generated native children.
                        outputs.insert(path, output);
                    }
                    Call::FlashLoan { callback, .. } => {
                        if !s
                            .model_versions
                            .iter()
                            .any(|v| v == "strict-flash-provider-v1")
                        {
                            return Err("missing flash model version".into());
                        }
                        *n += 1;
                        if *n > 1024 || depth + 1 > 32 {
                            return Err("callback node/depth limit".into());
                        }
                        check(
                            s,
                            callback,
                            depth + 2,
                            n,
                            &format!("{path}.callback"),
                            outputs,
                        )?;
                    }
                    Call::AddLiquidity { pool, .. } | Call::RemoveLiquidity { pool, .. } => {
                        if s.protocols.contains_key(pool)
                            && !s.model_versions.iter().any(|v| v == "cpmm-lp-v1")
                        {
                            return Err("missing LP model version".into());
                        }
                    }
                    Call::SwapExactIn { pool, asset_in, .. }
                    | Call::SwapOutput { pool, asset_in, .. } => {
                        if let Call::SwapOutput { output_of, .. } = call {
                            if !s.model_versions.iter().any(|v| v == "bounded-route-v1") {
                                return Err("missing route model version".into());
                            }
                            if outputs.get(output_of) != Some(asset_in) {
                                return Err("unavailable or incompatible output reference".into());
                            }
                        }
                        if let Some(pool) = s.pool(pool) {
                            let asset = if asset_in == &pool.x {
                                Some(&pool.y)
                            } else if asset_in == &pool.y {
                                Some(&pool.x)
                            } else {
                                None
                            };
                            if let Some(asset) = asset {
                                outputs.insert(path, asset.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(())
        }
        check(self, &tx.program, 1, &mut nodes, "", &mut BTreeMap::new())?;
        if serde_json::to_vec(tx).map_err(|e| e.to_string())?.len() > MAX_INPUT {
            return Err("transaction exceeds 1 MiB".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct World {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub hooks: BTreeMap<String, crate::hooks::State>,
    pub accounts: Arc<BTreeMap<String, Account>>,
    pub allowances: Arc<Allowances>,
    pub shares: BTreeMap<String, Amount>,
    pub lp_balances: Arc<BTreeMap<String, BTreeMap<String, Amount>>>,
    pub block: Amount,
    pub timestamp: Amount,
    pub lending: Arc<BTreeMap<String, crate::credit::LendingState>>,
    pub feeds: Arc<BTreeMap<String, crate::credit::FeedState>>,
    #[serde(default)]
    pub price_history: Arc<BTreeMap<String, crate::twap::History>>,
    #[serde(default)]
    pub books: Arc<BTreeMap<String, crate::book::State>>,
    #[serde(default)]
    pub clmm: Arc<BTreeMap<String, crate::clmm::State>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub kind: String,
    pub from: String,
    pub to: String,
    pub asset: String,
    pub amount: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook: Option<crate::hooks::Trace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub liquidation: Option<crate::liquidation::SaleResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<crate::routing::heterogeneous::RouteResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clmm: Option<crate::clmm::Trace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book: Option<crate::book::OrderTrace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub twap: Option<crate::twap::QueryTrace>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_updates: Vec<crate::twap::SourceUpdate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub solver: Vec<crate::stable::SolverStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle: Option<crate::credit::OracleUse>,
    pub path: String,
    pub origin: String,
    pub caller: String,
    pub callee: String,
    pub operation: String,
    pub outcome: String,
    pub used: Amount,
    pub events: Vec<Event>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Charge {
    pub phase: String,
    pub requested: Amount,
    pub charged: Amount,
    pub total: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub liquidations: BTreeMap<String, crate::liquidation::SaleResult>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub route_results: BTreeMap<String, crate::routing::heterogeneous::RouteResult>,
    pub tx_id: String,
    pub sender: String,
    pub status: String,
    pub error: Option<String>,
    pub used: Amount,
    pub reserved: Amount,
    pub fee: Amount,
    pub refund: Amount,
    pub events: Vec<Event>,
    pub trace: Vec<Trace>,
    pub charges: Vec<Charge>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Rejected {
    pub tx_id: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Block {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler: Option<crate::scheduler::Report>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub system_events: Vec<Event>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decisions: Vec<crate::policy::Decision>,
    pub number: Amount,
    pub timestamp: Amount,
    pub receipts: Vec<Receipt>,
    pub rejected: Vec<Rejected>,
    pub used: Amount,
    pub state: World,
}
