use slog::{o, Logger};

/// Factory for creating component and subgraph loggers.
#[derive(Clone)]
pub struct LoggerFactory {
    parent: Logger,
}

impl LoggerFactory {
    /// Creates a new factory using a parent logger and optional Elasticsearch configuration.
    pub fn new(logger: Logger) -> Self {
        Self { parent: logger }
    }

    /// Creates a new factory with a new parent logger.
    pub fn with_parent(&self, parent: Logger) -> Self {
        Self { parent }
    }

    /// Creates a component-specific logger with optional Elasticsearch support.
    pub fn component_logger(&self, component: &str) -> Logger {
        return self.parent.new(o!("component" => component.to_string()));
    }
}
