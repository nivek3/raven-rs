table! {
    blocks {
        id -> BigInt,
        title -> Text,
        body -> Text,
        draft -> Bool,
        visit_count -> Integer,
    }
}

#[derive(Queryable, Identifiable, AsChangeset)]
pub struct Block {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub draft: bool,
    pub visit_count: i32,
}
