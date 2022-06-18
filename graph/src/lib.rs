pub mod blockchain;
pub mod components;
pub mod log;

/// Extension traits for external types.
pub mod ext;
mod task_spawn;
pub mod util;

pub use task_spawn::{
    block_on, spawn, spawn_allow_panic, spawn_blocking, spawn_blocking_allow_panic, spawn_thread,
};
pub use tokio_stream;

/// A prelude that makes all system component traits and data types available.
///
/// Add the following code to import all traits and data types listed below at once.
///
/// ```
/// use graph::prelude::*;
///
pub mod prelude {
    pub use crate::blockchain::BlockPtr;
    pub use crate::components::ethereum::{
        EthereumBlock, LightEthereumBlock, LightEthereumBlockExt,
    };
    pub use crate::components::store::{BlockNumber, ChainStore, StoreError};
    pub use crate::ext::futures::{
        CancelGuard, CancelHandle, CancelToken, CancelableError, FutureExtension,
        SharedCancelGuard, StreamExtension,
    };
    pub use crate::log::factory::LoggerFactory;
    pub use crate::util::futures::{retry, TimeoutError};
    pub use ::anyhow;
    pub use anyhow::{anyhow, Context as _, Error};
    pub use async_trait::async_trait;
    pub use ethabi;
    pub use futures::future;
    pub use futures::prelude::*;
    pub use futures::stream;
    pub use futures03;
    pub use futures03::compat::{Future01CompatExt, Sink01CompatExt, Stream01CompatExt};
    pub use futures03::future::{FutureExt as _, TryFutureExt};
    pub use futures03::sink::SinkExt as _;
    pub use futures03::stream::{StreamExt as _, TryStreamExt};
    pub use http;
    pub use lazy_static::lazy_static;
    pub use serde;
    pub use serde_derive::{Deserialize, Serialize};
    pub use serde_json;
    pub use serde_yaml;
    pub use slog;
    pub use slog::{crit, debug, error, info, o, trace, warn, Logger};
    pub use std::pin::Pin;
    pub use std::sync::Arc;
    pub use std::time::Duration;
    pub use thiserror;
    pub use tokio;
    pub use web3;
}
