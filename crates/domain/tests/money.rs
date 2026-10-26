use rolvya_domain::money::{Correction, CostBreakdown, CostScenarios, Currency, Money, MoneyError};

fn usd() -> Currency {
    Currency::new("USD").unwrap()
}

fn amount(value: i64) -> Money {
    Money::new(value, usd()).unwrap()
}

fn categories(values: [i64; 4]) -> CostBreakdown {
    CostBreakdown {
        ai_api: amount(values[0]),
        compute: amount(values[1]),
        storage: amount(values[2]),
        transfer: amount(values[3]),
    }
}

#[test]
fn negative_exposure_is_rejected() {
    assert_eq!(Money::new(-1, usd()), Err(MoneyError::NegativeExposure));
    assert_eq!(
        Money::new(i64::MIN, usd()),
        Err(MoneyError::NegativeExposure)
    );
}

#[test]
fn zero_and_maximum_minor_units_are_exact() {
    assert_eq!(amount(0).minor_units(), 0);
    assert_eq!(amount(i64::MAX).minor_units(), i64::MAX);
    assert_eq!(
        amount(i64::MAX).checked_add(amount(0)).unwrap(),
        amount(i64::MAX)
    );
}

#[test]
fn addition_subtraction_and_corrections_are_checked() {
    assert_eq!(
        amount(i64::MAX).checked_add(amount(1)),
        Err(MoneyError::Overflow)
    );
    assert_eq!(
        amount(1).checked_sub(amount(2)),
        Err(MoneyError::NegativeExposure)
    );
    assert_eq!(
        amount(5)
            .apply_correction(Correction::new(-3, usd()))
            .unwrap(),
        amount(2)
    );
    assert_eq!(
        amount(0).apply_correction(Correction::new(i64::MIN, usd())),
        Err(MoneyError::NegativeExposure)
    );
    assert_eq!(
        amount(i64::MAX).apply_correction(Correction::new(1, usd())),
        Err(MoneyError::Overflow)
    );
    assert_eq!(
        amount(0)
            .apply_correction(Correction::new(i64::MAX, usd()))
            .unwrap(),
        amount(i64::MAX)
    );
}

#[test]
fn currency_mismatch_never_silently_combines_values() {
    let eur = Currency::new("EUR").unwrap();
    let other = Money::new(1, eur).unwrap();
    assert_eq!(
        amount(1).checked_add(other),
        Err(MoneyError::CurrencyMismatch)
    );
    assert_eq!(
        amount(1).checked_sub(other),
        Err(MoneyError::CurrencyMismatch)
    );
    assert_eq!(
        amount(1).apply_correction(Correction::new(1, eur)),
        Err(MoneyError::CurrencyMismatch)
    );
}

#[test]
fn currency_codes_have_one_unambiguous_representation() {
    for invalid in [
        "", "US", "USDD", "usd", "Usd", "US1", " USD", "USD\n", "€UR",
    ] {
        assert_eq!(Currency::new(invalid), Err(MoneyError::InvalidCurrency));
    }
    assert_eq!(usd().to_string(), "USD");
}

#[test]
fn deserialization_cannot_bypass_money_or_currency_validation() {
    assert!(serde_json::from_str::<Currency>("\"usd\"").is_err());
    assert!(serde_json::from_str::<Money>(r#"{"minor_units":-1,"currency":"USD"}"#).is_err());
    assert!(serde_json::from_str::<Money>(r#"{"minor_units":1.5,"currency":"USD"}"#).is_err());
    assert!(
        serde_json::from_str::<Money>(r#"{"minor_units":9223372036854775808,"currency":"USD"}"#)
            .is_err()
    );
    assert!(serde_json::from_str::<Money>(r#"{"minor_units":1,"currency":"usd"}"#).is_err());
    for value in [0, 1, i64::MAX] {
        let original = amount(value);
        let json = serde_json::to_string(&original).unwrap();
        assert_eq!(serde_json::from_str::<Money>(&json).unwrap(), original);
    }
    let correction = Correction::new(i64::MIN, usd());
    let json = serde_json::to_string(&correction).unwrap();
    assert_eq!(
        serde_json::from_str::<Correction>(&json).unwrap(),
        correction
    );
}

#[test]
fn category_identity_is_preserved_and_totals_are_checked() {
    let cost = categories([1, 2, 3, 4]);
    assert_eq!(cost.total().unwrap(), amount(10));
    assert_eq!(cost.ai_api, amount(1));
    assert_eq!(cost.compute, amount(2));
    assert_eq!(cost.storage, amount(3));
    assert_eq!(cost.transfer, amount(4));
    assert_eq!(
        categories([i64::MAX, 1, 0, 0]).total(),
        Err(MoneyError::Overflow)
    );
    let mut mixed = cost;
    mixed.storage = Money::new(0, Currency::new("EUR").unwrap()).unwrap();
    assert_eq!(mixed.validate(), Err(MoneyError::CurrencyMismatch));
    assert_eq!(mixed.total(), Err(MoneyError::CurrencyMismatch));
    let encoded = serde_json::to_value(cost).unwrap();
    assert_eq!(encoded["transfer"]["minor_units"], 4);
    assert_eq!(
        serde_json::from_value::<CostBreakdown>(encoded).unwrap(),
        cost
    );
}

#[test]
fn scenarios_remain_distinct_and_their_totals_are_ordered() {
    let scenarios = CostScenarios {
        optimistic: categories([1, 2, 3, 4]),
        expected: categories([2, 3, 4, 5]),
        pessimistic: categories([3, 4, 5, 6]),
    };
    scenarios.validate().unwrap();
    let json = serde_json::to_value(scenarios).unwrap();
    assert_eq!(json["optimistic"]["compute"]["minor_units"], 2);
    assert_eq!(json["expected"]["compute"]["minor_units"], 3);
    assert_eq!(json["pessimistic"]["compute"]["minor_units"], 4);
    assert_eq!(
        serde_json::from_value::<CostScenarios>(json).unwrap(),
        scenarios
    );
    let nonmonotone = CostScenarios {
        expected: categories([0, 100, 100, 100]),
        ..scenarios
    };
    assert_eq!(nonmonotone.validate(), Err(MoneyError::ScenarioOrder));
    let mut mixed = scenarios;
    mixed.pessimistic.transfer = Money::new(10, Currency::new("EUR").unwrap()).unwrap();
    assert_eq!(mixed.validate(), Err(MoneyError::CurrencyMismatch));
}

#[test]
fn deterministic_boundary_grid_preserves_checked_arithmetic() {
    let values = [0, 1, 2, 100, i64::MAX / 2, i64::MAX - 1, i64::MAX];
    for a in values {
        for b in values {
            let sum = amount(a).checked_add(amount(b));
            match a.checked_add(b) {
                Some(expected) => assert_eq!(sum.unwrap(), amount(expected)),
                None => assert_eq!(sum, Err(MoneyError::Overflow)),
            }
            let difference = amount(a).checked_sub(amount(b));
            if a >= b {
                assert_eq!(difference.unwrap(), amount(a - b));
            } else {
                assert_eq!(difference, Err(MoneyError::NegativeExposure));
            }
        }
    }
}

#[test]
fn cost_deserialization_rejects_incomplete_or_invalid_forecasts() {
    let complete = categories([1, 2, 3, 4]);
    let mut mixed = serde_json::to_value(complete).unwrap();
    mixed["storage"]["currency"] = "EUR".into();
    assert!(serde_json::from_value::<CostBreakdown>(mixed).is_err());

    let mut unknown = serde_json::to_value(complete).unwrap();
    unknown["untracked_category"] = serde_json::json!({"minor_units": 1, "currency": "USD"});
    assert!(serde_json::from_value::<CostBreakdown>(unknown).is_err());

    let mut incomplete = serde_json::to_value(complete).unwrap();
    incomplete.as_object_mut().unwrap().remove("transfer");
    assert!(serde_json::from_value::<CostBreakdown>(incomplete).is_err());

    let overflow = serde_json::to_value(categories([i64::MAX, 1, 0, 0])).unwrap();
    assert!(serde_json::from_value::<CostBreakdown>(overflow).is_err());

    let unordered = CostScenarios {
        optimistic: categories([1, 2, 3, 4]),
        expected: categories([0, 1, 1, 1]),
        pessimistic: categories([3, 4, 5, 6]),
    };
    assert!(
        serde_json::from_value::<CostScenarios>(serde_json::to_value(unordered).unwrap()).is_err()
    );
}

#[test]
fn category_tradeoffs_are_valid_when_scenario_totals_are_ordered() {
    let scenarios = CostScenarios {
        optimistic: categories([8, 0, 0, 0]),
        expected: categories([4, 8, 0, 0]),
        pessimistic: categories([3, 12, 0, 0]),
    };
    assert_eq!(scenarios.optimistic.total().unwrap(), amount(8));
    assert_eq!(scenarios.expected.total().unwrap(), amount(12));
    assert_eq!(scenarios.pessimistic.total().unwrap(), amount(15));
    scenarios.validate().unwrap();
    let json = serde_json::to_value(scenarios).unwrap();
    assert_eq!(
        serde_json::from_value::<CostScenarios>(json).unwrap(),
        scenarios
    );
}

#[test]
fn overflowing_scenario_total_never_becomes_a_valid_forecast() {
    let forecasts = CostScenarios {
        optimistic: categories([0, 0, 0, 0]),
        expected: categories([1, 2, 3, 4]),
        pessimistic: categories([i64::MAX, 1, 0, 0]),
    };
    assert_eq!(forecasts.validate(), Err(MoneyError::Overflow));
    assert!(
        serde_json::from_value::<CostScenarios>(serde_json::to_value(forecasts).unwrap()).is_err()
    );
}
