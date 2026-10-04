//! Application pricing and chain exceptions, separate from Raven network metadata.
use crate::ExampleError;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Token {
    pub symbol: String,
    pub name: String,
    pub decimals: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChainConfig {
    pub reference_token: String,
    pub stable_token_pool: String,
    pub minimum_native_locked: String,
    pub whitelist_tokens: Vec<String>,
    pub stable_coins: Vec<String>,
    #[serde(default)]
    pub skip_pools: Vec<String>,
    #[serde(default)]
    pub tokens: BTreeMap<String, Token>,
}
impl ChainConfig {
    /// Loads and normalizes the pricing policy from a file or bundled defaults.
    pub fn load(path: Option<&Path>) -> Result<Self, ExampleError> {
        let mut config: Self = match path {
            Some(path) => serde_json::from_slice(
                &std::fs::read(path).map_err(|_| ExampleError::InvalidChainConfig)?,
            ),
            None => serde_json::from_str(include_str!("../config/ethereum.json")),
        }
        .map_err(|_| ExampleError::InvalidChainConfig)?;
        for address in std::iter::once(&mut config.reference_token)
            .chain(std::iter::once(&mut config.stable_token_pool))
            .chain(config.whitelist_tokens.iter_mut())
            .chain(config.stable_coins.iter_mut())
            .chain(config.skip_pools.iter_mut())
        {
            *address = address.to_lowercase();
            address
                .parse::<alloy_primitives::Address>()
                .map_err(|_| ExampleError::InvalidChainConfig)?;
        }
        config.tokens = config
            .tokens
            .into_iter()
            .map(|(key, value)| (key.to_lowercase(), value))
            .collect();
        config
            .minimum_native_locked
            .parse::<bigdecimal::BigDecimal>()
            .map_err(|_| ExampleError::InvalidChainConfig)?;
        Ok(config)
    }
}
