use slog::{o, Drain, Logger};

pub mod factory;

pub fn logger() -> Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::CompactFormat::new(decorator).build();
    let drain = std::sync::Mutex::new(drain).fuse();

    slog::Logger::root(drain, o!())
}
