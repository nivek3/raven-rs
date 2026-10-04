//! Application entities stored by the Uniswap V3 index.
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Factory {
    pub id: String,
    #[serde(rename = "poolCount")]
    pub pool_count: BigDecimal,
    #[serde(rename = "txCount")]
    pub tx_count: BigDecimal,
    #[serde(rename = "totalVolumeUSD")]
    pub total_volume_usd: BigDecimal,
    #[serde(rename = "totalVolumeETH")]
    pub total_volume_eth: BigDecimal,
    #[serde(rename = "totalFeesUSD")]
    pub total_fees_usd: BigDecimal,
    #[serde(rename = "totalFeesETH")]
    pub total_fees_eth: BigDecimal,
    #[serde(rename = "untrackedVolumeUSD")]
    pub untracked_volume_usd: BigDecimal,
    #[serde(rename = "totalValueLockedUSD")]
    pub total_value_locked_usd: BigDecimal,
    #[serde(rename = "totalValueLockedETH")]
    pub total_value_locked_eth: BigDecimal,
    #[serde(rename = "totalValueLockedUSDUntracked")]
    pub total_value_locked_usd_untracked: BigDecimal,
    #[serde(rename = "totalValueLockedETHUntracked")]
    pub total_value_locked_eth_untracked: BigDecimal,
    pub owner: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Bundle {
    pub id: String,
    #[serde(rename = "ethPriceUSD")]
    pub eth_price_usd: BigDecimal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Token {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub decimals: BigDecimal,
    #[serde(rename = "totalSupply")]
    pub total_supply: BigDecimal,
    pub volume: BigDecimal,
    #[serde(rename = "volumeUSD")]
    pub volume_usd: BigDecimal,
    #[serde(rename = "untrackedVolumeUSD")]
    pub untracked_volume_usd: BigDecimal,
    #[serde(rename = "feesUSD")]
    pub fees_usd: BigDecimal,
    #[serde(rename = "txCount")]
    pub tx_count: BigDecimal,
    #[serde(rename = "poolCount")]
    pub pool_count: BigDecimal,
    #[serde(rename = "totalValueLocked")]
    pub total_value_locked: BigDecimal,
    #[serde(rename = "totalValueLockedUSD")]
    pub total_value_locked_usd: BigDecimal,
    #[serde(rename = "totalValueLockedUSDUntracked")]
    pub total_value_locked_usd_untracked: BigDecimal,
    #[serde(rename = "derivedETH")]
    pub derived_eth: BigDecimal,
    #[serde(rename = "whitelistPools")]
    pub whitelist_pools: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Pool {
    pub id: String,
    #[serde(rename = "createdAtTimestamp")]
    pub created_at_timestamp: BigDecimal,
    #[serde(rename = "createdAtBlockNumber")]
    pub created_at_block_number: BigDecimal,
    pub token0: String,
    pub token1: String,
    #[serde(rename = "feeTier")]
    pub fee_tier: BigDecimal,
    pub liquidity: BigDecimal,
    #[serde(rename = "sqrtPrice")]
    pub sqrt_price: BigDecimal,
    #[serde(rename = "feeGrowthGlobal0X128")]
    pub fee_growth_global0_x128: BigDecimal,
    #[serde(rename = "feeGrowthGlobal1X128")]
    pub fee_growth_global1_x128: BigDecimal,
    #[serde(rename = "token0Price")]
    pub token0_price: BigDecimal,
    #[serde(rename = "token1Price")]
    pub token1_price: BigDecimal,
    #[serde(default)]
    pub tick: Option<BigDecimal>,
    #[serde(rename = "observationIndex")]
    pub observation_index: BigDecimal,
    #[serde(rename = "volumeToken0")]
    pub volume_token0: BigDecimal,
    #[serde(rename = "volumeToken1")]
    pub volume_token1: BigDecimal,
    #[serde(rename = "volumeUSD")]
    pub volume_usd: BigDecimal,
    #[serde(rename = "untrackedVolumeUSD")]
    pub untracked_volume_usd: BigDecimal,
    #[serde(rename = "feesUSD")]
    pub fees_usd: BigDecimal,
    #[serde(rename = "txCount")]
    pub tx_count: BigDecimal,
    #[serde(rename = "collectedFeesToken0")]
    pub collected_fees_token0: BigDecimal,
    #[serde(rename = "collectedFeesToken1")]
    pub collected_fees_token1: BigDecimal,
    #[serde(rename = "collectedFeesUSD")]
    pub collected_fees_usd: BigDecimal,
    #[serde(rename = "totalValueLockedToken0")]
    pub total_value_locked_token0: BigDecimal,
    #[serde(rename = "totalValueLockedToken1")]
    pub total_value_locked_token1: BigDecimal,
    #[serde(rename = "totalValueLockedETH")]
    pub total_value_locked_eth: BigDecimal,
    #[serde(rename = "totalValueLockedUSD")]
    pub total_value_locked_usd: BigDecimal,
    #[serde(rename = "totalValueLockedUSDUntracked")]
    pub total_value_locked_usd_untracked: BigDecimal,
    #[serde(rename = "liquidityProviderCount")]
    pub liquidity_provider_count: BigDecimal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Tick {
    pub id: String,
    #[serde(rename = "poolAddress")]
    pub pool_address: Option<String>,
    pub pool: String,
    #[serde(rename = "tickIdx")]
    pub tick_idx: BigDecimal,
    #[serde(rename = "liquidityGross")]
    pub liquidity_gross: BigDecimal,
    #[serde(rename = "liquidityNet")]
    pub liquidity_net: BigDecimal,
    pub price0: BigDecimal,
    pub price1: BigDecimal,
    #[serde(rename = "volumeToken0")]
    pub volume_token0: BigDecimal,
    #[serde(rename = "volumeToken1")]
    pub volume_token1: BigDecimal,
    #[serde(rename = "volumeUSD")]
    pub volume_usd: BigDecimal,
    #[serde(rename = "untrackedVolumeUSD")]
    pub untracked_volume_usd: BigDecimal,
    #[serde(rename = "feesUSD")]
    pub fees_usd: BigDecimal,
    #[serde(rename = "collectedFeesToken0")]
    pub collected_fees_token0: BigDecimal,
    #[serde(rename = "collectedFeesToken1")]
    pub collected_fees_token1: BigDecimal,
    #[serde(rename = "collectedFeesUSD")]
    pub collected_fees_usd: BigDecimal,
    #[serde(rename = "createdAtTimestamp")]
    pub created_at_timestamp: BigDecimal,
    #[serde(rename = "createdAtBlockNumber")]
    pub created_at_block_number: BigDecimal,
    #[serde(rename = "liquidityProviderCount")]
    pub liquidity_provider_count: BigDecimal,
    #[serde(rename = "feeGrowthOutside0X128")]
    pub fee_growth_outside0_x128: BigDecimal,
    #[serde(rename = "feeGrowthOutside1X128")]
    pub fee_growth_outside1_x128: BigDecimal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Position {
    pub id: String,
    pub owner: String,
    pub pool: String,
    pub token0: String,
    pub token1: String,
    #[serde(rename = "tickLower")]
    pub tick_lower: String,
    #[serde(rename = "tickUpper")]
    pub tick_upper: String,
    pub liquidity: BigDecimal,
    #[serde(rename = "depositedToken0")]
    pub deposited_token0: BigDecimal,
    #[serde(rename = "depositedToken1")]
    pub deposited_token1: BigDecimal,
    #[serde(rename = "withdrawnToken0")]
    pub withdrawn_token0: BigDecimal,
    #[serde(rename = "withdrawnToken1")]
    pub withdrawn_token1: BigDecimal,
    #[serde(rename = "collectedFeesToken0")]
    pub collected_fees_token0: BigDecimal,
    #[serde(rename = "collectedFeesToken1")]
    pub collected_fees_token1: BigDecimal,
    pub transaction: String,
    #[serde(rename = "feeGrowthInside0LastX128")]
    pub fee_growth_inside0_last_x128: BigDecimal,
    #[serde(rename = "feeGrowthInside1LastX128")]
    pub fee_growth_inside1_last_x128: BigDecimal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PositionSnapshot {
    pub id: String,
    pub owner: String,
    pub pool: String,
    pub position: String,
    #[serde(rename = "blockNumber")]
    pub block_number: BigDecimal,
    pub timestamp: BigDecimal,
    pub liquidity: BigDecimal,
    #[serde(rename = "depositedToken0")]
    pub deposited_token0: BigDecimal,
    #[serde(rename = "depositedToken1")]
    pub deposited_token1: BigDecimal,
    #[serde(rename = "withdrawnToken0")]
    pub withdrawn_token0: BigDecimal,
    #[serde(rename = "withdrawnToken1")]
    pub withdrawn_token1: BigDecimal,
    #[serde(rename = "collectedFeesToken0")]
    pub collected_fees_token0: BigDecimal,
    #[serde(rename = "collectedFeesToken1")]
    pub collected_fees_token1: BigDecimal,
    pub transaction: String,
    #[serde(rename = "feeGrowthInside0LastX128")]
    pub fee_growth_inside0_last_x128: BigDecimal,
    #[serde(rename = "feeGrowthInside1LastX128")]
    pub fee_growth_inside1_last_x128: BigDecimal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    #[serde(rename = "blockNumber")]
    pub block_number: BigDecimal,
    pub timestamp: BigDecimal,
    #[serde(rename = "gasUsed")]
    pub gas_used: BigDecimal,
    #[serde(rename = "gasPrice")]
    pub gas_price: BigDecimal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Mint {
    pub id: String,
    pub transaction: String,
    pub timestamp: BigDecimal,
    pub pool: String,
    pub token0: String,
    pub token1: String,
    pub owner: String,
    pub sender: Option<String>,
    pub origin: String,
    pub amount: BigDecimal,
    pub amount0: BigDecimal,
    pub amount1: BigDecimal,
    #[serde(rename = "amountUSD")]
    pub amount_usd: Option<BigDecimal>,
    #[serde(rename = "tickLower")]
    pub tick_lower: BigDecimal,
    #[serde(rename = "tickUpper")]
    pub tick_upper: BigDecimal,
    #[serde(rename = "logIndex", default)]
    pub log_index: Option<BigDecimal>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Burn {
    pub id: String,
    pub transaction: String,
    pub pool: String,
    pub token0: String,
    pub token1: String,
    pub timestamp: BigDecimal,
    pub owner: Option<String>,
    pub origin: String,
    pub amount: BigDecimal,
    pub amount0: BigDecimal,
    pub amount1: BigDecimal,
    #[serde(rename = "amountUSD")]
    pub amount_usd: Option<BigDecimal>,
    #[serde(rename = "tickLower")]
    pub tick_lower: BigDecimal,
    #[serde(rename = "tickUpper")]
    pub tick_upper: BigDecimal,
    #[serde(rename = "logIndex", default)]
    pub log_index: Option<BigDecimal>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Swap {
    pub id: String,
    pub transaction: String,
    pub timestamp: BigDecimal,
    pub pool: String,
    pub token0: String,
    pub token1: String,
    pub sender: String,
    pub recipient: String,
    pub origin: String,
    pub amount0: BigDecimal,
    pub amount1: BigDecimal,
    #[serde(rename = "amountUSD")]
    pub amount_usd: BigDecimal,
    #[serde(rename = "sqrtPriceX96")]
    pub sqrt_price_x96: BigDecimal,
    pub tick: BigDecimal,
    #[serde(rename = "logIndex", default)]
    pub log_index: Option<BigDecimal>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Collect {
    pub id: String,
    pub transaction: String,
    pub timestamp: BigDecimal,
    pub pool: String,
    pub owner: Option<String>,
    pub amount0: BigDecimal,
    pub amount1: BigDecimal,
    #[serde(rename = "amountUSD")]
    pub amount_usd: Option<BigDecimal>,
    #[serde(rename = "tickLower")]
    pub tick_lower: BigDecimal,
    #[serde(rename = "tickUpper")]
    pub tick_upper: BigDecimal,
    #[serde(rename = "logIndex", default)]
    pub log_index: Option<BigDecimal>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Flash {
    pub id: String,
    pub transaction: String,
    pub timestamp: BigDecimal,
    pub pool: String,
    pub sender: String,
    pub recipient: String,
    pub amount0: BigDecimal,
    pub amount1: BigDecimal,
    #[serde(rename = "amountUSD")]
    pub amount_usd: BigDecimal,
    #[serde(rename = "amount0Paid")]
    pub amount0_paid: BigDecimal,
    #[serde(rename = "amount1Paid")]
    pub amount1_paid: BigDecimal,
    #[serde(rename = "logIndex", default)]
    pub log_index: Option<BigDecimal>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UniswapDayData {
    pub id: String,
    pub date: i32,
    #[serde(rename = "volumeETH")]
    pub volume_eth: BigDecimal,
    #[serde(rename = "volumeUSD")]
    pub volume_usd: BigDecimal,
    #[serde(rename = "volumeUSDUntracked")]
    pub volume_usd_untracked: BigDecimal,
    #[serde(rename = "feesUSD")]
    pub fees_usd: BigDecimal,
    #[serde(rename = "txCount")]
    pub tx_count: BigDecimal,
    #[serde(rename = "tvlUSD")]
    pub tvl_usd: BigDecimal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PoolDayData {
    pub id: String,
    pub date: i32,
    pub pool: String,
    pub liquidity: BigDecimal,
    #[serde(rename = "sqrtPrice")]
    pub sqrt_price: BigDecimal,
    #[serde(rename = "token0Price")]
    pub token0_price: BigDecimal,
    #[serde(rename = "token1Price")]
    pub token1_price: BigDecimal,
    #[serde(default)]
    pub tick: Option<BigDecimal>,
    #[serde(rename = "feeGrowthGlobal0X128")]
    pub fee_growth_global0_x128: BigDecimal,
    #[serde(rename = "feeGrowthGlobal1X128")]
    pub fee_growth_global1_x128: BigDecimal,
    #[serde(rename = "tvlUSD")]
    pub tvl_usd: BigDecimal,
    #[serde(rename = "volumeToken0")]
    pub volume_token0: BigDecimal,
    #[serde(rename = "volumeToken1")]
    pub volume_token1: BigDecimal,
    #[serde(rename = "volumeUSD")]
    pub volume_usd: BigDecimal,
    #[serde(rename = "feesUSD")]
    pub fees_usd: BigDecimal,
    #[serde(rename = "txCount")]
    pub tx_count: BigDecimal,
    pub open: BigDecimal,
    pub high: BigDecimal,
    pub low: BigDecimal,
    pub close: BigDecimal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PoolHourData {
    pub id: String,
    #[serde(rename = "periodStartUnix")]
    pub period_start_unix: i32,
    pub pool: String,
    pub liquidity: BigDecimal,
    #[serde(rename = "sqrtPrice")]
    pub sqrt_price: BigDecimal,
    #[serde(rename = "token0Price")]
    pub token0_price: BigDecimal,
    #[serde(rename = "token1Price")]
    pub token1_price: BigDecimal,
    #[serde(default)]
    pub tick: Option<BigDecimal>,
    #[serde(rename = "feeGrowthGlobal0X128")]
    pub fee_growth_global0_x128: BigDecimal,
    #[serde(rename = "feeGrowthGlobal1X128")]
    pub fee_growth_global1_x128: BigDecimal,
    #[serde(rename = "tvlUSD")]
    pub tvl_usd: BigDecimal,
    #[serde(rename = "volumeToken0")]
    pub volume_token0: BigDecimal,
    #[serde(rename = "volumeToken1")]
    pub volume_token1: BigDecimal,
    #[serde(rename = "volumeUSD")]
    pub volume_usd: BigDecimal,
    #[serde(rename = "feesUSD")]
    pub fees_usd: BigDecimal,
    #[serde(rename = "txCount")]
    pub tx_count: BigDecimal,
    pub open: BigDecimal,
    pub high: BigDecimal,
    pub low: BigDecimal,
    pub close: BigDecimal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TickHourData {
    pub id: String,
    #[serde(rename = "periodStartUnix")]
    pub period_start_unix: i32,
    pub pool: String,
    pub tick: String,
    #[serde(rename = "liquidityGross")]
    pub liquidity_gross: BigDecimal,
    #[serde(rename = "liquidityNet")]
    pub liquidity_net: BigDecimal,
    #[serde(rename = "volumeToken0")]
    pub volume_token0: BigDecimal,
    #[serde(rename = "volumeToken1")]
    pub volume_token1: BigDecimal,
    #[serde(rename = "volumeUSD")]
    pub volume_usd: BigDecimal,
    #[serde(rename = "feesUSD")]
    pub fees_usd: BigDecimal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TickDayData {
    pub id: String,
    pub date: i32,
    pub pool: String,
    pub tick: String,
    #[serde(rename = "liquidityGross")]
    pub liquidity_gross: BigDecimal,
    #[serde(rename = "liquidityNet")]
    pub liquidity_net: BigDecimal,
    #[serde(rename = "volumeToken0")]
    pub volume_token0: BigDecimal,
    #[serde(rename = "volumeToken1")]
    pub volume_token1: BigDecimal,
    #[serde(rename = "volumeUSD")]
    pub volume_usd: BigDecimal,
    #[serde(rename = "feesUSD")]
    pub fees_usd: BigDecimal,
    #[serde(rename = "feeGrowthOutside0X128")]
    pub fee_growth_outside0_x128: BigDecimal,
    #[serde(rename = "feeGrowthOutside1X128")]
    pub fee_growth_outside1_x128: BigDecimal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenDayData {
    pub id: String,
    pub date: i32,
    pub token: String,
    pub volume: BigDecimal,
    #[serde(rename = "volumeUSD")]
    pub volume_usd: BigDecimal,
    #[serde(rename = "untrackedVolumeUSD")]
    pub untracked_volume_usd: BigDecimal,
    #[serde(rename = "totalValueLocked")]
    pub total_value_locked: BigDecimal,
    #[serde(rename = "totalValueLockedUSD")]
    pub total_value_locked_usd: BigDecimal,
    #[serde(rename = "priceUSD")]
    pub price_usd: BigDecimal,
    #[serde(rename = "feesUSD")]
    pub fees_usd: BigDecimal,
    pub open: BigDecimal,
    pub high: BigDecimal,
    pub low: BigDecimal,
    pub close: BigDecimal,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenHourData {
    pub id: String,
    #[serde(rename = "periodStartUnix")]
    pub period_start_unix: i32,
    pub token: String,
    pub volume: BigDecimal,
    #[serde(rename = "volumeUSD")]
    pub volume_usd: BigDecimal,
    #[serde(rename = "untrackedVolumeUSD")]
    pub untracked_volume_usd: BigDecimal,
    #[serde(rename = "totalValueLocked")]
    pub total_value_locked: BigDecimal,
    #[serde(rename = "totalValueLockedUSD")]
    pub total_value_locked_usd: BigDecimal,
    #[serde(rename = "priceUSD")]
    pub price_usd: BigDecimal,
    #[serde(rename = "feesUSD")]
    pub fees_usd: BigDecimal,
    pub open: BigDecimal,
    pub high: BigDecimal,
    pub low: BigDecimal,
    pub close: BigDecimal,
}

#[cfg(test)]
/// Creates the default JSON representation for a named test entity.
pub(crate) fn new_entity(kind: &str, id: &str) -> serde_json::Value {
    use serde_json::Value;

    let mut value = match kind {
        "Factory" => serde_json::to_value(Factory::default()),
        "Bundle" => serde_json::to_value(Bundle::default()),
        "Token" => serde_json::to_value(Token::default()),
        "Pool" => serde_json::to_value(Pool::default()),
        "Tick" => serde_json::to_value(Tick::default()),
        "Position" => serde_json::to_value(Position::default()),
        "PositionSnapshot" => serde_json::to_value(PositionSnapshot::default()),
        "Transaction" => serde_json::to_value(Transaction::default()),
        "Mint" => serde_json::to_value(Mint::default()),
        "Burn" => serde_json::to_value(Burn::default()),
        "Swap" => serde_json::to_value(Swap::default()),
        "Collect" => serde_json::to_value(Collect::default()),
        "Flash" => serde_json::to_value(Flash::default()),
        "UniswapDayData" => serde_json::to_value(UniswapDayData::default()),
        "PoolDayData" => serde_json::to_value(PoolDayData::default()),
        "PoolHourData" => serde_json::to_value(PoolHourData::default()),
        "TickHourData" => serde_json::to_value(TickHourData::default()),
        "TickDayData" => serde_json::to_value(TickDayData::default()),
        "TokenDayData" => serde_json::to_value(TokenDayData::default()),
        "TokenHourData" => serde_json::to_value(TokenHourData::default()),
        _ => panic!("unknown application entity"),
    }
    .expect("entity serialization");
    value["id"] = Value::String(id.into());
    value
}

impl raven_engine::Entity for Factory {
    const ENTITY_NAME: &'static str = "Factory";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for Bundle {
    const ENTITY_NAME: &'static str = "Bundle";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for Token {
    const ENTITY_NAME: &'static str = "Token";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for Pool {
    const ENTITY_NAME: &'static str = "Pool";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for Tick {
    const ENTITY_NAME: &'static str = "Tick";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for Position {
    const ENTITY_NAME: &'static str = "Position";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for PositionSnapshot {
    const ENTITY_NAME: &'static str = "PositionSnapshot";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for Transaction {
    const ENTITY_NAME: &'static str = "Transaction";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for Mint {
    const ENTITY_NAME: &'static str = "Mint";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for Burn {
    const ENTITY_NAME: &'static str = "Burn";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for Swap {
    const ENTITY_NAME: &'static str = "Swap";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for Collect {
    const ENTITY_NAME: &'static str = "Collect";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for Flash {
    const ENTITY_NAME: &'static str = "Flash";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for UniswapDayData {
    const ENTITY_NAME: &'static str = "UniswapDayData";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for PoolDayData {
    const ENTITY_NAME: &'static str = "PoolDayData";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for PoolHourData {
    const ENTITY_NAME: &'static str = "PoolHourData";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for TickHourData {
    const ENTITY_NAME: &'static str = "TickHourData";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for TickDayData {
    const ENTITY_NAME: &'static str = "TickDayData";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for TokenDayData {
    const ENTITY_NAME: &'static str = "TokenDayData";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for TokenHourData {
    const ENTITY_NAME: &'static str = "TokenHourData";

    fn id(&self) -> &str {
        &self.id
    }
}
