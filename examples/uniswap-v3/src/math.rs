//! Decimal arithmetic that rounds each operation to 34 significant digits.
use std::{cmp::Ordering, str::FromStr};

use bigdecimal::BigDecimal;

/// Parses and rounds a decimal value to the mapping precision.
pub fn decimal(value: &str) -> BigDecimal {
    round34(integer(value))
}

/// Parses an exact integer value for subsequent decimal arithmetic.
pub fn integer(value: &str) -> BigDecimal {
    BigDecimal::from_str(value).expect("application integer")
}

/// Rounds values exceeding 34 significant digits.
pub fn round34(value: BigDecimal) -> BigDecimal {
    if value.digits() > 34 {
        value.with_prec(34)
    } else {
        value
    }
}

/// Raises a decimal base to an integer exponent with mapping precision.
pub fn power(base: &BigDecimal, exponent: i32) -> BigDecimal {
    match exponent.cmp(&0) {
        Ordering::Less => {
            let inverse = power(base, -exponent);
            if inverse == BigDecimal::from(0) {
                BigDecimal::from(0)
            } else {
                round34(BigDecimal::from(1) / inverse)
            }
        }
        Ordering::Equal => BigDecimal::from(1),
        Ordering::Greater if exponent == 1 => base.clone(),
        _ => {
            let half = power(base, exponent / 2);
            let value = round34(half.clone() * half);
            if exponent % 2 == 1 {
                round34(value * base.clone())
            } else {
                value
            }
        }
    }
}

/// Converts a raw token amount into decimal token units.
pub(crate) fn units(raw: &str, decimals: &str) -> BigDecimal {
    let denominator = BigDecimal::from_str(&format!("1e{decimals}")).expect("token decimals");
    round34(decimal(raw) / denominator)
}
