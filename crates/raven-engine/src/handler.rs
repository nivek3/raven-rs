//! Deterministic handler execution over parser outputs.

use async_trait::async_trait;

use crate::{EntityStore, RavenResult};

/// Deterministic application work. Errors discard the entire block before its write transaction begins.
#[async_trait]
pub trait Handler<T: Send + Sync>: Send + Sync {
    /// Handles one parsed value using the current block-local entity state.
    async fn handle(&self, entities: &mut dyn EntityStore, value: &T) -> RavenResult<()>;
}

/// Ordered handlers for one parsed value. Tuples of one through eight handlers
/// are supported. A custom implementation must preserve sequential execution.
/// There is deliberately no empty-tuple implementation.
#[async_trait]
pub trait Handlers<T: Send + Sync>: Send + Sync {
    /// Invokes every handler in its declared order for one parsed value.
    async fn handle(&self, entities: &mut dyn EntityStore, value: &T) -> RavenResult<()>;
}

macro_rules! tuple_handlers {
    ($($handler:ident:$index:tt),+) => {
        #[async_trait]
        impl<T, $($handler),+> Handlers<T> for ($($handler,)+)
        where
            T: Send + Sync,
            $($handler: Handler<T>,)+
        {
            /// Invokes each tuple member in declaration order.
            async fn handle(&self, entities: &mut dyn EntityStore, value: &T) -> RavenResult<()> {
                $(self.$index.handle(entities, value).await?;)+
                Ok(())
            }
        }
    };
}

tuple_handlers!(A:0);
tuple_handlers!(A:0, B:1);
tuple_handlers!(A:0, B:1, C:2);
tuple_handlers!(A:0, B:1, C:2, D:3);
tuple_handlers!(A:0, B:1, C:2, D:3, E:4);
tuple_handlers!(A:0, B:1, C:2, D:3, E:4, F:5);
tuple_handlers!(A:0, B:1, C:2, D:3, E:4, F:5, G:6);
tuple_handlers!(A:0, B:1, C:2, D:3, E:4, F:5, G:6, H:7);
