//! Shutdown signal handling for local verification workflows.

use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::Result;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// Starts listening for SIGINT and SIGTERM to cancel local workflows.
pub(crate) fn install_signal_handlers() -> io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    tokio::spawn(async move {
        tokio::select! {
            _ = interrupt.recv() => {},
            _ = terminate.recv() => {},
        }
        INTERRUPTED.store(true, Ordering::Relaxed);
    });
    Ok(())
}

/// Returns an error when the workflow has received a cancellation signal.
pub(crate) fn check_cancelled() -> Result<()> {
    if INTERRUPTED.load(Ordering::Relaxed) {
        Err("testkit run interrupted".into())
    } else {
        Ok(())
    }
}
