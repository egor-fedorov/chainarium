use super::*;
use crate::routing::heterogeneous::{HopResult, ResidualPolicy, RouteResult, validate_hops};

impl Context<'_> {
    pub(super) fn route_call(
        &mut self,
        w: &mut World,
        call: &Call,
        path: &str,
    ) -> Result<(), ExecutionError> {
        let Call::ExecuteRoute {
            asset_in,
            amount,
            hops,
            min_out,
            residual_policy,
        } = call
        else {
            return Err(Model("invalid route dispatch".into()));
        };
        let asset_out = validate_hops(self.s, asset_in, hops)?;
        if amount.is_zero() || w.balance(&self.tx.sender, asset_in) < *amount {
            return Err(Protocol("insufficient route input budget".into()));
        }
        let ti = self
            .trace
            .len()
            .checked_sub(1)
            .ok_or_else(|| Model("missing route trace".into()))?;
        let mut result = RouteResult {
            asset_in: asset_in.clone(),
            asset_out,
            input_budget: *amount,
            input_spent: Amount::ZERO,
            input_unspent: *amount,
            output: Amount::ZERO,
            residual_policy: *residual_policy,
            residuals: BTreeMap::new(),
            hops: vec![],
        };
        self.trace[ti].route = Some(result.clone());
        let mut input = asset_in.clone();
        let mut budget = *amount;
        for (index, hop) in hops.iter().enumerate() {
            let output_asset = hop.other(self.s, &input)?;
            let before_in = w.balance(&self.tx.sender, &input);
            let before_out = w.balance(&self.tx.sender, &output_asset);
            let native = hop.native_call(self.s, &input, budget)?;
            let hop_path = format!("{path}.{index}");
            self.call(w, &native, hop_path.clone())?;
            // Native settlement includes book refunds and fee-to-self transfers.
            // Envelope reserve was already removed and its refund happens later.
            let (spent, output) =
                if matches!(hop, crate::routing::heterogeneous::VenueHop::Amm { .. }) {
                    // A hook payout is separate inventory, never swap output or a refund.
                    (
                        budget,
                        self.outputs
                            .get(&hop_path)
                            .ok_or_else(|| Model("missing AMM output".into()))?
                            .1,
                    )
                } else {
                    (
                        before_in
                            .checked_sub(w.balance(&self.tx.sender, &input))
                            .map_err(Model)?,
                        w.balance(&self.tx.sender, &output_asset)
                            .checked_sub(before_out)
                            .map_err(Model)?,
                    )
                };
            let unspent = budget.checked_sub(spent).map_err(Model)?;
            if index == 0 {
                result.input_spent = spent;
                result.input_unspent = unspent;
            }
            if !unspent.is_zero() {
                let entry = result.residuals.entry(input.clone()).or_default();
                *entry = entry.checked_add(unspent).map_err(Model)?;
            }
            result.hops.push(HopResult {
                hop: hop.clone(),
                asset_in: input,
                asset_out: output_asset.clone(),
                budget,
                spent,
                unspent,
                output,
            });
            if index + 1 == hops.len() {
                result.output = output;
            }
            self.trace[ti].route = Some(result.clone());
            if output.is_zero() {
                return Err(Protocol("route hop has no output".into()));
            }
            if *residual_policy == ResidualPolicy::RequireFull && !unspent.is_zero() {
                return Err(Protocol("route requires full input consumption".into()));
            }
            input = output_asset;
            budget = output;
        }
        if result.output < *min_out {
            return Err(Protocol("route min_out".into()));
        }
        self.outputs
            .insert(path.into(), (result.asset_out.clone(), result.output));
        self.route_results.insert(path.into(), result);
        Ok(())
    }
}
