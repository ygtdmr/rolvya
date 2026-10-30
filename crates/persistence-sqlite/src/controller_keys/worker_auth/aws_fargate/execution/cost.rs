//! Checked conservative arithmetic, separate from independent enforcement proof.
use super::super::{AwsIdentityError as E, FargateWorkload};
use crate::finance::EvidenceRef;
use rolvya_domain::money::{CostBreakdown, Money};
use serde::{Deserialize, Serialize};
/// A reviewed price input per quantity. Neither a live price catalogue nor an
/// enforceable provider ceiling. All divisions round upward in minor units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitRate {
    pub charge: Money,
    pub units: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExposureEnvelope {
    pub format: u16,
    pub evidence: EvidenceRef,
    pub revision: u64,
    pub collected_at: i64,
    pub valid_until: i64,
    pub runtime_seconds: u32,
    pub provisioning_seconds: u32,
    pub termination_seconds: u32,
    pub cpu_unit_second: UnitRate,
    pub memory_mib_second: UnitRate,
    pub extra_gib_second: UnitRate,
    /// Fixed maxima include image pulls/service activity, bootstrap storage/API,
    /// logs/artifacts, NAT/VPC endpoints and transfer under separately verified caps.
    pub fixed: CostBreakdown,
}
impl ExposureEnvelope {
    pub fn upper_bound(&self, w: &FargateWorkload, now: i64) -> Result<CostBreakdown, E> {
        w.validate()?;
        if self.format != 1
            || self.revision == 0
            || self.collected_at < 0
            || now < self.collected_at
            || now >= self.valid_until
            || self.valid_until - self.collected_at > 86400
            || self.runtime_seconds != w.runtime_seconds
            || self.provisioning_seconds == 0
            || self.termination_seconds == 0
            || self.provisioning_seconds > 3600
            || self.termination_seconds > 3600
        {
            return Err(E::FinancialRisk);
        }
        self.fixed.validate().map_err(|_| E::FinancialRisk)?;
        let seconds = u64::from(self.runtime_seconds)
            .checked_add(u64::from(self.provisioning_seconds))
            .and_then(|v| v.checked_add(u64::from(self.termination_seconds)))
            .ok_or(E::FinancialRisk)?
            .max(60);
        let amount = |r: UnitRate, q: u64| -> Result<Money, E> {
            if r.units == 0 || r.charge.currency() != self.fixed.compute.currency() {
                return Err(E::FinancialRisk);
            }
            let n = (r.charge.minor_units() as u128)
                .checked_mul(q as u128)
                .ok_or(E::FinancialRisk)?;
            let rounded = n
                .checked_add(u128::from(r.units) - 1)
                .ok_or(E::FinancialRisk)?
                / u128::from(r.units);
            Money::new(
                i64::try_from(rounded).map_err(|_| E::FinancialRisk)?,
                r.charge.currency(),
            )
            .map_err(|_| E::FinancialRisk)
        };
        let cpu = amount(
            self.cpu_unit_second,
            seconds
                .checked_mul(u64::from(w.cpu_units))
                .ok_or(E::FinancialRisk)?,
        )?;
        let ram = amount(
            self.memory_mib_second,
            seconds
                .checked_mul(u64::from(w.memory_mib))
                .ok_or(E::FinancialRisk)?,
        )?;
        let storage = amount(
            self.extra_gib_second,
            seconds
                .checked_mul(u64::from(w.ephemeral_gib - 20))
                .ok_or(E::FinancialRisk)?,
        )?;
        let b = CostBreakdown {
            ai_api: self.fixed.ai_api,
            compute: self
                .fixed
                .compute
                .checked_add(cpu)
                .and_then(|v| v.checked_add(ram))
                .map_err(|_| E::FinancialRisk)?,
            storage: self
                .fixed
                .storage
                .checked_add(storage)
                .map_err(|_| E::FinancialRisk)?,
            transfer: self.fixed.transfer,
        };
        b.total().map_err(|_| E::FinancialRisk)?;
        Ok(b)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use rolvya_domain::money::Currency;
    fn m(n: i64) -> Money {
        Money::new(n, Currency::new("USD").unwrap()).unwrap()
    }
    fn envelope() -> ExposureEnvelope {
        ExposureEnvelope {
            format: 1,
            evidence: EvidenceRef::new("synthetic-price-and-enforcement-evidence").unwrap(),
            revision: 1,
            collected_at: 100,
            valid_until: 200,
            runtime_seconds: 10,
            provisioning_seconds: 3,
            termination_seconds: 5,
            cpu_unit_second: UnitRate {
                charge: m(1),
                units: 1024,
            },
            memory_mib_second: UnitRate {
                charge: m(1),
                units: 1024,
            },
            extra_gib_second: UnitRate {
                charge: m(1),
                units: 1,
            },
            fixed: CostBreakdown {
                ai_api: m(7),
                compute: m(4),
                storage: m(5),
                transfer: m(6),
            },
        }
    }
    #[test]
    fn linux_minimum_billing_and_all_four_categories_are_conservative() {
        let mut w = crate::controller_keys::worker_auth::aws_fargate::fixture_workload();
        w.runtime_seconds = 10;
        w.ephemeral_gib = 25;
        let b = envelope().upper_bound(&w, 100).unwrap();
        assert_eq!(
            (
                b.ai_api.minor_units(),
                b.compute.minor_units(),
                b.storage.minor_units(),
                b.transfer.minor_units()
            ),
            (7, 184, 305, 6)
        );
    }
    #[test]
    fn invalid_overflow_currency_and_expiry_fail_closed() {
        let mut w = crate::controller_keys::worker_auth::aws_fargate::fixture_workload();
        w.runtime_seconds = 10;
        for n in 0..6 {
            let mut e = envelope();
            match n {
                0 => e.cpu_unit_second.units = 0,
                1 => e.cpu_unit_second.charge = m(i64::MAX),
                2 => {
                    e.memory_mib_second.charge =
                        Money::new(1, Currency::new("EUR").unwrap()).unwrap()
                }
                3 => e.valid_until = 100,
                4 => e.runtime_seconds = 11,
                _ => e.termination_seconds = 0,
            };
            assert!(e.upper_bound(&w, 100).is_err(), "case {n}");
        }
        assert!(envelope().upper_bound(&w, 99).is_err());
        assert!(envelope().upper_bound(&w, 200).is_err());
    }
}
