//! Application day/hour aggregates with their required read/save ordering.
use bigdecimal::BigDecimal;
use raven_engine::{EntityStore, EntityStoreExt, RavenResult};

use crate::{
    entities::{
        Bundle, Factory, Pool, PoolDayData, PoolHourData, Tick, TickDayData, Token, TokenDayData,
        TokenHourData, UniswapDayData,
    },
    handlers::{Context, load},
    math::{integer, round34},
};

pub(crate) struct Metrics {
    pub amount0: BigDecimal,
    pub amount1: BigDecimal,
    pub usd: BigDecimal,
    pub eth: BigDecimal,
    pub fees: BigDecimal,
}

/// Updates the pool's hourly and daily aggregate records.
pub(crate) async fn pool_intervals(
    store: &mut dyn EntityStore,
    context: &Context,
    pool: &Pool,
    metrics: Option<&Metrics>,
) -> RavenResult<()> {
    let day = context.timestamp / 86_400;
    let day_id = format!("{}-{day}", pool.id);
    let mut day_data = store
        .load::<PoolDayData>(&day_id)
        .await?
        .unwrap_or_else(|| PoolDayData {
            id: day_id.clone(),
            date: i32::try_from(day * 86_400).expect("day timestamp fits Graph Int"),
            pool: pool.id.clone(),
            open: pool.token0_price.clone(),
            high: pool.token0_price.clone(),
            low: pool.token0_price.clone(),
            close: pool.token0_price.clone(),
            ..Default::default()
        });
    update_pool_day(&mut day_data, pool, metrics);
    store.save(&day_data).await?;

    let hour = context.timestamp / 3_600;
    let hour_id = format!("{}-{hour}", pool.id);
    let mut hour_data = store
        .load::<PoolHourData>(&hour_id)
        .await?
        .unwrap_or_else(|| PoolHourData {
            id: hour_id.clone(),
            period_start_unix: i32::try_from(hour * 3_600).expect("hour timestamp fits Graph Int"),
            pool: pool.id.clone(),
            open: pool.token0_price.clone(),
            high: pool.token0_price.clone(),
            low: pool.token0_price.clone(),
            close: pool.token0_price.clone(),
            ..Default::default()
        });
    update_pool_hour(&mut hour_data, pool, metrics);
    store.save(&hour_data).await
}

/// Copies pool state and optional event metrics into a daily aggregate.
fn update_pool_day(value: &mut PoolDayData, pool: &Pool, metrics: Option<&Metrics>) {
    let price = pool.token0_price.clone();
    if price > value.high {
        value.high = price.clone();
    }
    if price < value.low {
        value.low = price.clone();
    }
    value.close = price;
    value.liquidity = pool.liquidity.clone();
    value.sqrt_price = pool.sqrt_price.clone();
    value.token0_price = pool.token0_price.clone();
    value.token1_price = pool.token1_price.clone();
    value.tick = pool.tick.clone();
    value.fee_growth_global0_x128 = pool.fee_growth_global0_x128.clone();
    value.fee_growth_global1_x128 = pool.fee_growth_global1_x128.clone();
    value.tvl_usd = pool.total_value_locked_usd.clone();
    value.tx_count = value.tx_count.clone() + integer("1");
    if let Some(metrics) = metrics {
        value.volume_token0 = round34(value.volume_token0.clone() + metrics.amount0.clone());
        value.volume_token1 = round34(value.volume_token1.clone() + metrics.amount1.clone());
        value.volume_usd = round34(value.volume_usd.clone() + metrics.usd.clone());
        value.fees_usd = round34(value.fees_usd.clone() + metrics.fees.clone());
    }
}

/// Copies pool state and optional event metrics into an hourly aggregate.
fn update_pool_hour(value: &mut PoolHourData, pool: &Pool, metrics: Option<&Metrics>) {
    let price = pool.token0_price.clone();
    if price > value.high {
        value.high = price.clone();
    }
    if price < value.low {
        value.low = price.clone();
    }
    value.close = price;
    value.liquidity = pool.liquidity.clone();
    value.sqrt_price = pool.sqrt_price.clone();
    value.token0_price = pool.token0_price.clone();
    value.token1_price = pool.token1_price.clone();
    value.tick = pool.tick.clone();
    value.fee_growth_global0_x128 = pool.fee_growth_global0_x128.clone();
    value.fee_growth_global1_x128 = pool.fee_growth_global1_x128.clone();
    value.tvl_usd = pool.total_value_locked_usd.clone();
    value.tx_count = value.tx_count.clone() + integer("1");
    if let Some(metrics) = metrics {
        value.volume_token0 = round34(value.volume_token0.clone() + metrics.amount0.clone());
        value.volume_token1 = round34(value.volume_token1.clone() + metrics.amount1.clone());
        value.volume_usd = round34(value.volume_usd.clone() + metrics.usd.clone());
        value.fees_usd = round34(value.fees_usd.clone() + metrics.fees.clone());
    }
}

/// Updates the daily aggregate for an initialized tick.
pub(crate) async fn tick_day(
    store: &mut dyn EntityStore,
    context: &Context,
    tick: &Tick,
) -> RavenResult<()> {
    let day = context.timestamp / 86_400;
    let id = format!("{}-{day}", tick.id);
    let mut value = store
        .load::<TickDayData>(&id)
        .await?
        .unwrap_or_else(|| TickDayData {
            id: id.clone(),
            date: i32::try_from(day * 86_400).expect("day timestamp fits Graph Int"),
            pool: tick.pool.clone(),
            tick: tick.id.clone(),
            ..Default::default()
        });
    value.liquidity_gross = tick.liquidity_gross.clone();
    value.liquidity_net = tick.liquidity_net.clone();
    value.volume_token0 = tick.volume_token0.clone();
    // Keep both token volume fields aligned with volumeToken0.
    value.volume_token1 = tick.volume_token0.clone();
    value.volume_usd = tick.volume_usd.clone();
    value.fees_usd = tick.fees_usd.clone();
    value.fee_growth_outside0_x128 = tick.fee_growth_outside0_x128.clone();
    value.fee_growth_outside1_x128 = tick.fee_growth_outside1_x128.clone();
    store.save(&value).await
}

/// Updates protocol daily aggregates and pool/token interval aggregates for an event.
pub(crate) async fn update(
    store: &mut dyn EntityStore,
    context: &Context,
    factory_id: &str,
    pool_id: &str,
    token0: &Token,
    token1: &Token,
    metrics: Option<&Metrics>,
) -> RavenResult<()> {
    let factory = load::<Factory>(store, factory_id).await?;
    let day = context.timestamp / 86_400;
    let id = day.to_string();
    let mut data = store
        .load::<UniswapDayData>(&id)
        .await?
        .unwrap_or_else(|| UniswapDayData {
            id: id.clone(),
            date: i32::try_from(day * 86_400).expect("day timestamp fits Graph Int"),
            ..Default::default()
        });
    data.tvl_usd = factory.total_value_locked_usd;
    data.tx_count = factory.tx_count;
    if let Some(metrics) = metrics {
        data.volume_eth = round34(data.volume_eth.clone() + metrics.eth.clone());
        data.volume_usd = round34(data.volume_usd.clone() + metrics.usd.clone());
        data.fees_usd = round34(data.fees_usd.clone() + metrics.fees.clone());
    }
    store.save(&data).await?;

    let pool = load::<Pool>(store, pool_id).await?;
    pool_intervals(store, context, &pool, metrics).await?;
    let bundle = load::<Bundle>(store, "1").await?;
    update_token_intervals(
        store,
        context,
        token0,
        metrics.map(|m| &m.amount0),
        &bundle,
        metrics,
    )
    .await?;
    update_token_intervals(
        store,
        context,
        token1,
        metrics.map(|m| &m.amount1),
        &bundle,
        metrics,
    )
    .await
}

/// Updates one token's hourly and daily aggregates.
async fn update_token_intervals(
    store: &mut dyn EntityStore,
    context: &Context,
    token: &Token,
    amount: Option<&BigDecimal>,
    bundle: &Bundle,
    metrics: Option<&Metrics>,
) -> RavenResult<()> {
    let price = round34(token.derived_eth.clone() * bundle.eth_price_usd.clone());
    let day = context.timestamp / 86_400;
    let day_id = format!("{}-{day}", token.id);
    let mut day_data = store
        .load::<TokenDayData>(&day_id)
        .await?
        .unwrap_or_else(|| TokenDayData {
            id: day_id.clone(),
            date: i32::try_from(day * 86_400).expect("day timestamp fits Graph Int"),
            token: token.id.clone(),
            open: price.clone(),
            high: price.clone(),
            low: price.clone(),
            close: price.clone(),
            ..Default::default()
        });
    update_token_day(&mut day_data, token, &price, amount, metrics);
    store.save(&day_data).await?;

    let hour = context.timestamp / 3_600;
    let hour_id = format!("{}-{hour}", token.id);
    let mut hour_data = store
        .load::<TokenHourData>(&hour_id)
        .await?
        .unwrap_or_else(|| TokenHourData {
            id: hour_id.clone(),
            period_start_unix: i32::try_from(hour * 3_600).expect("hour timestamp fits Graph Int"),
            token: token.id.clone(),
            open: price.clone(),
            high: price.clone(),
            low: price.clone(),
            close: price.clone(),
            ..Default::default()
        });
    update_token_hour(&mut hour_data, token, &price, amount, metrics);
    store.save(&hour_data).await
}

/// Copies token state and optional event metrics into a daily aggregate.
fn update_token_day(
    value: &mut TokenDayData,
    token: &Token,
    price: &BigDecimal,
    amount: Option<&BigDecimal>,
    metrics: Option<&Metrics>,
) {
    if price > &value.high {
        value.high = price.clone();
    }
    if price < &value.low {
        value.low = price.clone();
    }
    value.close = price.clone();
    value.price_usd = price.clone();
    value.total_value_locked = token.total_value_locked.clone();
    value.total_value_locked_usd = token.total_value_locked_usd.clone();
    if let (Some(metrics), Some(amount)) = (metrics, amount) {
        value.volume = round34(value.volume.clone() + amount.clone());
        value.volume_usd = round34(value.volume_usd.clone() + metrics.usd.clone());
        // This aggregate uses tracked USD for untracked_volume_usd.
        value.untracked_volume_usd =
            round34(value.untracked_volume_usd.clone() + metrics.usd.clone());
        value.fees_usd = round34(value.fees_usd.clone() + metrics.fees.clone());
    }
}

/// Copies token state and optional event metrics into an hourly aggregate.
fn update_token_hour(
    value: &mut TokenHourData,
    token: &Token,
    price: &BigDecimal,
    amount: Option<&BigDecimal>,
    metrics: Option<&Metrics>,
) {
    if price > &value.high {
        value.high = price.clone();
    }
    if price < &value.low {
        value.low = price.clone();
    }
    value.close = price.clone();
    value.price_usd = price.clone();
    value.total_value_locked = token.total_value_locked.clone();
    value.total_value_locked_usd = token.total_value_locked_usd.clone();
    if let (Some(metrics), Some(amount)) = (metrics, amount) {
        value.volume = round34(value.volume.clone() + amount.clone());
        value.volume_usd = round34(value.volume_usd.clone() + metrics.usd.clone());
        // This aggregate uses tracked USD for untracked_volume_usd.
        value.untracked_volume_usd =
            round34(value.untracked_volume_usd.clone() + metrics.usd.clone());
        value.fees_usd = round34(value.fees_usd.clone() + metrics.fees.clone());
    }
}
