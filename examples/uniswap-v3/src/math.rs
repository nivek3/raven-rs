//! Decimal arithmetic that rounds each operation to 34 significant digits.
use std::str::FromStr;

use alloy_primitives::U160;
use bigdecimal::BigDecimal;
use uniswap_v3_sdk::{
    prelude::sdk_core::prelude::{Fraction, FractionBase, Rounding, ToBig},
    utils::Q192,
};

use crate::ExampleError;

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

/// Converts the SDK's exact Q64.96 price ratio into the mapping's decimal units.
pub(crate) fn sqrt_prices(
    sqrt: U160,
    decimals0: &BigDecimal,
    decimals1: &BigDecimal,
) -> Result<(BigDecimal, BigDecimal), ExampleError> {
    let ratio = Fraction::new(sqrt.to_big_int().pow(2), Q192.to_big_int());
    let inverse = if sqrt == U160::ZERO {
        Fraction::ZERO
    } else {
        ratio.invert()
    };
    let to_decimal = |value: Fraction| {
        value
            .to_significant(34, Some(Rounding::RoundHalfUp))
            .map(|value| integer(&value))
            .map_err(|error| ExampleError::UniswapSdk(error.into()))
    };
    let scale = integer(&format!("1e{}", decimals0 - decimals1));
    Ok((
        round34(to_decimal(ratio)? * scale.clone()),
        round34(to_decimal(inverse)? / scale),
    ))
}

/// Converts a raw token amount into decimal token units.
pub(crate) fn units(raw: &str, decimals: &str) -> BigDecimal {
    let denominator = BigDecimal::from_str(&format!("1e{decimals}")).expect("token decimals");
    round34(decimal(raw) / denominator)
}
