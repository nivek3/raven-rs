use anyhow::{anyhow, Context};
use std::convert::TryFrom;
use std::{fmt, str::FromStr};
use web3::types::{Block, H256};

use crate::components::store::BlockNumber;

#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct BlockHash(pub Box<[u8]>);

impl BlockHash {
    pub fn new(hash: &[u8]) -> Self {
        BlockHash(hash.to_vec().into_boxed_slice())
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    pub fn hash_hex(&self) -> String {
        hex::encode(self.as_slice())
    }
}

impl fmt::Display for BlockHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "0x{}", hex::encode(&self.0))
    }
}

impl fmt::Debug for BlockHash {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "0x{}", hex::encode(&self.0))
    }
}

impl From<H256> for BlockHash {
    fn from(hash: H256) -> Self {
        BlockHash(hash.as_bytes().into())
    }
}

impl From<Vec<u8>> for BlockHash {
    fn from(bytes: Vec<u8>) -> Self {
        BlockHash(bytes.as_slice().into())
    }
}

impl TryFrom<&str> for BlockHash {
    type Error = anyhow::Error;

    fn try_from(hash: &str) -> Result<Self, Self::Error> {
        let hash = hash.trim_start_matches("0x");
        let hash = hex::decode(hash)
            .with_context(|| format!("Cannot parse H256 value from string `{}`", hash))?;

        Ok(BlockHash(hash.as_slice().into()))
    }
}

/// A block hash and block number from a specific chain block.
///
/// Block numbers are signed 64 bit integers
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct BlockPtr {
    pub hash: BlockHash,
    pub number: BlockNumber,
}

impl BlockPtr {
    pub fn new(hash: BlockHash, number: BlockNumber) -> Self {
        Self { hash, number }
    }

    /// Encodes the block hash into a hexadecimal string **without** a "0x" prefix.
    /// Hashes are stored in the database in this format.
    pub fn hash_hex(&self) -> String {
        self.hash.hash_hex()
    }

    /// Block number to be passed into the store. Panics if it does not fit in an i32.
    pub fn block_number(&self) -> BlockNumber {
        self.number
    }

    pub fn hash_as_h256(&self) -> H256 {
        H256::from_slice(self.hash_slice())
    }

    pub fn hash_slice(&self) -> &[u8] {
        self.hash.0.as_ref()
    }
}

impl<T> From<Block<T>> for BlockPtr {
    fn from(b: Block<T>) -> BlockPtr {
        BlockPtr::from((b.hash.unwrap(), b.number.unwrap().as_u64()))
    }
}

impl<'a, T> From<&'a Block<T>> for BlockPtr {
    fn from(b: &'a Block<T>) -> BlockPtr {
        BlockPtr::from((b.hash.unwrap(), b.number.unwrap().as_u64()))
    }
}

impl From<(Vec<u8>, i32)> for BlockPtr {
    fn from((bytes, number): (Vec<u8>, i32)) -> Self {
        BlockPtr {
            hash: BlockHash::from(bytes),
            number,
        }
    }
}

impl From<(H256, i32)> for BlockPtr {
    fn from((hash, number): (H256, i32)) -> BlockPtr {
        BlockPtr {
            hash: hash.into(),
            number,
        }
    }
}

impl From<(H256, u64)> for BlockPtr {
    fn from((hash, number): (H256, u64)) -> Self {
        let number = i32::try_from(number).unwrap();
        BlockPtr::from((hash, number))
    }
}

impl From<(H256, i64)> for BlockPtr {
    fn from((hash, number): (H256, i64)) -> Self {
        if number < 0 {
            panic!("block number out of range: {}", number);
        }
        BlockPtr::from((hash, number as u64))
    }
}

impl TryFrom<(&str, i64)> for BlockPtr {
    type Error = anyhow::Error;

    fn try_from((hash, number): (&str, i64)) -> Result<Self, Self::Error> {
        let hash = hash.trim_start_matches("0x");
        let hash = H256::from_str(hash)
            .map_err(|e| anyhow!("Cannot parse H256 value from string `{}`: {}", hash, e))?;

        Ok(BlockPtr::from((hash, number)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// A collection of attributes that (kind of) uniquely identify a blockchain.
pub struct ChainIdentifier {
    pub net_version: String,
    pub genesis_block_hash: BlockHash,
}

#[cfg(test)]
mod tests {
    use super::BlockHash;

    #[test]
    fn block_hash() {
        let hash = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10,
        ];
        let block_hash = BlockHash::new(&hash);
        println!("block_hash {:?}", block_hash.to_string());
    }
}
