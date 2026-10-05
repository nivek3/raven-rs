//! EVM pipeline assembly and deterministic parser/handler dispatch.
//!
//! The builder registers parser/handler routes. Supplied sources must cover
//! parser acquisition requirements. Updates are processed in source order, with
//! routes visited in registration order.

use std::sync::Arc;

use async_trait::async_trait;
use raven_engine::{
    BlockBatch, BlockProcessor, BlockSource, CancellationToken, ChainStore, Datasource, Engine,
    EngineError, EntityStore, FinalityPolicy, Handlers, RavenResult, RunOptions,
};

use crate::{Parser, PipelineError, Update};

#[async_trait]
trait Route: Send + Sync {
    /// Parses and handles one update using the route's registered components.
    async fn process(&self, entities: &mut dyn EntityStore, update: &Update) -> RavenResult<()>;
}

struct ParserRoute<P, H> {
    parser: P,
    handlers: H,
}

#[async_trait]
impl<P, H> Route for ParserRoute<P, H>
where
    P: Parser,
    H: Handlers<P::Output>,
{
    /// Sends a parsed value to its handlers when the parser matches the update.
    async fn process(&self, entities: &mut dyn EntityStore, update: &Update) -> RavenResult<()> {
        if let Some(value) = self.parser.parse(update)? {
            self.handlers.handle(entities, &value).await?;
        }
        Ok(())
    }
}

struct PipelineProcessor {
    routes: Vec<Box<dyn Route>>,
}

#[async_trait]
impl<H> BlockProcessor<H, Update> for PipelineProcessor
where
    H: Send + Sync,
{
    /// Dispatches each normalized update to routes in registration order.
    async fn process(
        &self,
        entities: &mut dyn EntityStore,
        batch: &BlockBatch<H, Update>,
    ) -> RavenResult<()> {
        // The source owns normalized update ordering. Never invert these loops:
        // handlers may read entities written by earlier updates and parsers.
        for update in &batch.updates {
            for route in &self.routes {
                route.process(entities, update).await?;
            }
        }
        Ok(())
    }
}

/// A configured parser/handler pipeline backed by the canonical engine.
/// Sequential and random-access sources must cover parser acquisition
/// requirements before being passed in.
pub struct Pipeline<D, S, T> {
    datasource: Arc<D>,
    engine: Engine<S, T, PipelineProcessor>,
    options: RunOptions,
    cancellation: CancellationToken,
}

impl Pipeline<(), (), ()> {
    /// Starts a type-state builder with no source, store or parser routes.
    pub fn builder() -> PipelineBuilder<(), (), ()> {
        PipelineBuilder {
            datasource: (),
            source: (),
            store: (),
            processor: PipelineProcessor { routes: Vec::new() },
            options: RunOptions::default(),
            policy: FinalityPolicy::Head,
            cancellation: CancellationToken::new(),
            metrics_index: "default".to_owned(),
        }
    }
}

impl<D, S, T> Pipeline<D, S, T>
where
    D: Datasource<Update = Update> + 'static,
    D::Hash: Clone + Eq + 'static,
    S: BlockSource<Hash = D::Hash, Update = Update> + 'static,
    T: ChainStore<Hash = D::Hash, ChainIdentity = S::ChainIdentity>,
{
    /// Runs until cancellation, finite-source completion or a terminal error.
    pub async fn run(&mut self) -> RavenResult<()> {
        self.engine
            .run(
                Arc::clone(&self.datasource),
                self.options,
                self.cancellation.clone(),
            )
            .await
    }
}

/// The source/store setters establish their concrete types; missing components
/// cannot build. Parsers may have different output and handler types, but share
/// the normalized EVM update representation of their source.
pub struct PipelineBuilder<D, S, T> {
    datasource: D,
    source: S,
    store: T,
    processor: PipelineProcessor,
    options: RunOptions,
    policy: FinalityPolicy,
    cancellation: CancellationToken,
    metrics_index: String,
}

impl<D, S, T> PipelineBuilder<D, S, T> {
    /// Sets the sequential producer used for catch-up and live ingestion.
    pub fn datasource<N: Datasource<Update = Update>>(
        self,
        datasource: N,
    ) -> PipelineBuilder<N, S, T> {
        PipelineBuilder {
            datasource,
            source: self.source,
            store: self.store,
            processor: self.processor,
            options: self.options,
            policy: self.policy,
            cancellation: self.cancellation,
            metrics_index: self.metrics_index,
        }
    }

    /// Sets the exact random-access source used for verification and reorgs.
    pub fn block_source<N: BlockSource<Update = Update>>(
        self,
        source: N,
    ) -> PipelineBuilder<D, N, T> {
        PipelineBuilder {
            datasource: self.datasource,
            source,
            store: self.store,
            processor: self.processor,
            options: self.options,
            policy: self.policy,
            cancellation: self.cancellation,
            metrics_index: self.metrics_index,
        }
    }

    /// Sets the transactional canonical store.
    pub fn store<N: ChainStore>(self, store: N) -> PipelineBuilder<D, S, N> {
        PipelineBuilder {
            datasource: self.datasource,
            source: self.source,
            store,
            processor: self.processor,
            options: self.options,
            policy: self.policy,
            cancellation: self.cancellation,
            metrics_index: self.metrics_index,
        }
    }

    /// Registers one parser and its ordered handler tuple.
    ///
    /// Routes retain registration order. For each normalized update the pipeline
    /// runs every route, then moves to the next update.
    pub fn parser<P, H>(mut self, parser: P, handlers: H) -> Self
    where
        P: Parser + 'static,
        H: Handlers<P::Output> + 'static,
    {
        self.processor
            .routes
            .push(Box::new(ParserRoute { parser, handlers }));
        self
    }

    /// Sets the immutable indexing start for a fresh store.
    pub fn from_block(mut self, number: u64) -> Self {
        self.options.start_block = number;
        self
    }

    /// Sets the indexing start, channel capacity and chain polling interval.
    pub fn run_options(mut self, options: RunOptions) -> Self {
        self.options = options;
        self
    }

    /// Delays application according to the selected canonical finality policy.
    pub fn finality_policy(mut self, policy: FinalityPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Uses the caller-owned token for cooperative pipeline shutdown.
    pub fn cancellation_token(mut self, cancellation: CancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Labels indexing metrics with a stable name, distinct for each concurrent index.
    pub fn metrics(mut self, index: impl Into<String>) -> Self {
        self.metrics_index = index.into();
        self
    }
}

impl<D, S, T> PipelineBuilder<D, S, T>
where
    D: Datasource<Update = Update> + 'static,
    D::Hash: Clone + Eq + 'static,
    S: BlockSource<Hash = D::Hash, Update = Update> + 'static,
    T: ChainStore<Hash = D::Hash, ChainIdentity = S::ChainIdentity>,
{
    /// Builds a pipeline after checking parser registration and run options.
    pub fn build(self) -> RavenResult<Pipeline<D, S, T>> {
        if self.processor.routes.is_empty() {
            return Err(PipelineError::NoParsers.into());
        }
        if self.options.channel_capacity == 0 || self.options.poll_interval.is_zero() {
            return Err(EngineError::InvalidRunOptions.into());
        }
        Ok(Pipeline {
            datasource: Arc::new(self.datasource),
            engine: Engine::new(Arc::new(self.source), self.store, self.processor)
                .with_finality_policy(self.policy)
                .with_metrics(self.metrics_index),
            options: self.options,
            cancellation: self.cancellation,
        })
    }
}
