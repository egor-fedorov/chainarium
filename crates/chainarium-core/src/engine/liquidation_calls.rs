use super::*;
use crate::{liquidation::SaleResult, routing::heterogeneous::ResidualPolicy};

impl Context<'_> {
    pub(super) fn liquidation_sale(
        &mut self,
        w: &mut World,
        call: &Call,
        path: &str,
    ) -> Result<(), ExecutionError> {
        let Call::LiquidateAndSell {
            market,
            borrower,
            max_repay,
            min_collateral,
            hops,
            min_out,
        } = call
        else {
            return Err(Model("invalid liquidation sale dispatch".into()));
        };
        let cfg = self
            .s
            .lending
            .get(market)
            .ok_or("unknown lending market")?
            .clone();
        let sender = self.tx.sender.clone();
        let before_debt = w.balance(&sender, &cfg.debt_asset);
        let before_collateral = w.balance(&sender, &cfg.collateral_asset);
        let ti = self
            .trace
            .len()
            .checked_sub(1)
            .ok_or_else(|| Model("missing liquidation wrapper trace".into()))?;
        let child = self.trace.len();
        self.call(
            w,
            &Call::Liquidate {
                market: market.clone(),
                borrower: borrower.clone(),
                amount: *max_repay,
                min_collateral: *min_collateral,
            },
            format!("{path}.liquidate"),
        )?;
        let paid = before_debt
            .checked_sub(w.balance(&sender, &cfg.debt_asset))
            .map_err(Model)?;
        let seized = w
            .balance(&sender, &cfg.collateral_asset)
            .checked_sub(before_collateral)
            .map_err(Model)?;
        let mut result = SaleResult {
            market: market.clone(),
            borrower: borrower.clone(),
            requested: *max_repay,
            paid,
            seized,
            oracle: self.trace[child].oracle.clone(),
            route: None,
        };
        self.trace[ti].liquidation = Some(result.clone());
        let sale_index = self.trace.len();
        let sale_path = format!("{path}.sale");
        let outcome = self.call(
            w,
            &Call::ExecuteRoute {
                asset_in: cfg.collateral_asset,
                amount: seized,
                hops: hops.clone(),
                min_out: *min_out,
                residual_policy: ResidualPolicy::RequireFull,
            },
            sale_path.clone(),
        );
        result.route = self.trace[sale_index].route.clone();
        self.trace[ti].liquidation = Some(result.clone());
        outcome?;
        if result
            .route
            .as_ref()
            .is_none_or(|r| r.asset_out != cfg.debt_asset || !r.residuals.is_empty())
        {
            return Err(Model("liquidation sale result invariant".into()));
        }
        self.liquidations.insert(path.into(), result);
        Ok(())
    }
}
