mod fake;

use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use async_trait::async_trait;
use raven_engine::{
    BlockSource, CancellationToken, Datasource, Engine, EngineError, EntityStore, EntityValue,
    Handler, IngestOutcome, RavenError, RavenResult, RunOptions,
};
use raven_evm::{EvmFilter, Filter, Parser, Pipeline, PipelineError, Update};
use serde_json::json;

use crate::fake::{
    FakeProcessor, FakeSource, FakeStore, address, chain_block, hash, header, log_value, value_log,
};

struct Parsed {
    block_number: u64,
    value: u8,
    parser: &'static str,
}

struct RecordingParser {
    name: &'static str,
    calls: Arc<AtomicUsize>,
    skip: Option<u8>,
    fail: Option<u8>,
}

impl RecordingParser {
    /// Creates the configured fixture instance.
    fn new(name: &'static str) -> Self {
        Self {
            name,
            calls: Arc::new(AtomicUsize::new(0)),
            skip: None,
            fail: None,
        }
    }
}

/// Requests every fixture log.
fn all_logs() -> EvmFilter {
    EvmFilter {
        blocks: false,
        logs: Some(Filter::default()),
    }
}

/// Builds address constraints for selected fixture values.
fn select_filters(values: &[u8]) -> EvmFilter {
    EvmFilter {
        blocks: false,
        logs: Some(Filter::new().address(values.iter().copied().map(address).collect::<Vec<_>>())),
    }
}

#[async_trait]
impl Parser for RecordingParser {
    type Output = Parsed;

    /// Declares the fixture parser filter.
    fn filter(&self) -> EvmFilter {
        all_logs()
    }

    /// Parses one fixture update when it matches.
    async fn parse(&self, update: &Update) -> RavenResult<Option<Parsed>> {
        let Some(value) = log_value(update) else {
            return Ok(None);
        };
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.skip == Some(value) {
            return Ok(None);
        }
        if self.fail == Some(value) {
            return Err(RavenError::Parser(Box::new(io::Error::other(
                "malformed fixture update",
            ))));
        }
        let Update::Log(log) = update else {
            unreachable!("fixture parser accepts only logs")
        };
        Ok(Some(Parsed {
            block_number: log.block_number,
            value,
            parser: self.name,
        }))
    }
}

struct Append(&'static str);

#[async_trait]
impl Handler<Parsed> for Append {
    /// Applies one fixture handler action.
    async fn handle(&self, entities: &mut dyn EntityStore, value: &Parsed) -> RavenResult<()> {
        let block = value.block_number;
        let mut trace = entities.get("Trace", "main").await?.unwrap_or(json!([]));
        trace
            .as_array_mut()
            .unwrap()
            .push(json!([block, value.value, value.parser, self.0]));
        entities.put("Trace", "main", &trace).await
    }
}

struct CopyTrace;

#[async_trait]
impl Handler<Parsed> for CopyTrace {
    /// Applies one fixture handler action.
    async fn handle(&self, entities: &mut dyn EntityStore, _: &Parsed) -> RavenResult<()> {
        let trace = entities
            .get("Trace", "main")
            .await?
            .expect("earlier handler write");
        entities.put("Trace", "mirror", &trace).await
    }
}

/// Builds a contiguous fixture source from per-block values.
fn source(updates: Vec<Vec<u8>>) -> FakeSource {
    let source = FakeSource::new(1);
    let mut batches = vec![chain_block(0, 10, 0, vec![])];
    for (index, updates) in updates.into_iter().enumerate() {
        let number = index as u64 + 1;
        batches.push(chain_block(
            number,
            number as u8 + 10,
            number as u8 + 9,
            updates,
        ));
    }
    source.set_chain(batches);
    source
}

/// Builds a source containing matched and unmatched fixture logs.
fn filter_source() -> FakeSource {
    let source = FakeSource::new(1);
    source.set_chain(vec![
        chain_block(0, 10, 0, vec![]),
        chain_block(1, 11, 10, vec![9, 1, 2, 3, 1]),
        chain_block(2, 12, 11, vec![9]),
        chain_block(3, 13, 12, vec![2, 3]),
    ]);
    source
}

struct Select {
    values: Vec<u8>,
    name: &'static str,
    malformed: Option<u8>,
}

impl Select {
    /// Creates the configured fixture instance.
    fn new(values: Vec<u8>, name: &'static str) -> Self {
        Self {
            values,
            name,
            malformed: None,
        }
    }
}

#[async_trait]
impl Parser for Select {
    type Output = (u8, &'static str, u64);

    /// Declares the fixture parser filter.
    fn filter(&self) -> EvmFilter {
        select_filters(&self.values)
    }

    /// Parses one fixture update when it matches.
    async fn parse(&self, update: &Update) -> RavenResult<Option<Self::Output>> {
        let Some(value) = log_value(update) else {
            return Ok(None);
        };
        if !self.values.contains(&value) {
            return Ok(None);
        }
        if self.malformed == Some(value) {
            return Err(RavenError::Parser(Box::new(io::Error::other(
                "malformed matching input",
            ))));
        }
        let Update::Log(log) = update else {
            unreachable!("fixture parser accepts only logs")
        };
        Ok(Some((value, self.name, log.block_number)))
    }
}

struct Record;

#[async_trait]
impl Handler<(u8, &'static str, u64)> for Record {
    /// Applies one fixture handler action.
    async fn handle(
        &self,
        entities: &mut dyn EntityStore,
        value: &(u8, &'static str, u64),
    ) -> RavenResult<()> {
        let number = value.2;
        let mut trace = entities.get("Trace", "main").await?.unwrap_or(json!([]));
        trace
            .as_array_mut()
            .unwrap()
            .push(json!([number, value.0, value.1]));
        entities.put("Trace", "main", &trace).await
    }
}

#[tokio::test]
/// Verifies that update parser and handler order is stable with read your writes.
async fn update_parser_and_handler_order_is_stable_with_read_your_writes() {
    let source = source(vec![vec![1, 2]]);
    let store = FakeStore::default();
    let parser = RecordingParser::new("a");
    let calls = Arc::clone(&parser.calls);
    let mut pipeline = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(store.clone())
        .from_block(1)
        .parser(parser, (Append("first"), CopyTrace))
        .parser(RecordingParser::new("b"), (Append("second"),))
        .build()
        .unwrap();
    pipeline.run().await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2); // Once per update, not once per handler.
    assert_eq!(
        store.entity("Trace", "main"),
        Some(json!([
            [1, 1, "a", "first"],
            [1, 1, "b", "second"],
            [1, 2, "a", "first"],
            [1, 2, "b", "second"]
        ]))
    );
    assert_eq!(
        store.entity("Trace", "mirror"),
        Some(json!([
            [1, 1, "a", "first"],
            [1, 1, "b", "second"],
            [1, 2, "a", "first"]
        ]))
    );
    pipeline.run().await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2); // Restart does not replay committed blocks.
}

struct TextParser;

#[async_trait]
impl Parser for TextParser {
    type Output = String;
    /// Declares the fixture parser filter.
    fn filter(&self) -> EvmFilter {
        all_logs()
    }
    /// Parses one fixture update when it matches.
    async fn parse(&self, update: &Update) -> RavenResult<Option<String>> {
        Ok(log_value(update).map(|value| value.to_string()))
    }
}

struct TextHandler;

#[async_trait]
impl Handler<String> for TextHandler {
    /// Applies one fixture handler action.
    async fn handle(&self, entities: &mut dyn EntityStore, value: &String) -> RavenResult<()> {
        entities.put("Text", value, &json!(value)).await
    }
}

#[tokio::test]
/// Verifies that different parser output types share the same pipeline.
async fn different_parser_output_types_share_the_same_pipeline() {
    let source = source(vec![vec![7]]);
    let store = FakeStore::default();
    Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(store.clone())
        .from_block(1)
        .parser(RecordingParser::new("numeric"), (Append("first"),))
        .parser(TextParser, (TextHandler,))
        .build()
        .unwrap()
        .run()
        .await
        .unwrap();
    assert_eq!(store.entity("Text", "7"), Some(json!("7")));
    assert!(store.entity("Trace", "main").is_some());
}

#[tokio::test]
/// Verifies that nonmatching updates and empty blocks do not invoke handlers.
async fn nonmatching_updates_and_empty_blocks_do_not_invoke_handlers() {
    let source = source(vec![vec![1], vec![], vec![2]]);
    let store = FakeStore::default();
    let mut parser = RecordingParser::new("selective");
    parser.skip = Some(1);
    Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(store.clone())
        .from_block(1)
        .parser(parser, (Append("only"),))
        .build()
        .unwrap()
        .run()
        .await
        .unwrap();
    assert_eq!(
        store.entity("Trace", "main"),
        Some(json!([[3, 2, "selective", "only"]]))
    );
    assert_eq!(store.block_ptr().unwrap().number, 3);
    assert_eq!(store.operations().len(), 3);
}

#[tokio::test]
/// Verifies that parser can run without pipeline source or database.
async fn parser_can_run_without_pipeline_source_or_database() {
    let mut parser = RecordingParser::new("offline");
    parser.skip = Some(1);
    parser.fail = Some(2);
    let header = header();
    assert!(
        parser
            .parse(&Update::Log(value_log(&header, 1, 0)))
            .await
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        parser.parse(&Update::Log(value_log(&header, 2, 1))).await,
        Err(RavenError::Parser(_))
    ));
    assert_eq!(
        parser
            .parse(&Update::Log(value_log(&header, 3, 2)))
            .await
            .unwrap()
            .unwrap()
            .value,
        3
    );
}

#[tokio::test]
/// Verifies that parser failure rolls back writes from earlier updates in the block.
async fn parser_failure_rolls_back_writes_from_earlier_updates_in_the_block() {
    let source = source(vec![vec![1, 2]]);
    let store = FakeStore::default();
    let mut parser = RecordingParser::new("a");
    parser.fail = Some(2);
    let result = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(store.clone())
        .from_block(1)
        .parser(parser, (Append("first"),))
        .build()
        .unwrap()
        .run()
        .await;
    assert!(matches!(result, Err(RavenError::Parser(_))));
    assert!(store.entity("Trace", "main").is_none());
    assert!(store.block_ptr().is_none());
}

struct FailAfterWrite;

#[async_trait]
impl Handler<Parsed> for FailAfterWrite {
    /// Applies one fixture handler action.
    async fn handle(&self, entities: &mut dyn EntityStore, _: &Parsed) -> RavenResult<()> {
        entities.put("Temporary", "failed", &json!(true)).await?;
        Err(RavenError::Handler(Box::new(io::Error::other(
            "fixture handler failed",
        ))))
    }
}

struct NeverHandler;

#[async_trait]
impl Handler<Parsed> for NeverHandler {
    /// Applies one fixture handler action.
    async fn handle(&self, _: &mut dyn EntityStore, _: &Parsed) -> RavenResult<()> {
        panic!("handlers after a failure must not execute");
    }
}

#[tokio::test]
/// Verifies that handler failure stops later handlers and discards all staged entities.
async fn handler_failure_stops_later_handlers_and_discards_all_staged_entities() {
    let source = source(vec![vec![1]]);
    let store = FakeStore::default();
    let result = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(store.clone())
        .from_block(1)
        .parser(
            RecordingParser::new("a"),
            (Append("first"), FailAfterWrite, NeverHandler),
        )
        .build()
        .unwrap()
        .run()
        .await;
    assert!(matches!(result, Err(RavenError::Handler(_))));
    assert!(store.entity("Trace", "main").is_none());
    assert!(store.entity("Temporary", "failed").is_none());
    assert!(store.block_ptr().is_none());
}

#[tokio::test]
/// Verifies that entity write failure propagates as store error without advancing block ptr.
async fn entity_write_failure_propagates_as_store_error_without_advancing_block_ptr() {
    let source = source(vec![vec![1]]);
    let store = FakeStore::default();
    store.fail_entity_write(true);
    let result = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(store.clone())
        .from_block(1)
        .parser(RecordingParser::new("a"), (Append("first"),))
        .build()
        .unwrap()
        .run()
        .await;
    assert!(matches!(result, Err(RavenError::ChainStore(_))));
    assert!(store.entity("Trace", "main").is_none());
    assert!(store.block_ptr().is_none());
}

#[tokio::test]
/// Verifies that commit failure does not publish handler writes.
async fn commit_failure_does_not_publish_handler_writes() {
    let source = source(vec![vec![1]]);
    let store = FakeStore::default();
    store.fail_commit(Some(hash(11)));
    let result = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(store.clone())
        .from_block(1)
        .parser(RecordingParser::new("a"), (Append("first"),))
        .build()
        .unwrap()
        .run()
        .await;
    assert!(matches!(result, Err(RavenError::ChainStore(_))));
    assert!(store.entity("Trace", "main").is_none());
    assert!(store.block_ptr().is_none());
}

#[tokio::test]
/// Verifies that failure in a later block preserves the prior committed block.
async fn failure_in_a_later_block_preserves_the_prior_committed_block() {
    let source = source(vec![vec![1], vec![2, 3]]);
    let store = FakeStore::default();
    let mut parser = RecordingParser::new("a");
    parser.fail = Some(3);
    let result = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(store.clone())
        .from_block(1)
        .parser(parser, (Append("first"),))
        .build()
        .unwrap()
        .run()
        .await;
    assert!(matches!(result, Err(RavenError::Parser(_))));
    assert_eq!(
        store.entity("Trace", "main"),
        Some(json!([[1, 1, "a", "first"]]))
    );
    assert_eq!(store.entity_block_hash("Trace", "main"), Some(hash(11)));
    assert_eq!(store.block_ptr().unwrap().number, 1);
}

#[tokio::test]
/// Verifies that reorg replays handler state to the same result as clean canonical replay.
async fn reorg_replays_handler_state_to_the_same_result_as_clean_canonical_replay() {
    let source = source(vec![vec![1], vec![2]]);
    let store = FakeStore::default();
    let mut pipeline = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source.clone())
        .store(store.clone())
        .from_block(1)
        .parser(RecordingParser::new("a"), (Append("first"),))
        .build()
        .unwrap();
    pipeline.run().await.unwrap();
    source.set_chain(vec![
        chain_block(0, 10, 0, vec![]),
        chain_block(1, 11, 10, vec![1]),
        chain_block(2, 22, 11, vec![20]),
    ]);
    pipeline.run().await.unwrap();
    let clean = FakeStore::default();
    Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(clean.clone())
        .from_block(1)
        .parser(RecordingParser::new("a"), (Append("first"),))
        .build()
        .unwrap()
        .run()
        .await
        .unwrap();
    assert_eq!(store.entity("Trace", "main"), clean.entity("Trace", "main"));
    assert_eq!(
        store.entity("Trace", "main"),
        Some(json!([[1, 1, "a", "first"], [2, 20, "a", "first"]]))
    );
    assert_eq!(store.entity_block_hash("Trace", "main"), Some(hash(22)));
}

#[tokio::test]
/// Verifies that empty replacement restores the prior entity metadata.
async fn empty_replacement_restores_the_prior_entity_metadata() {
    let source = source(vec![vec![1], vec![2]]);
    let store = FakeStore::default();
    let mut pipeline = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source.clone())
        .store(store.clone())
        .from_block(1)
        .parser(RecordingParser::new("a"), (Append("first"),))
        .build()
        .unwrap();
    pipeline.run().await.unwrap();
    source.set_chain(vec![
        chain_block(0, 10, 0, vec![]),
        chain_block(1, 11, 10, vec![1]),
        chain_block(2, 22, 11, vec![]),
    ]);
    pipeline.run().await.unwrap();
    assert_eq!(
        store.entity("Trace", "main"),
        Some(json!([[1, 1, "a", "first"]]))
    );
    assert_eq!(store.entity_block_hash("Trace", "main"), Some(hash(11)));
    assert_eq!(store.block_ptr().unwrap().hash, hash(22));
}

struct EntityCommands;

#[async_trait]
impl Handler<Parsed> for EntityCommands {
    /// Applies one fixture handler action.
    async fn handle(&self, entities: &mut dyn EntityStore, parsed: &Parsed) -> RavenResult<()> {
        match parsed.value {
            1 => entities.put("Entity", "key", &EntityValue::Null).await?,
            2 => {
                assert_eq!(
                    entities.get("Entity", "key").await?,
                    Some(EntityValue::Null)
                );
                entities.delete("Entity", "key").await?;
                assert_eq!(entities.get("Entity", "key").await?, None);
            }
            3 => entities.put("Entity", "key", &json!({"value": 3})).await?,
            _ => unreachable!(),
        }
        Ok(())
    }
}

#[tokio::test]
/// Verifies that repeated entity mutations preserve null vs absence and revert cleanly.
async fn repeated_entity_mutations_preserve_null_vs_absence_and_revert_cleanly() {
    let source = source(vec![vec![1], vec![2, 3]]);
    let store = FakeStore::default();
    let mut pipeline = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source.clone())
        .store(store.clone())
        .from_block(1)
        .parser(RecordingParser::new("a"), (EntityCommands,))
        .build()
        .unwrap();
    pipeline.run().await.unwrap();
    assert_eq!(store.entity("Entity", "key"), Some(json!({"value": 3})));
    source.set_chain(vec![
        chain_block(0, 10, 0, vec![]),
        chain_block(1, 11, 10, vec![1]),
        chain_block(2, 22, 11, vec![]),
    ]);
    pipeline.run().await.unwrap();
    assert_eq!(store.entity("Entity", "key"), Some(EntityValue::Null));
    assert_eq!(store.entity_block_hash("Entity", "key"), Some(hash(11)));
}

#[test]
/// Verifies that builder rejects empty routes and invalid runtime options without initializing store.
fn builder_rejects_empty_routes_and_invalid_runtime_options_without_initializing_store() {
    let source = source(vec![vec![1]]);
    let store = FakeStore::default();
    let result = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source.clone())
        .store(store.clone())
        .build();
    assert!(matches!(result, Err(RavenError::Configuration(error))
        if error.downcast_ref::<PipelineError>() == Some(&PipelineError::NoParsers)));
    let result = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(store.clone())
        .parser(RecordingParser::new("a"), (Append("first"),))
        .run_options(RunOptions {
            channel_size: 0,
            ..RunOptions::default()
        })
        .build();
    assert!(matches!(
        result,
        Err(RavenError::Engine(EngineError::InvalidRunOptions))
    ));
    assert!(store.network().is_none());
}

#[tokio::test]
/// Verifies that configured cancellation reaches the runtime before source startup.
async fn configured_cancellation_reaches_the_runtime_before_source_startup() {
    let source = source(vec![vec![1]]);
    let store = FakeStore::default();
    let token = CancellationToken::new();
    token.cancel();
    Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(store.clone())
        .parser(RecordingParser::new("a"), (Append("first"),))
        .cancellation_token(token)
        .build()
        .unwrap()
        .run()
        .await
        .unwrap();
    assert!(store.network().is_none());
    assert!(store.entity("Trace", "main").is_none());
}

#[tokio::test]
/// Verifies that build preserves a broader caller configured source filter.
async fn build_preserves_a_broader_caller_configured_source_filter() {
    let source = filter_source().with_filter(select_filters(&[1, 2, 3]));
    let mut pipeline = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source.clone())
        .store(FakeStore::default())
        .from_block(1)
        .parser(Select::new(vec![1], "a"), (Record,))
        .build()
        .unwrap();
    pipeline.run().await.unwrap();
    let batch = source.block_by_number(1).await.unwrap().unwrap();
    assert!(
        batch
            .updates
            .iter()
            .any(|update| log_value(update) == Some(2))
    );
    assert!(
        batch
            .updates
            .iter()
            .any(|update| log_value(update) == Some(3))
    );
}

#[tokio::test]
/// Verifies that sequential and hash lookups return identical filtered batches including empty blocks.
async fn sequential_and_hash_lookups_return_identical_filtered_batches_including_empty_blocks() {
    let source = filter_source().with_filter(select_filters(&[1, 3]));
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
    source
        .consume(1, sender, CancellationToken::new())
        .await
        .unwrap();
    let mut batches = Vec::new();
    while let Some(batch) = receiver.recv().await {
        assert_eq!(
            source.block_by_hash(&batch.header.hash).await.unwrap(),
            Some(batch.clone())
        );
        assert_eq!(
            source.block_by_number(batch.header.number).await.unwrap(),
            Some(batch.clone())
        );
        batches.push(batch);
    }
    assert_eq!(batches.len(), 3);
    assert_eq!(
        batches[0]
            .updates
            .iter()
            .filter_map(log_value)
            .collect::<Vec<_>>(),
        vec![1, 3, 1]
    );
    assert!(batches[1].updates.is_empty());
    assert_eq!(batches[1].header.number, 2);
}

#[tokio::test]
/// Verifies that filtering preserves handler inputs order and empty blocks advance block ptr.
async fn filtering_preserves_handler_inputs_order_and_empty_blocks_advance_block_ptr() {
    let mut results = Vec::new();
    for filter in [select_filters(&[1, 2, 3, 9]), select_filters(&[1, 2, 3])] {
        let source = filter_source().with_filter(filter);
        let store = FakeStore::default();
        Pipeline::builder()
            .datasource(source.clone())
            .block_source(source)
            .store(store.clone())
            .from_block(1)
            .parser(Select::new(vec![1, 2], "a"), (Record,))
            .parser(Select::new(vec![2, 3], "b"), (Record,))
            .build()
            .unwrap()
            .run()
            .await
            .unwrap();
        assert_eq!(store.block_ptr().unwrap().number, 3);
        assert_eq!(
            store.operations(),
            vec![(true, hash(11)), (true, hash(12)), (true, hash(13))]
        );
        results.push(store.entity("Trace", "main"));
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(
        results[0],
        Some(json!([
            [1, 1, "a"],
            [1, 2, "a"],
            [1, 2, "b"],
            [1, 3, "b"],
            [1, 1, "a"],
            [3, 2, "a"],
            [3, 2, "b"],
            [3, 3, "b"]
        ]))
    );
}

#[tokio::test]
/// Verifies that gap ingestion fetches filtered ancestors and commits empty blocks.
async fn gap_ingestion_fetches_filtered_ancestors_and_commits_empty_blocks() {
    let source = Arc::new(filter_source().with_filter(select_filters(&[1, 3])));
    let store = FakeStore::default();
    let mut engine = Engine::new(Arc::clone(&source), store.clone(), FakeProcessor);
    engine.initialize(1).await.unwrap();
    let first = source.block_by_hash(&hash(11)).await.unwrap().unwrap();
    assert_eq!(engine.ingest(first).await.unwrap(), IngestOutcome::Applied);
    let third = source.block_by_hash(&hash(13)).await.unwrap().unwrap();
    assert_eq!(engine.ingest(third).await.unwrap(), IngestOutcome::Resync);
    engine.resync().await.unwrap();
    assert_eq!(store.values(), vec![1, 3, 1, 3]);
    assert_eq!(
        store.operations(),
        vec![(true, hash(11)), (true, hash(12)), (true, hash(13))]
    );
}

#[tokio::test]
/// Verifies that building another pipeline does not change existing source filters.
async fn building_another_pipeline_does_not_change_existing_source_filters() {
    let source = filter_source();
    let first = FakeStore::default();
    let second = FakeStore::default();
    let mut a = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source.clone())
        .store(first.clone())
        .from_block(1)
        .parser(Select::new(vec![1], "a"), (Record,))
        .build()
        .unwrap();
    let mut b = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(second.clone())
        .from_block(1)
        .parser(Select::new(vec![3], "b"), (Record,))
        .build()
        .unwrap();
    b.run().await.unwrap();
    a.run().await.unwrap();
    assert_eq!(
        first.entity("Trace", "main"),
        Some(json!([[1, 1, "a"], [1, 1, "a"]]))
    );
    assert_eq!(
        second.entity("Trace", "main"),
        Some(json!([[1, 3, "b"], [3, 3, "b"]]))
    );
}

#[tokio::test]
/// Verifies that reorg and gap recovery keep the original filters and equal clean replay.
async fn reorg_and_gap_recovery_keep_the_original_filters_and_equal_clean_replay() {
    let source = filter_source().with_filter(select_filters(&[1, 3]));
    let store = FakeStore::default();
    let mut pipeline = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source.clone())
        .store(store.clone())
        .from_block(1)
        .parser(Select::new(vec![1, 3], "a"), (Record,))
        .build()
        .unwrap();
    pipeline.run().await.unwrap();
    source.set_chain(vec![
        chain_block(0, 10, 0, vec![]),
        chain_block(1, 11, 10, vec![9, 1, 2, 3, 1]),
        chain_block(2, 22, 11, vec![9]),
        chain_block(3, 23, 22, vec![3, 9]),
        chain_block(4, 24, 23, vec![9]),
        chain_block(5, 25, 24, vec![1]),
    ]);
    pipeline.run().await.unwrap();
    assert_eq!(store.block_ptr().unwrap().hash, hash(25));
    assert_eq!(
        &store.operations()[3..],
        &[
            (false, hash(13)),
            (false, hash(12)),
            (true, hash(22)),
            (true, hash(23)),
            (true, hash(24)),
            (true, hash(25)),
        ]
    );
    let clean = FakeStore::default();
    let clean_source = source.with_filter(select_filters(&[1, 2, 3, 9]));
    Pipeline::builder()
        .datasource(clean_source.clone())
        .block_source(clean_source)
        .store(clean.clone())
        .from_block(1)
        .parser(Select::new(vec![1, 3], "a"), (Record,))
        .build()
        .unwrap()
        .run()
        .await
        .unwrap();
    assert_eq!(store.entity("Trace", "main"), clean.entity("Trace", "main"));
    assert_eq!(
        store.entity_block_hash("Trace", "main"),
        clean.entity_block_hash("Trace", "main")
    );
}

#[tokio::test]
/// Verifies that matching malformed inputs are not hidden by pushdown.
async fn matching_malformed_inputs_are_not_hidden_by_pushdown() {
    for filter in [select_filters(&[1, 2, 3, 9]), select_filters(&[1, 2])] {
        let source = filter_source().with_filter(filter);
        let store = FakeStore::default();
        let mut parser = Select::new(vec![1, 2], "a");
        parser.malformed = Some(2);
        let result = Pipeline::builder()
            .datasource(source.clone())
            .block_source(source)
            .store(store.clone())
            .from_block(1)
            .parser(parser, (Record,))
            .build()
            .unwrap()
            .run()
            .await;
        assert!(matches!(result, Err(RavenError::Parser(_))));
        assert!(store.block_ptr().is_none());
        assert!(store.entity("Trace", "main").is_none());
    }
}

#[tokio::test]
/// Verifies that restarted pipeline rolls back a fork without replacing source filters.
async fn restarted_pipeline_rolls_back_a_fork_without_replacing_source_filters() {
    let source = filter_source().with_filter(select_filters(&[1, 3]));
    let store = FakeStore::default();
    let mut original = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source.clone())
        .store(store.clone())
        .from_block(1)
        .parser(Select::new(vec![1], "a"), (Record,))
        .build()
        .unwrap();
    original.run().await.unwrap();
    drop(original);
    source.set_chain(vec![
        chain_block(0, 10, 0, vec![]),
        chain_block(1, 11, 10, vec![9, 1, 2, 3, 1]),
        chain_block(2, 22, 11, vec![9]),
        chain_block(3, 23, 22, vec![9, 3, 1]),
        chain_block(4, 24, 23, vec![9]),
    ]);
    let mut restarted = Pipeline::builder()
        .datasource(source.clone())
        .block_source(source.clone())
        .store(store.clone())
        .from_block(999)
        .parser(Select::new(vec![1], "a"), (Record,))
        .build()
        .unwrap();
    restarted.run().await.unwrap();
    assert_eq!(store.block_ptr().unwrap().hash, hash(24));
    assert_eq!(
        &store.operations()[3..],
        &[
            (false, hash(13)),
            (false, hash(12)),
            (true, hash(22)),
            (true, hash(23)),
            (true, hash(24)),
        ]
    );
    let batch = source.block_by_hash(&hash(23)).await.unwrap().unwrap();
    assert_eq!(
        batch
            .updates
            .iter()
            .filter_map(log_value)
            .collect::<Vec<_>>(),
        vec![3, 1]
    );
    assert!(
        source
            .block_by_hash(&hash(24))
            .await
            .unwrap()
            .unwrap()
            .updates
            .is_empty()
    );
    let clean = FakeStore::default();
    Pipeline::builder()
        .datasource(source.clone())
        .block_source(source)
        .store(clean.clone())
        .from_block(1)
        .parser(Select::new(vec![1], "a"), (Record,))
        .build()
        .unwrap()
        .run()
        .await
        .unwrap();
    assert_eq!(store.entity("Trace", "main"), clean.entity("Trace", "main"));
    assert_eq!(
        store.entity_block_hash("Trace", "main"),
        clean.entity_block_hash("Trace", "main")
    );
}
