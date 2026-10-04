mod fake;

use raven_engine::{BlockSource, CancellationToken, Datasource};

use crate::fake::{Batch, FakeSource, block};

#[tokio::test]
/// Verifies that stream matches random access and keeps empty blocks.
async fn stream_matches_random_access_and_keeps_empty_blocks() {
    let source = FakeSource::new(1);
    let empty = block(100, 1000, 990, vec![]);
    let event = block(101, 1010, 1000, vec![7, 8]);
    source.set_chain(vec![empty.clone(), event.clone()]);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
    source
        .consume(100, sender, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(source.chain_identity().await.unwrap(), 1);
    assert_eq!(source.head().await.unwrap(), event.header);
    for expected in [empty, event] {
        assert_eq!(receiver.recv().await, Some(expected.clone()));
        assert_eq!(
            source.block_by_hash(&expected.header.hash).await.unwrap(),
            Some(expected.clone())
        );
        assert_eq!(
            source
                .block_by_number(expected.header.number)
                .await
                .unwrap(),
            Some(expected)
        );
    }
    assert_eq!(receiver.recv().await, None);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    source
        .consume(101, sender, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(receiver.recv().await.unwrap().header.number, 101);
    assert_eq!(receiver.recv().await, None);
}

#[tokio::test]
/// Verifies that hash lookup retains orphans after same height branch switch.
async fn hash_lookup_retains_orphans_after_same_height_branch_switch() {
    let source = FakeSource::new(1);
    let old = block(100, 1000, 990, vec![1]);
    let replacement = block(100, 1001, 990, vec![]);
    source.set_chain(vec![old.clone()]);
    source.set_chain(vec![replacement.clone()]);
    assert_eq!(source.head().await.unwrap(), replacement.header);
    assert_eq!(
        source.block_by_number(100).await.unwrap(),
        Some(replacement)
    );
    assert_eq!(source.block_by_hash(&1000).await.unwrap(), Some(old));
    assert_eq!(source.block_by_hash(&9999).await.unwrap(), None);
    assert_eq!(source.block_by_number(101).await.unwrap(), None);
}

#[tokio::test]
/// Verifies that cancellation interrupts a full channel without needing receiver progress.
async fn cancellation_interrupts_a_full_channel_without_needing_receiver_progress() {
    use std::{
        future::Future,
        sync::Arc,
        task::{Context, Poll, Wake, Waker},
    };
    struct Noop;
    impl Wake for Noop {
        /// Intentionally ignores wake notifications for manual future polling.
        fn wake(self: Arc<Self>) {}
    }

    let source = FakeSource::new(1);
    source.set_chain(vec![block(100, 1000, 990, vec![])]);
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let sentinel = block(99, 990, 980, vec![]);
    sender.try_send(sentinel.clone()).unwrap();
    let token = CancellationToken::new();
    let mut consume = Box::pin(source.consume(100, sender, token.clone()));
    let waker = Waker::from(Arc::new(Noop));
    let mut context = Context::from_waker(&waker);
    assert!(matches!(consume.as_mut().poll(&mut context), Poll::Pending));
    token.cancel();
    assert!(matches!(
        consume.as_mut().poll(&mut context),
        Poll::Ready(Ok(()))
    ));
    drop(consume);
    assert_eq!(receiver.recv().await, Some(sentinel));
    assert_eq!(receiver.recv().await, None);
}

#[tokio::test]
/// Verifies that closed receiver is an error unless consumption was cancelled.
async fn closed_receiver_is_an_error_unless_consumption_was_cancelled() {
    let source = FakeSource::new(1);
    source.set_chain(vec![block(1, 1, 0, vec![])]);
    let (sender, receiver) = tokio::sync::mpsc::channel(1);
    drop(receiver);
    let error = source
        .consume(1, sender, CancellationToken::new())
        .await
        .unwrap_err();
    assert!(matches!(&error, raven_engine::RavenError::Source(_)));
    assert!(
        std::error::Error::source(&error)
            .unwrap()
            .is::<tokio::sync::mpsc::error::SendError<Batch>>()
    );
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    let token = CancellationToken::new();
    token.cancel();
    source.consume(1, sender, token).await.unwrap();
    assert_eq!(receiver.recv().await, None);
}

#[tokio::test]
/// Verifies that source contracts support trait objects with send futures.
async fn source_contracts_support_trait_objects_with_send_futures() {
    let source = FakeSource::new(1);
    let expected = block(1, 10, 0, vec![7]);
    source.set_chain(vec![expected.clone()]);
    let random: &dyn BlockSource<Hash = u64, Update = u64, ChainIdentity = u64> = &source;
    /// Returns its input while requiring the supplied future to implement `Send`.
    fn assert_send<T: Send>(value: T) -> T {
        value
    }
    assert_eq!(
        assert_send(random.block_by_hash(&10)).await.unwrap(),
        Some(expected.clone())
    );
    let sequential: &dyn Datasource<Hash = u64, Update = u64> = &source;
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    assert_send(sequential.consume(1, sender, CancellationToken::new()))
        .await
        .unwrap();
    assert_eq!(receiver.recv().await, Some(expected));
    assert_eq!(receiver.recv().await, None);
}
