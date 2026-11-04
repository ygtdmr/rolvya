//! Checked forecasts are deliberately separate from externally enforced limits.
use super::*;
use crate::controller_keys::worker_auth::aws_fargate::{
    FargateWorkload,
    execution::{ExposureEnvelope, UnitRate},
};
use rolvya_domain::money::{CostBreakdown, Money};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenCharge {
    pub tokens: u64,
    pub rate: UnitRate,
}
/// Billing dimensions remain provider specific. Requested limits are estimates,
/// not independently enforced dollar caps. No numeric rate is built into code.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApiExposure {
    OpenAi {
        model: String,
        uncached_input: TokenCharge,
        cached_input: TokenCharge,
        output_including_reasoning: TokenCharge,
    },
    Anthropic {
        model: String,
        input: TokenCharge,
        cache_read: TokenCharge,
        cache_write_5m: TokenCharge,
        cache_write_1h: TokenCharge,
        output_including_thinking: TokenCharge,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExposureCalculation {
    pub version: u16,
    pub coding: ExposureEnvelope,
    pub verifier: ExposureEnvelope,
    pub api: ApiExposure,
    pub calls: u16,
    /// Includes explicitly reviewed tools/fees/tax where applicable; None is unknown.
    pub additional_ai: Option<Money>,
    pub price_source_sha256: String,
    pub region: String,
    pub observed_at: i64,
    pub expires_at: i64,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimatedExposure {
    pub estimated: CostBreakdown,
    pub commitment: String,
    pub independently_enforced: bool,
}
impl ExposureCalculation {
    pub fn estimate(
        &self,
        coding: &FargateWorkload,
        verifier: &FargateWorkload,
        now: i64,
    ) -> Result<EstimatedExposure, E> {
        if self.version != 1
            || !(1..=8).contains(&self.calls)
            || self.observed_at < 0
            || now < self.observed_at
            || now >= self.expires_at
            || self
                .expires_at
                .checked_sub(self.observed_at)
                .is_none_or(|v| v > 86400)
            || !hash(&self.price_source_sha256)
            || self.region != coding.cluster.region()
            || self.region != verifier.cluster.region()
        {
            return Err(E::Unbounded);
        }
        let a = self
            .coding
            .upper_bound(coding, now)
            .map_err(|_| E::Unbounded)?;
        let b = self
            .verifier
            .upper_bound(verifier, now)
            .map_err(|_| E::Unbounded)?;
        let (model, charges) = match &self.api {
            ApiExposure::OpenAi {
                model,
                uncached_input,
                cached_input,
                output_including_reasoning,
            } => (
                model,
                vec![uncached_input, cached_input, output_including_reasoning],
            ),
            ApiExposure::Anthropic {
                model,
                input,
                cache_read,
                cache_write_5m,
                cache_write_1h,
                output_including_thinking,
            } => (
                model,
                vec![
                    input,
                    cache_read,
                    cache_write_5m,
                    cache_write_1h,
                    output_including_thinking,
                ],
            ),
        };
        if !atom(model) {
            return Err(E::InvalidInput);
        }
        let mut ai = self.additional_ai.ok_or(E::Unbounded)?;
        if ai.currency() != a.ai_api.currency() {
            return Err(E::Unbounded);
        }
        for c in charges {
            if c.rate.units == 0 || c.rate.charge.currency() != ai.currency() {
                return Err(E::Unbounded);
            }
            let q = u128::from(c.tokens)
                .checked_mul(u128::from(self.calls))
                .ok_or(E::Unbounded)?;
            let n = q
                .checked_mul(c.rate.charge.minor_units() as u128)
                .and_then(|n| n.checked_add(u128::from(c.rate.units) - 1))
                .ok_or(E::Unbounded)?
                / u128::from(c.rate.units);
            ai = ai
                .checked_add(
                    Money::new(i64::try_from(n).map_err(|_| E::Unbounded)?, ai.currency())
                        .map_err(|_| E::Unbounded)?,
                )
                .map_err(|_| E::Unbounded)?;
        }
        let add = |x: Money, y: Money| x.checked_add(y).map_err(|_| E::Unbounded);
        let estimated = CostBreakdown {
            ai_api: add(add(a.ai_api, b.ai_api)?, ai)?,
            compute: add(a.compute, b.compute)?,
            storage: add(a.storage, b.storage)?,
            transfer: add(a.transfer, b.transfer)?,
        };
        estimated.total().map_err(|_| E::Unbounded)?;
        Ok(EstimatedExposure {
            estimated,
            commitment: digest(
                "rolvya/exposure-calculation/v1",
                &(self, coding, verifier, estimated),
            )?,
            independently_enforced: false,
        })
    }
}
/// Category-wise coverage prevents unused compute from masking uncovered AI.
/// This checks accounting only; it does not establish external enforcement.
pub fn require_coverage(proposed: CostBreakdown, reserved: CostBreakdown) -> Result<(), E> {
    proposed.total().map_err(|_| E::Unbounded)?;
    reserved.total().map_err(|_| E::Unbounded)?;
    for (a, b) in [
        (proposed.ai_api, reserved.ai_api),
        (proposed.compute, reserved.compute),
        (proposed.storage, reserved.storage),
        (proposed.transfer, reserved.transfer),
    ] {
        if a.currency() != b.currency() || a.minor_units() > b.minor_units() {
            return Err(E::Unbounded);
        }
    }
    Ok(())
}
