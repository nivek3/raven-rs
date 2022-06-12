table! {
    ethereum_blocks(hash) {
        id -> Int8,
        hash -> Varchar,
        number -> BigInt,
        parent_hash -> Nullable<Varchar>,
    }
}

#[derive(Queryable)]
pub struct Block {
    pub id: i64,
    pub number: i64,
    pub hash: String,
    pub parent_hash: String,
}
