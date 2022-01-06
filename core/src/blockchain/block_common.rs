#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct BlockHash(pub Box<[u8]>);

impl BlockHash {
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for BlockHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "0x({})", hex::encode(self.as_slice()))
    }
}

impl fmt::Debug for BlockHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "0x{}", hex::encode(&self.0))
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct BlockPtr {
    pub hash: BlockHash,
    pub number: BlockNumber,
}

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
