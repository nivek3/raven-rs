use diesel::pg::PgConnection;
use diesel::{
    r2d2::{self, event as e, Builder, ConnectionManager, HandleEvent, Pool, PooledConnection},
    Connection,
};
use diesel::{sql_query, RunQueryDsl};

use graph::{
    constraint_violation,
    prelude::{
        anyhow::{self},
        error, info, o, tokio,
        tokio::sync::Semaphore,
        CancelGuard, CancelHandle, CancelToken as _, CancelableError, Logger, StoreError,
    },
    util::timed_rw_lock::TimedMutex,
};

use std::collections::HashMap;
use std::fmt::{self, Write};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

lazy_static::lazy_static! {
    // There is typically no need to configure this. But this can be used to effectivey disable the
    // query semaphore by setting it to a high number.
    static ref EXTRA_QUERY_PERMITS: usize = {
        std::env::var("GRAPH_EXTRA_QUERY_PERMITS")
            .ok()
            .map(|s| {
                usize::from_str(&s).unwrap_or_else(|_| {
                    panic!("GRAPH_EXTRA_QUERY_PERMITS must be a number, but is `{}`", s)
                })
            })
            .unwrap_or(0)
    };

    // These environment variables should really be set through the
    // configuration file; especially for min_idle and idle_timeout, it's
    // likely that they should be configured differently for each pool

    static ref CONNECTION_TIMEOUT: Duration = {
        std::env::var("GRAPH_STORE_CONNECTION_TIMEOUT").ok().map(|s| Duration::from_millis(u64::from_str(&s).unwrap_or_else(|_| {
            panic!("GRAPH_STORE_CONNECTION_TIMEOUT must be a positive number, but is `{}`", s)
        }))).unwrap_or(Duration::from_secs(5))
    };
    static ref MIN_IDLE: Option<u32> = {
        std::env::var("GRAPH_STORE_CONNECTION_MIN_IDLE").ok().map(|s| u32::from_str(&s).unwrap_or_else(|_| {
           panic!("GRAPH_STORE_CONNECTION_MIN_IDLE must be a positive number but is `{}`", s)
        }))
    };
    static ref IDLE_TIMEOUT: Duration = {
        std::env::var("GRAPH_STORE_CONNECTION_IDLE_TIMEOUT").ok().map(|s| Duration::from_secs(u64::from_str(&s).unwrap_or_else(|_| {
            panic!("GRAPH_STORE_CONNECTION_IDLE_TIMEOUT must be a positive number, but is `{}`", s)
        }))).unwrap_or(Duration::from_secs(600))
    };
    // A fallback in case the logic to remember database availability goes
    // wrong; when this is set, we always try to get a connection and never
    // use the availability state we remembered
    static ref TRY_ALWAYS: bool = {
        std::env::var("GRAPH_STORE_CONNECTION_TRY_ALWAYS").ok().map(|_| true).unwrap_or(false)
    };
}

/// How long to keep connections in the `fdw_pool` around before closing
/// them on idle. This is much shorter than the default of 10 minutes.
const FDW_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// A pool goes through several states, and this enum tracks what state we
/// are in, together with the `state_tracker` field on `ConnectionPool`.
/// When first created, the pool is in state `Created`; once we successfully
/// called `setup` on it, it moves to state `Ready`. During use, we use the
/// r2d2 callbacks to determine if the database is available or not, and set
/// the `available` field accordingly. Tracking that allows us to fail fast
/// and avoids having to wait for a connection timeout every time we need a
/// database connection. That avoids overall undesirable states like buildup
/// of queries; instead of queueing them until the database is available,
/// they return almost immediately with an error
enum PoolState {
    /// A connection pool, and all the servers for which we need to
    /// establish fdw mappings when we call `setup` on the pool
    Created(Arc<PoolInner>),
    /// The pool has been successfully set up
    Ready(Arc<PoolInner>),
}

#[derive(Clone)]
pub struct ConnectionPool {
    inner: Arc<TimedMutex<PoolState>>,
    logger: Logger,
    state_tracker: PoolStateTracker,
}

impl fmt::Debug for ConnectionPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionPool")
            .field("shard", &self)
            .finish()
    }
}

/// The name of the pool, mostly for logging, and what purpose it serves.
/// The main pool will always be called `main`, and can be used for reading
/// and writing. Replica pools can only be used for reading, and don't
/// require any setup (migrations etc.)
pub enum PoolName {
    Main,
    Replica(String),
}

impl PoolName {
    fn as_str(&self) -> &str {
        match self {
            PoolName::Main => "main",
            PoolName::Replica(name) => name,
        }
    }

    fn is_replica(&self) -> bool {
        match self {
            PoolName::Main => false,
            PoolName::Replica(_) => true,
        }
    }
}

#[derive(Clone)]
struct PoolStateTracker {
    available: Arc<AtomicBool>,
}

impl PoolStateTracker {
    fn new() -> Self {
        Self {
            available: Arc::new(AtomicBool::new(true)),
        }
    }

    fn mark_available(&self) {
        self.available.store(true, Ordering::Relaxed);
    }

    fn mark_unavailable(&self) {
        self.available.store(false, Ordering::Relaxed);
    }

    fn is_available(&self) -> bool {
        self.available.load(Ordering::Relaxed)
    }
}

impl ConnectionPool {
    pub fn create(
        shard_name: &str,
        pool_name: PoolName,
        postgres_url: String,
        pool_size: u32,
        fdw_pool_size: Option<u32>,
        logger: &Logger,
    ) -> ConnectionPool {
        let state_tracker = PoolStateTracker::new();
        let pool = PoolInner::create(
            shard_name,
            pool_name.as_str(),
            postgres_url,
            pool_size,
            fdw_pool_size,
            logger,
            state_tracker.clone(),
        );
        let pool_state = if pool_name.is_replica() {
            PoolState::Ready(Arc::new(pool))
        } else {
            PoolState::Created(Arc::new(pool))
        };
        ConnectionPool {
            inner: Arc::new(TimedMutex::new(pool_state, format!("pool-{}", shard_name))),
            logger: logger.clone(),
            state_tracker,
        }
    }

    /// This is only used for `graphman` to ensure it doesn't run migrations
    /// or other setup steps
    pub fn skip_setup(&self) {
        let mut guard = self.inner.lock(&self.logger);
        match &*guard {
            PoolState::Created(pool) => *guard = PoolState::Ready(pool.clone()),
            PoolState::Ready(_) => { /* nothing to do */ }
        }
    }

    /// Return a pool that is ready, i.e., connected to the database. If the
    /// pool has not been set up yet, call `setup`. If there are any errors
    /// or the pool is marked as unavailable, return
    /// `StoreError::DatabaseUnavailable`
    fn get_ready(&self) -> Result<Arc<PoolInner>, StoreError> {
        let mut guard = self.inner.lock(&self.logger);
        if !self.state_tracker.is_available() && !*TRY_ALWAYS {
            // We know that trying to use this pool is point since the
            // database is not available, and will only lead to other
            // operations having to wait until the connection timeout is
            // reached. `TRY_ALWAYS` allows users to force us to try
            // regardless.
            return Err(StoreError::DatabaseUnavailable);
        }

        match &*guard {
            PoolState::Created(pool) => {
                let pool2 = pool.clone();
                *guard = PoolState::Ready(pool.clone());
                self.state_tracker.mark_available();
                Ok(pool2)
            }
            PoolState::Ready(pool) => Ok(pool.clone()),
        }
    }

    /// Execute a closure with a connection to the database.
    ///
    /// # API
    ///   The API of using a closure to bound the usage of the connection serves several
    ///   purposes:
    ///
    ///   * Moves blocking database access out of the `Future::poll`. Within
    ///     `Future::poll` (which includes all `async` methods) it is illegal to
    ///     perform a blocking operation. This includes all accesses to the
    ///     database, acquiring of locks, etc. Calling a blocking operation can
    ///     cause problems with `Future` combinators (including but not limited
    ///     to select, timeout, and FuturesUnordered) and problems with
    ///     executors/runtimes. This method moves the database work onto another
    ///     thread in a way which does not block `Future::poll`.
    ///
    ///   * Limit the total number of connections. Because the supplied closure
    ///     takes a reference, we know the scope of the usage of all entity
    ///     connections and can limit their use in a non-blocking way.
    ///
    /// # Cancellation
    ///   The normal pattern for futures in Rust is drop to cancel. Once we
    ///   spawn the database work in a thread though, this expectation no longer
    ///   holds because the spawned task is the independent of this future. So,
    ///   this method provides a cancel token which indicates that the `Future`
    ///   has been dropped. This isn't *quite* as good as drop on cancel,
    ///   because a drop on cancel can do things like cancel http requests that
    ///   are in flight, but checking for cancel periodically is a significant
    ///   improvement.
    ///
    ///   The implementation of the supplied closure should check for cancel
    ///   between every operation that is potentially blocking. This includes
    ///   any method which may interact with the database. The check can be
    ///   conveniently written as `token.check_cancel()?;`. It is low overhead
    ///   to check for cancel, so when in doubt it is better to have too many
    ///   checks than too few.
    ///
    /// # Panics:
    ///   * This task will panic if the supplied closure panics
    ///   * This task will panic if the supplied closure returns Err(Cancelled)
    ///     when the supplied cancel token is not cancelled.
    pub(crate) async fn with_conn<T: Send + 'static>(
        &self,
        f: impl 'static
            + Send
            + FnOnce(
                &PooledConnection<ConnectionManager<PgConnection>>,
                &CancelHandle,
            ) -> Result<T, CancelableError<StoreError>>,
    ) -> Result<T, StoreError> {
        let pool = self.get_ready()?;
        pool.with_conn(f).await
    }

    pub fn get(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, StoreError> {
        self.get_ready()?.get()
    }

    /// Get a connection from the pool for foreign data wrapper access;
    /// since that pool can be very contended, periodically log that we are
    /// still waiting for a connection
    ///
    /// The `timeout` is called every time we time out waiting for a
    /// connection. If `timeout` returns `true`, `get_fdw` returns with that
    /// error, otherwise we try again to get a connection.
    pub fn get_fdw<F>(
        &self,
        logger: &Logger,
        timeout: F,
    ) -> Result<PooledConnection<ConnectionManager<PgConnection>>, StoreError>
    where
        F: FnMut() -> bool,
    {
        self.get_ready()?.get_fdw(logger, timeout)
    }

    /// Check that we can connect to the database
    pub fn check(&self) -> bool {
        true
    }

    /// Setup the database for this pool. This includes configuring foreign
    /// data wrappers for cross-shard communication, and running any pending
    /// schema migrations for this database.
    ///
    /// # Panics
    ///
    /// If any errors happen during the migration, the process panics
    pub async fn setup(&self) {
        let pool = self.clone();
        graph::spawn_blocking_allow_panic(move || {
            pool.get_ready().ok();
        })
        .await
        // propagate panics
        .unwrap();
    }
}

fn brief_error_msg(error: &dyn std::error::Error) -> String {
    // For 'Connection refused' errors, Postgres includes the IP and
    // port number in the error message. We want to suppress that and
    // only use the first line from the error message. For more detailed
    // analysis, 'Connection refused' manifests as a
    // `ConnectionError(BadConnection("could not connect to server:
    // Connection refused.."))`
    error
        .to_string()
        .split("\n")
        .next()
        .unwrap_or("no error details provided")
        .to_string()
}

#[derive(Clone)]
struct ErrorHandler {
    logger: Logger,
    state_tracker: PoolStateTracker,
}

impl ErrorHandler {
    fn new(logger: Logger, state_tracker: PoolStateTracker) -> Self {
        Self {
            logger,
            state_tracker,
        }
    }
}
impl std::fmt::Debug for ErrorHandler {
    fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Result::Ok(())
    }
}

impl r2d2::HandleError<r2d2::Error> for ErrorHandler {
    fn handle_error(&self, error: r2d2::Error) {
        let msg = brief_error_msg(&error);

        // Don't count canceling statements for timeouts etc. as a
        // connection error. Unfortunately, we only have the textual error
        // and need to infer whether the error indicates that the database
        // is down or if something else happened. When querying a replica,
        // these messages indicate that a query was canceled because it
        // conflicted with replication, but does not indicate that there is
        // a problem with the database itself.
        //
        // This check will break if users run Postgres (or even graph-node)
        // in a locale other than English. In that case, their database will
        // be marked as unavailable even though it is perfectly fine.
        if msg.contains("canceling statement")
            || msg.contains("no connection to the server")
            || msg.contains("terminating connection due to conflict with recovery")
        {
            return;
        }

        if self.state_tracker.is_available() {
            error!(self.logger, "Postgres connection error"; "error" => msg);
        }
        self.state_tracker.mark_unavailable();
    }
}

#[derive(Clone)]
struct EventHandler {
    logger: Logger,
    state_tracker: PoolStateTracker,
}

impl EventHandler {
    fn new(logger: Logger, state_tracker: PoolStateTracker) -> Self {
        EventHandler {
            logger,
            state_tracker,
        }
    }
}

impl std::fmt::Debug for EventHandler {
    fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Result::Ok(())
    }
}

impl HandleEvent for EventHandler {
    fn handle_acquire(&self, _: e::AcquireEvent) {
        self.state_tracker.mark_available();
    }

    fn handle_checkout(&self, event: e::CheckoutEvent) {
        self.state_tracker.mark_available();
    }

    fn handle_timeout(&self, event: e::TimeoutEvent) {
        if self.state_tracker.is_available() {
            error!(self.logger, "Connection checkout timed out";
               "wait_ms" => event.timeout().as_millis()
            )
        }
        self.state_tracker.mark_unavailable();
    }
}

#[derive(Clone)]
pub struct PoolInner {
    logger: Logger,
    pool: Pool<ConnectionManager<PgConnection>>,
    // A separate pool for connections that will use foreign data wrappers.
    // Once such a connection accesses a foreign table, Postgres keeps a
    // connection to the foreign server until the connection is closed.
    // Normal pooled connections live quite long (up to 10 minutes) and can
    // therefore keep a lot of connections into foreign databases open. We
    // mitigate this by using a separate small pool with a much shorter
    // connection lifetime. Starting with postgres_fdw 1.1 in Postgres 14,
    // this will no longer be needed since it will then be possible to
    // explicitly close connections to foreign servers when a connection is
    // returned to the pool.
    fdw_pool: Option<Pool<ConnectionManager<PgConnection>>>,
    limiter: Arc<Semaphore>,
    postgres_url: String,

    // Limits the number of graphql queries that may execute concurrently. Since one graphql query
    // may require multiple DB queries, it is useful to organize the queue at the graphql level so
    // that waiting queries consume few resources. Still this is placed here because the semaphore
    // is sized acording to the DB connection pool size.
    query_semaphore: Arc<tokio::sync::Semaphore>,
}

impl PoolInner {
    fn create(
        shard_name: &str,
        pool_name: &str,
        postgres_url: String,
        pool_size: u32,
        fdw_pool_size: Option<u32>,
        logger: &Logger,
        state_tracker: PoolStateTracker,
    ) -> PoolInner {
        let logger_store = logger.new(o!("component" => "Store"));
        let logger_pool = logger.new(o!("component" => "ConnectionPool"));
        let const_labels = {
            let mut map = HashMap::new();
            map.insert("pool".to_owned(), pool_name.to_owned());
            map.insert("shard".to_string(), shard_name.to_owned());
            map
        };

        let error_handler = Box::new(ErrorHandler::new(
            logger_pool.clone(),
            state_tracker.clone(),
        ));
        let event_handler = Box::new(EventHandler::new(logger_pool.clone(), state_tracker));

        // Connect to Postgres
        let conn_manager = ConnectionManager::new(postgres_url.clone());
        let builder: Builder<ConnectionManager<PgConnection>> = Pool::builder()
            .error_handler(error_handler.clone())
            .event_handler(event_handler.clone())
            .connection_timeout(*CONNECTION_TIMEOUT)
            .max_size(pool_size)
            .min_idle(*MIN_IDLE)
            .idle_timeout(Some(*IDLE_TIMEOUT));
        let pool = builder.build_unchecked(conn_manager);
        let fdw_pool = fdw_pool_size.map(|pool_size| {
            let conn_manager = ConnectionManager::new(postgres_url.clone());
            let builder: Builder<ConnectionManager<PgConnection>> = Pool::builder()
                .error_handler(error_handler)
                .event_handler(event_handler)
                .connection_timeout(*CONNECTION_TIMEOUT)
                .max_size(pool_size)
                .min_idle(Some(1))
                .idle_timeout(Some(FDW_IDLE_TIMEOUT));
            builder.build_unchecked(conn_manager)
        });

        let limiter = Arc::new(Semaphore::new(pool_size as usize));
        info!(logger_store, "Pool successfully connected to Postgres");

        let max_concurrent_queries = pool_size as usize + *EXTRA_QUERY_PERMITS;
        let query_semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent_queries));
        PoolInner {
            logger: logger_pool,
            postgres_url: postgres_url.clone(),
            pool,
            fdw_pool,
            limiter,
            query_semaphore,
        }
    }

    /// Execute a closure with a connection to the database.
    ///
    /// # API
    ///   The API of using a closure to bound the usage of the connection serves several
    ///   purposes:
    ///
    ///   * Moves blocking database access out of the `Future::poll`. Within
    ///     `Future::poll` (which includes all `async` methods) it is illegal to
    ///     perform a blocking operation. This includes all accesses to the
    ///     database, acquiring of locks, etc. Calling a blocking operation can
    ///     cause problems with `Future` combinators (including but not limited
    ///     to select, timeout, and FuturesUnordered) and problems with
    ///     executors/runtimes. This method moves the database work onto another
    ///     thread in a way which does not block `Future::poll`.
    ///
    ///   * Limit the total number of connections. Because the supplied closure
    ///     takes a reference, we know the scope of the usage of all entity
    ///     connections and can limit their use in a non-blocking way.
    ///
    /// # Cancellation
    ///   The normal pattern for futures in Rust is drop to cancel. Once we
    ///   spawn the database work in a thread though, this expectation no longer
    ///   holds because the spawned task is the independent of this future. So,
    ///   this method provides a cancel token which indicates that the `Future`
    ///   has been dropped. This isn't *quite* as good as drop on cancel,
    ///   because a drop on cancel can do things like cancel http requests that
    ///   are in flight, but checking for cancel periodically is a significant
    ///   improvement.
    ///
    ///   The implementation of the supplied closure should check for cancel
    ///   between every operation that is potentially blocking. This includes
    ///   any method which may interact with the database. The check can be
    ///   conveniently written as `token.check_cancel()?;`. It is low overhead
    ///   to check for cancel, so when in doubt it is better to have too many
    ///   checks than too few.
    ///
    /// # Panics:
    ///   * This task will panic if the supplied closure panics
    ///   * This task will panic if the supplied closure returns Err(Cancelled)
    ///     when the supplied cancel token is not cancelled.
    pub(crate) async fn with_conn<T: Send + 'static>(
        &self,
        f: impl 'static
            + Send
            + FnOnce(
                &PooledConnection<ConnectionManager<PgConnection>>,
                &CancelHandle,
            ) -> Result<T, CancelableError<StoreError>>,
    ) -> Result<T, StoreError> {
        let _permit = self.limiter.acquire().await;
        let pool = self.clone();

        let cancel_guard = CancelGuard::new();
        let cancel_handle = cancel_guard.handle();

        let result = graph::spawn_blocking_allow_panic(move || {
            // It is possible time has passed between scheduling on the
            // threadpool and being executed. Time to check for cancel.
            cancel_handle.check_cancel()?;

            // A failure to establish a connection is propagated as though the
            // closure failed.
            let conn = pool
                .get()
                .map_err(|_| CancelableError::Error(StoreError::DatabaseUnavailable))?;

            // It is possible time has passed while establishing a connection.
            // Time to check for cancel.
            cancel_handle.check_cancel()?;

            f(&conn, &cancel_handle)
        })
        .await
        .unwrap(); // Propagate panics, though there shouldn't be any.

        drop(cancel_guard);

        // Finding cancel isn't technically unreachable, since there is nothing
        // stopping the supplied closure from returning Canceled even if the
        // supplied handle wasn't canceled. That would be very unexpected, the
        // doc comment for this function says we will panic in this scenario.
        match result {
            Ok(t) => Ok(t),
            Err(CancelableError::Error(e)) => Err(e),
            Err(CancelableError::Cancel) => panic!("The closure supplied to with_entity_conn must not return Err(Canceled) unless the supplied token was canceled."),
        }
    }

    pub fn get(&self) -> Result<PooledConnection<ConnectionManager<PgConnection>>, StoreError> {
        self.pool.get().map_err(|_| StoreError::DatabaseUnavailable)
    }

    pub fn get_with_timeout_warning(
        &self,
        logger: &Logger,
    ) -> Result<PooledConnection<ConnectionManager<PgConnection>>, StoreError> {
        loop {
            match self.pool.get_timeout(*CONNECTION_TIMEOUT) {
                Ok(conn) => return Ok(conn),
                Err(e) => error!(logger, "Error checking out connection, retrying";
                   "error" => brief_error_msg(&e),
                ),
            }
        }
    }

    /// Get a connection from the pool for foreign data wrapper access;
    /// since that pool can be very contended, periodically log that we are
    /// still waiting for a connection
    ///
    /// The `timeout` is called every time we time out waiting for a
    /// connection. If `timeout` returns `true`, `get_fdw` returns with that
    /// error, otherwise we try again to get a connection.
    pub fn get_fdw<F>(
        &self,
        logger: &Logger,
        mut timeout: F,
    ) -> Result<PooledConnection<ConnectionManager<PgConnection>>, StoreError>
    where
        F: FnMut() -> bool,
    {
        let pool = match &self.fdw_pool {
            Some(pool) => pool,
            None => {
                const MSG: &str =
                    "internal error: trying to get fdw connection on a pool that doesn't have any";
                error!(logger, "{}", MSG);
                return Err(constraint_violation!(MSG));
            }
        };
        loop {
            match pool.get() {
                Ok(conn) => return Ok(conn),
                Err(e) => {
                    if timeout() {
                        return Err(e.into());
                    }
                }
            }
        }
    }

    /// Check that we can connect to the database
    pub fn check(&self) -> bool {
        self.pool
            .get()
            .ok()
            .map(|conn| sql_query("select 1").execute(&conn).is_ok())
            .unwrap_or(false)
    }
}
