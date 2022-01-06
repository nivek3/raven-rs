pub mod block_common;

pub trait Block: Send + Sync {
    fn ptr(&self) -> BlockPtr;
    fn parent_ptr(&self) -> Option<BlockPtr>;

    fn number(&self) -> i32 {
        self.ptr().number
    }

    fn hash(&self) -> BlockHash {
        self.ptr().hash
    }

    fn parent_hash(&self) -> Option<BlockHash> {
        self.parent_ptr().map(|ptr| ptr.hash)
    }
}

pub struct BlockIngestor<B>
where
    B: Blockchain,
{
    adapter: Arc<B>,
    polling_interval: Duration,
}

#[async_trait]
pub trait Blockchain: Debug + Sized + Send + Sync + 'static {
    // The `Clone` bound is used when reprocessing a block, because `triggers_in_block` requires an
    // owned `Block`. It would be good to come up with a way to remove this bound.

    fn ingestor_adapter(&self) -> Arc<Self::IngestorAdapter>;

    fn chain_store(&self) -> Arc<dyn ChainStore>;

    async fn block_pointer_from_number(
        &self,
        logger: &Logger,
        number: BlockNumber,
    ) -> Result<BlockPtr, IngestorError>;

    fn runtime_adapter(&self) -> Arc<Self::RuntimeAdapter>;
}