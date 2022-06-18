table! {
    ethereum_blocks (hash) {
        hash -> Varchar,
        number -> Int8,
        parent_hash -> Varchar,
        network_name -> Varchar,
        data -> Jsonb,
    }
}

table! {
    ethereum_networks (name) {
        name -> Varchar,
        head_block_hash -> Nullable<Varchar>,
        head_block_number -> Nullable<Int8>,
    }
}

joinable!(ethereum_blocks -> ethereum_networks (network_name));

allow_tables_to_appear_in_same_query!(
    ethereum_blocks,
    ethereum_networks,
);
