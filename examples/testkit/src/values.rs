//! ABI values and exact numeric reference calculations for local verification.

use std::str::FromStr;

use bigdecimal::{BigDecimal, num_bigint::BigInt};
use num_traits::{Pow, Signed, Zero};
use serde_json::{Number, Value};

use crate::{Result, require};

/// Extracts a JSON string value.
pub(crate) fn text(value: &Value) -> Result<String> {
    Ok(value.as_str().ok_or("expected a JSON string")?.to_owned())
}

/// Decodes cast's ABI output, preserving exact numbers as decimal text.
pub(crate) fn call_values(values: Value) -> Result<Value> {
    fn normalize(value: &mut Value) {
        match value {
            Value::Number(number) => *value = Value::String(number.to_string()),
            Value::Array(values) => values.iter_mut().for_each(normalize),
            _ => {}
        }
    }

    let Value::Array(mut values) = values else {
        return Err("cast returned invalid ABI values".into());
    };
    require(!values.is_empty(), "cast returned no ABI values")?;
    values.iter_mut().for_each(normalize);
    Ok(if values.len() == 1 {
        values.remove(0)
    } else {
        Value::Array(values)
    })
}

/// Parses an RPC hexadecimal quantity.
pub(crate) fn hex_u64(value: &str) -> Result<u64> {
    Ok(u64::from_str_radix(value.trim_start_matches("0x"), 16)?)
}

/// Parses a hexadecimal ABI integer or indexed event topic.
pub(crate) fn hex_bigint(value: &str) -> Result<BigInt> {
    BigInt::parse_bytes(value.trim_start_matches("0x").as_bytes(), 16)
        .ok_or_else(|| "invalid ABI hexadecimal integer".into())
}

/// Decodes ABI event data into 32-byte integer words.
pub(crate) fn log_words(data: &str) -> Result<Vec<BigInt>> {
    let data = data.trim_start_matches("0x");
    require(data.len() % 64 == 0, "event data is not ABI word aligned")?;
    data.as_bytes()
        .chunks(64)
        .map(|word| BigInt::parse_bytes(word, 16).ok_or_else(|| "invalid ABI event word".into()))
        .collect()
}

/// Returns an exact power of ten.
fn power10(exponent: u32) -> BigInt {
    BigInt::from(10).pow(exponent)
}

/// Rounds an exact rational to 34 significant digits, with ties away from zero.
/// Integer quotient and remainder avoid floating-point division.
fn divide34(numerator: &BigDecimal, denominator: &BigDecimal) -> Result<BigDecimal> {
    let (numerator, numerator_scale) = numerator.as_bigint_and_exponent();
    let (denominator, denominator_scale) = denominator.as_bigint_and_exponent();
    require(!denominator.is_zero(), "decimal reference division by zero")?;
    if numerator.is_zero() {
        return Ok(BigDecimal::from(0));
    }
    let negative = numerator.is_negative() != denominator.is_negative();
    let mut numerator = numerator.abs();
    let mut denominator = denominator.abs();
    let mut magnitude =
        numerator.to_str_radix(10).len() as i64 - denominator.to_str_radix(10).len() as i64;
    let below = if magnitude >= 0 {
        numerator < &denominator * power10(magnitude as u32)
    } else {
        &numerator * power10((-magnitude) as u32) < denominator
    };
    if below {
        magnitude -= 1;
    }
    let shift = 33 - magnitude;
    if shift >= 0 {
        numerator *= power10(shift as u32);
    } else {
        denominator *= power10((-shift) as u32);
    }
    let mut coefficient = &numerator / &denominator;
    if (&numerator % &denominator) * BigInt::from(2) >= denominator {
        coefficient += BigInt::from(1);
    }
    if negative {
        coefficient = -coefficient;
    }
    Ok(BigDecimal::new(
        coefficient,
        shift + numerator_scale - denominator_scale,
    ))
}

/// Rounds a decimal to the testkit's 34-significant-digit rule.
fn round34(value: BigDecimal) -> Result<BigDecimal> {
    divide34(&value, &BigDecimal::from(1))
}

/// Computes reciprocal token prices from a Q96 square-root price.
pub(crate) fn decimal34_price(
    sqrt_price_x96: &BigInt,
    decimals0: u32,
    decimals1: u32,
) -> Result<(BigDecimal, BigDecimal)> {
    // Keep intermediate operations at the same 34-significant-digit precision.
    let square = round34(BigDecimal::from(sqrt_price_x96 * sqrt_price_x96))?;
    let denominator = round34(BigDecimal::from(BigInt::from(1) << 192usize))?;
    let ratio = divide34(&square, &denominator)?;
    let ratio = round34(ratio * BigDecimal::from(power10(decimals0)))?;
    let ratio = divide34(&ratio, &BigDecimal::from(power10(decimals1)))?;
    Ok((divide34(&BigDecimal::from(1), &ratio)?, ratio))
}

/// SQL numeric JSON may preserve a different scale for the same exact value.
pub(crate) fn normalize_json_numbers(value: &mut Value) {
    match value {
        Value::Number(number) => {
            if let Ok(decimal) = BigDecimal::from_str(&number.to_string())
                && let Ok(normalized) = Number::from_str(&decimal.normalized().to_string())
            {
                *number = normalized;
            }
        }
        Value::Array(values) => values.iter_mut().for_each(normalize_json_numbers),
        Value::Object(values) => values.values_mut().for_each(normalize_json_numbers),
        _ => {}
    }
}

/// ERC20 views expose `value` as decimal text; other strings remain identifiers.
pub(crate) fn normalize_decimal_fields(value: &mut Value) {
    match value {
        Value::Number(_) => normalize_json_numbers(value),
        Value::Object(values) => {
            if let Some(Value::String(value)) = values.get_mut("value") {
                if let Ok(decimal) = BigDecimal::from_str(value) {
                    *value = decimal.normalized().to_string();
                }
            }
            values.values_mut().for_each(normalize_decimal_fields);
        }
        Value::Array(values) => values.iter_mut().for_each(normalize_decimal_fields),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cast_call_numbers_are_text_and_tuples_preserve_booleans() {
        assert_eq!(
            call_values(serde_json::from_str("[18]").unwrap()).unwrap(),
            serde_json::json!("18")
        );
        let values: Value =
            serde_json::from_str(r#"["340282366920938463463374607431768211455",-120,18,true]"#)
                .unwrap();
        assert_eq!(
            call_values(values).unwrap(),
            serde_json::json!([
                "340282366920938463463374607431768211455",
                "-120",
                "18",
                true
            ])
        );
        assert!(call_values(serde_json::json!([])).is_err());
    }

    #[test]
    /// Verifies 34-digit rounding and uint256 parsing remain exact.
    fn reference_rounds_half_up_and_keeps_uint256_exact() {
        let value = BigDecimal::from_str("12345678901234567890123456789012345").unwrap();
        assert_eq!(
            round34(value).unwrap(),
            BigDecimal::from_str("12345678901234567890123456789012350").unwrap()
        );
        assert_eq!(
            hex_bigint("0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff")
                .unwrap(),
            (BigInt::from(1) << 256usize) - BigInt::from(1)
        );
    }

    #[test]
    /// Verifies equivalent SQL decimal scales normalize to equal JSON.
    fn equivalent_sql_scales_compare_without_floating_point() {
        let mut left: Value = serde_json::from_str(
            r#"{"amount":123456789012345678901234567890.000,"value":"1.00"}"#,
        )
        .unwrap();
        let mut right: Value =
            serde_json::from_str(r#"{"amount":1.2345678901234567890123456789e29,"value":"1"}"#)
                .unwrap();
        normalize_decimal_fields(&mut left);
        normalize_decimal_fields(&mut right);
        assert_eq!(left, right);
    }

    #[test]
    /// Verifies Q96 prices account for the tokens' decimal scales.
    fn sqrt_reference_respects_token_decimal_scaling() {
        let sqrt = BigInt::from(1) << 96usize;
        let (price0, price1) = decimal34_price(&sqrt, 18, 6).unwrap();
        assert_eq!(price0, BigDecimal::from_str("1e-12").unwrap());
        assert_eq!(price1, BigDecimal::from_str("1e12").unwrap());
    }
}
