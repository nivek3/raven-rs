table! {
    ethereum_blocks(hash) {
        hash -> Varchar,
        number -> BigInt,
        network_name -> Varchar,
        parent_hash -> Nullable<Varchar>,
        data -> Jsonb,
    }
}

table! {
    ethereum_networks (name) {
        name -> Varchar,
        namespace -> Varchar,
        head_block_hash -> Nullable<Varchar>,
        head_block_number -> Nullable<BigInt>,
        net_version -> Varchar,
        genesis_block_hash -> Varchar,
    }
}
