use std::collections::BTreeMap;

use bigdecimal::BigDecimal;
use serde_json::{Map, Value, json};

use crate::{
    Result,
    kit::Testkit,
    require,
    values::{hex_bigint, hex_u64, normalize_decimal_fields, text},
};

type EntityKey = (String, String);

/// Creates a missing-row error for an expected ERC20 record.
fn missing(what: &str) -> std::io::Error {
    std::io::Error::other(what.to_owned())
}

/// Canonicalize application-view values before comparing them with SQL values.
/// PostgreSQL is allowed to choose a different decimal scale, but never a
/// different exact numeric value.
/// Loads entity rows normalized for exact JSON comparison.
pub(crate) fn comparable_entities(
    rows: Vec<(String, String, Value)>,
) -> BTreeMap<EntityKey, Value> {
    rows.into_iter()
        .map(|(kind, id, mut data)| {
            normalize_decimal_fields(&mut data);
            ((kind, id), data)
        })
        .collect()
}

impl Testkit {
    /// Loads relational ERC20 state at a specific indexed height.
    pub(crate) fn relational_entities(
        &self,
        schema: &str,
        number: u64,
    ) -> Result<BTreeMap<EntityKey, Value>> {
        let fields: [(&str, &str, bool, &[(&str, &str)], &[(&str, &str)]); 5] = [
            (
                "Account",
                "account",
                false,
                &[("id", "id"), ("asERC20", "as_erc20")],
                &[],
            ),
            (
                "ERC20Contract",
                "erc20_contract",
                true,
                &[("id", "id"), ("asAccount", "as_account")],
                &[
                    ("name", "name"),
                    ("symbol", "symbol"),
                    ("decimals", "decimals"),
                    ("totalSupply", "total_supply"),
                ],
            ),
            (
                "ERC20Balance",
                "erc20_balance",
                false,
                &[("contract", "contract"), ("account", "account")],
                &[
                    ("id", "id"),
                    ("value", "value::text"),
                    ("valueExact", "value_exact::text"),
                ],
            ),
            (
                "ERC20Transfer",
                "erc20_transfer",
                true,
                &[
                    ("emitter", "emitter"),
                    ("contract", "contract"),
                    ("from", r#""from""#),
                    ("to", r#""to""#),
                ],
                &[
                    ("id", "id"),
                    ("transaction", r#""transaction""#),
                    ("timestamp", "timestamp::text"),
                    ("fromBalance", "from_balance"),
                    ("toBalance", "to_balance"),
                    ("value", "value::text"),
                    ("valueExact", "value_exact::text"),
                ],
            ),
            (
                "Transaction",
                "transaction",
                true,
                &[],
                &[
                    ("id", "id"),
                    ("timestamp", "timestamp::text"),
                    ("blockNumber", "block_number::text"),
                ],
            ),
        ];

        let mut rows = Vec::new();
        for (kind, table, immutable, hex_fields, scalars) in fields {
            let arguments = hex_fields
                .iter()
                .chain(scalars.iter())
                .map(|(key, expression)| format!("'{key}', {expression}"))
                .collect::<Vec<_>>()
                .join(", ");
            let condition = if immutable {
                format!("block_number <= {number}")
            } else {
                format!("block_range @> {number}::bigint")
            };
            let values: Vec<Value> = serde_json::from_str(&self.sql(&format!(
                r#"SELECT COALESCE(jsonb_agg(jsonb_build_object({arguments}) ORDER BY id), '[]'::jsonb) FROM {schema}."{table}" WHERE {condition};"#
            ))?)?;
            for value in values {
                let id = text(
                    value
                        .get("id")
                        .ok_or_else(|| missing("relational entity has no id"))?,
                )?;
                rows.push((kind.to_owned(), id, value));
            }
        }
        Ok(comparable_entities(rows))
    }

    /// Returns normalized relational version rows for rollback assertions.
    pub(crate) fn relational_versions(&self, schema: &str) -> Result<Value> {
        let mut result = Map::new();
        for table in [
            "account",
            "erc20_contract",
            "erc20_balance",
            "erc20_transfer",
            "transaction",
        ] {
            let version = if matches!(table, "account" | "erc20_balance") {
                "block_range"
            } else {
                "block_number"
            };
            let mut rows: Value = serde_json::from_str(&self.sql(&format!(
                r#"SELECT COALESCE(jsonb_agg(to_jsonb(t) - 'vid' ORDER BY id, "{version}"), '[]'::jsonb) FROM {schema}."{table}" t;"#
            ))?)?;
            normalize_decimal_fields(&mut rows);
            result.insert(table.to_owned(), rows);
        }
        Ok(Value::Object(result))
    }

    /// Verifies address and hash columns use normalized hexadecimal text.
    pub(crate) fn verify_hex_text(&self, schema: &str, tables: &[&str]) -> Result<()> {
        require(
            self.sql(&format!(
                "SELECT count(*) FROM information_schema.columns WHERE table_schema = '{schema}' \
                 AND data_type = 'bytea';"
            ))? == "0",
            "example still contains binary bytea columns",
        )?;
        for table in tables {
            require(
                self.sql(&format!(
                    r#"SELECT count(*) FROM {schema}."{table}" t CROSS JOIN LATERAL jsonb_each_text(to_jsonb(t)) field WHERE field.value ~ '^0x[0-9a-fA-F]+(:[0-9]+)?$' AND field.value <> lower(field.value);"#
                ))? == "0",
                &format!("{table} contains mixed-case hexadecimal data"),
            )?;
        }
        Ok(())
    }

    /// Compares current relational state with the normalized entity view.
    pub(crate) fn verify_relational_current(
        &self,
        schema: &str,
    ) -> Result<(u64, BTreeMap<EntityKey, Value>)> {
        let (height, _) = self.head()?;
        let rows = self.entities(schema)?;
        let mut expected = Vec::new();
        for row in rows {
            let values = row
                .as_array()
                .ok_or_else(|| missing("application entity row is not an array"))?;
            expected.push((
                text(
                    values
                        .first()
                        .ok_or_else(|| missing("application entity has no kind"))?,
                )?,
                text(
                    values
                        .get(1)
                        .ok_or_else(|| missing("application entity has no id"))?,
                )?,
                values
                    .get(2)
                    .ok_or_else(|| missing("application entity has no data"))?
                    .clone(),
            ));
        }
        let expected = comparable_entities(expected);
        require(
            self.relational_entities(schema, height)? == expected,
            "native ERC20 read mapping differs from historical SQL rows",
        )?;
        Ok((height, expected))
    }

    /// Verifies indexed ERC20 balances and total supply against expectations.
    pub(crate) fn balances(
        &self,
        schema: &str,
        expected: &[(String, i64)],
        supply: i64,
    ) -> Result<Vec<Value>> {
        let rows = self.entities(schema)?;
        let mut balances = BTreeMap::<Option<String>, Value>::new();
        for row in &rows {
            let values = row
                .as_array()
                .ok_or_else(|| missing("application entity row is not an array"))?;
            if text(
                values
                    .first()
                    .ok_or_else(|| missing("application entity has no kind"))?,
            )? == "ERC20Balance"
            {
                let data = values
                    .get(2)
                    .ok_or_else(|| missing("ERC20 balance has no data"))?
                    .clone();
                let account = match data.get("account") {
                    Some(Value::Null) | None => None,
                    Some(value) => Some(text(value)?.to_lowercase()),
                };
                balances.insert(account, data);
            }
        }
        let expected_accounts = expected
            .iter()
            .map(|(address, _)| address.to_lowercase())
            .collect::<std::collections::BTreeSet<_>>();
        let actual_accounts = balances
            .keys()
            .filter_map(Clone::clone)
            .collect::<std::collections::BTreeSet<_>>();
        require(
            actual_accounts == expected_accounts && balances.contains_key(&None),
            "unexpected indexed ERC20 accounts",
        )?;
        for (address, amount) in expected {
            let balance = balances
                .get(&Some(address.to_lowercase()))
                .ok_or_else(|| missing("expected ERC20 account balance is absent"))?;
            let exact = balance
                .get("valueExact")
                .ok_or_else(|| missing("balance lacks valueExact"))?;
            require(
                text(exact)? == amount.to_string(),
                "ERC20 balance differs from fixture history",
            )?;
            require(
                text(
                    balance
                        .get("value")
                        .ok_or_else(|| missing("balance lacks value"))?,
                )?
                .parse::<BigDecimal>()?
                    == BigDecimal::new((*amount).into(), 18),
                "ERC20 decimal balance differs from fixture history",
            )?;
        }
        let total = balances
            .get(&None)
            .ok_or_else(|| missing("ERC20 total supply is absent"))?;
        let total_exact = total
            .get("valueExact")
            .ok_or_else(|| missing("supply lacks valueExact"))?;
        require(
            text(total_exact)? == supply.to_string(),
            "ERC20 supply differs from mint/burn history",
        )?;
        require(
            text(
                total
                    .get("value")
                    .ok_or_else(|| missing("supply lacks value"))?,
            )?
            .parse::<BigDecimal>()?
                == BigDecimal::new(supply.into(), 18),
            "ERC20 decimal supply differs from fixture history",
        )?;

        let contract = rows
            .iter()
            .find_map(|row| {
                let values = row.as_array()?;
                if text(values.first()?).ok()? == "ERC20Contract" {
                    values.get(2).cloned()
                } else {
                    None
                }
            })
            .ok_or_else(|| missing("ERC20 contract entity is absent"))?;
        require(
            (
                text(
                    contract
                        .get("name")
                        .ok_or_else(|| missing("contract lacks name"))?,
                )?,
                text(
                    contract
                        .get("symbol")
                        .ok_or_else(|| missing("contract lacks symbol"))?,
                )?,
                contract
                    .get("decimals")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| missing("contract decimals is not an unsigned JSON number"))?,
            ) == ("Raven Test Token".to_owned(), "RAVEN".to_owned(), 18),
            "historical token metadata is incorrect",
        )?;
        let token = text(
            contract
                .get("id")
                .ok_or_else(|| missing("contract lacks id"))?,
        )?;
        require(
            text(
                contract
                    .get("asAccount")
                    .ok_or_else(|| missing("contract lacks asAccount"))?,
            )? == token
                && text(
                    contract
                        .get("totalSupply")
                        .ok_or_else(|| missing("contract lacks totalSupply"))?,
                )? == format!("{token}/totalSupply"),
            "ERC20 contract references differ from the OpenZeppelin schema",
        )?;
        for (account, balance) in &balances {
            require(
                text(
                    balance
                        .get("id")
                        .ok_or_else(|| missing("balance lacks id"))?,
                )? == format!("{token}/{}", account.as_deref().unwrap_or("totalSupply"))
                    && text(
                        balance
                            .get("contract")
                            .ok_or_else(|| missing("balance lacks contract"))?,
                    )? == token,
                "ERC20 balance references or IDs are incorrect",
            )?;
        }
        let mut accounts = BTreeMap::<String, Value>::new();
        for row in &rows {
            let values = row
                .as_array()
                .ok_or_else(|| missing("application entity row is not an array"))?;
            if text(
                values
                    .first()
                    .ok_or_else(|| missing("application entity has no kind"))?,
            )? == "Account"
            {
                accounts.insert(
                    text(values.get(1).ok_or_else(|| missing("Account has no id"))?)?,
                    values
                        .get(2)
                        .ok_or_else(|| missing("Account has no data"))?
                        .clone(),
                );
            }
        }
        let mut expected_ids = expected_accounts;
        expected_ids.insert(token.clone());
        require(
            accounts
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
                == expected_ids,
            "ERC20 account entities differ from the indexed Transfer endpoints",
        )?;
        for (id, account) in accounts {
            require(
                account
                    == json!({"id": id, "asERC20": if id == token { Some(token.clone()) } else { None }}),
                "ERC20 account backlink is incorrect",
            )?;
        }
        Ok(rows)
    }

    /// Verifies transfer history retains canonical receipt amounts and ordering.
    pub(crate) fn transfer_history(&self, schema: &str, token: &str) -> Result<()> {
        let mut expected_transfers = BTreeMap::<String, Value>::new();
        let mut expected_transactions = BTreeMap::<String, Value>::new();
        let zero = format!("0x{}", "0".repeat(40));
        for receipt in &self.receipts {
            for log in receipt
                .get("logs")
                .and_then(Value::as_array)
                .ok_or_else(|| missing("receipt lacks logs"))?
            {
                if text(
                    log.get("address")
                        .ok_or_else(|| missing("log lacks address"))?,
                )?
                .to_lowercase()
                    != token
                {
                    continue;
                }
                let block_number = log
                    .get("blockNumber")
                    .ok_or_else(|| missing("log lacks block number"))?;
                let block = self.rpc("eth_getBlockByNumber", json!([block_number, false]))?;
                if block.is_null() || block.get("hash") != log.get("blockHash") {
                    continue;
                }
                let topics = log
                    .get("topics")
                    .and_then(Value::as_array)
                    .ok_or_else(|| missing("log lacks topics"))?;
                let data = text(log.get("data").ok_or_else(|| missing("log lacks data"))?)?;
                require(
                    topics.len() == 3 && data.len() == 66,
                    "fixture emitted an unexpected token event",
                )?;
                let endpoint = |topic: &Value| -> Result<Option<String>> {
                    let topic = text(topic)?;
                    require(topic.len() >= 40, "Transfer topic is too short")?;
                    let address = format!("0x{}", &topic[topic.len() - 40..]).to_lowercase();
                    Ok((address != zero).then_some(address))
                };
                let source = endpoint(&topics[1])?;
                let target = endpoint(&topics[2])?;
                let number = hex_u64(&text(block_number)?)?.to_string();
                let event_id = format!(
                    "{number}-{}",
                    hex_u64(&text(
                        log.get("logIndex")
                            .ok_or_else(|| missing("log lacks log index"))?,
                    )?)?
                );
                let amount = hex_bigint(&data)?;
                let tx = text(
                    log.get("transactionHash")
                        .ok_or_else(|| missing("log lacks transaction hash"))?,
                )?
                .to_lowercase();
                let timestamp = hex_u64(&text(
                    block
                        .get("timestamp")
                        .ok_or_else(|| missing("block lacks timestamp"))?,
                )?)?
                .to_string();
                let value = BigDecimal::new(amount.clone(), 18);
                expected_transfers.insert(event_id.clone(), json!({
                    "id": event_id, "emitter": token, "contract": token, "transaction": tx.clone(),
                    "timestamp": timestamp.clone(), "from": source.clone(), "to": target.clone(),
                    "fromBalance": source.as_ref().map(|address| format!("{token}/{address}")),
                    "toBalance": target.as_ref().map(|address| format!("{token}/{address}")),
                    "valueExact": amount.to_string(), "value": value.to_plain_string(),
                }));
                expected_transactions.insert(
                    tx.clone(),
                    json!({
                        "id": tx, "timestamp": timestamp, "blockNumber": number,
                    }),
                );
            }
        }
        let mut actual_transfers = BTreeMap::new();
        let mut actual_transactions = BTreeMap::new();
        for row in self.entities(schema)? {
            let values = row
                .as_array()
                .ok_or_else(|| missing("application entity row is not an array"))?;
            let kind = text(
                values
                    .first()
                    .ok_or_else(|| missing("application entity has no kind"))?,
            )?;
            let id = text(
                values
                    .get(1)
                    .ok_or_else(|| missing("application entity has no id"))?,
            )?;
            let mut data = values
                .get(2)
                .ok_or_else(|| missing("application entity has no data"))?
                .clone();
            normalize_decimal_fields(&mut data);
            match kind.as_str() {
                "ERC20Transfer" => {
                    actual_transfers.insert(id, data);
                }
                "Transaction" => {
                    actual_transactions.insert(id, data);
                }
                _ => {}
            }
        }
        for value in expected_transfers.values_mut() {
            normalize_decimal_fields(value);
        }
        require(
            actual_transfers == expected_transfers,
            "Transfer fields or IDs differ from canonical receipt logs",
        )?;
        require(
            actual_transactions == expected_transactions,
            "Transaction fields or IDs differ from canonical receipt logs",
        )?;
        require(
            !expected_transfers.is_empty(),
            "fixture produced no canonical Transfer logs",
        )
    }

    /// Captures indexed state and version rows for a rollback report stage.
    pub(crate) fn show_rollback(&mut self, schema: &str, stage: &str) -> Result<()> {
        let mut balances = BTreeMap::<Option<String>, String>::new();
        for row in self.entities(schema)? {
            let values = row
                .as_array()
                .ok_or_else(|| missing("application entity row is not an array"))?;
            if text(
                values
                    .first()
                    .ok_or_else(|| missing("application entity has no kind"))?,
            )? == "ERC20Balance"
            {
                let data = values
                    .get(2)
                    .ok_or_else(|| missing("ERC20 balance has no data"))?;
                balances.insert(
                    match data.get("account") {
                        Some(Value::Null) | None => None,
                        Some(value) => Some(text(value)?.to_lowercase()),
                    },
                    text(
                        data.get("valueExact")
                            .ok_or_else(|| missing("ERC20 balance lacks valueExact"))?,
                    )?,
                );
            }
        }
        let block: Value = serde_json::from_str(&self.sql(&format!(
            "SELECT json_build_array(latest_block_number, latest_block_hash) FROM {schema}.networks;"
        ))?)?;
        let state = json!({
            "stage": stage,
            "block": block,
            "account_0": balances.get(&Some(self.accounts[0].to_lowercase())).cloned().unwrap_or_else(|| "absent".to_owned()),
            "account_1": balances.get(&Some(self.accounts[1].to_lowercase())).cloned().unwrap_or_else(|| "absent".to_owned()),
            "account_2": balances.get(&Some(self.accounts[2].to_lowercase())).cloned().unwrap_or_else(|| "absent".to_owned()),
            "total_supply": balances.get(&None).ok_or_else(|| missing("ERC20 total supply is absent"))?,
        });
        self.report
            .as_object_mut()
            .ok_or_else(|| missing("testkit report is not an object"))?
            .entry("stages")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| missing("testkit report stages is not an array"))?
            .push(state.clone());
        println!("{}", serde_json::to_string_pretty(&state)?);
        Ok(())
    }

    /// Runs the ERC20 fork rollback scenario against the local fixtures.
    pub(crate) fn erc20_rollback(&mut self) -> Result<()> {
        self.build_erc20()?;
        self.connect_database()?;
        self.start_anvil()?;
        let accounts = self.accounts.clone();
        let (token, start) = self.deploy(
            "erc20",
            "RavenTestToken",
            &[accounts[0].clone(), "1000".to_owned()],
            None,
        )?;
        let schema = self.schema("erc20_rollback");
        let clean_schema = self.schema("erc20_rollback_clean");
        self.report["schema"] = Value::String(schema.clone());
        self.report["clean_schema"] = Value::String(clean_schema.clone());
        self.report["token"] = Value::String(token.clone());
        let mut writer = self.index("token", &schema, &token, start)?;
        self.wait_synced(&[(schema.as_str(), writer)])?;
        self.balances(&schema, &[(accounts[0].clone(), 1000)], 1000)?;
        let (base_height, base_state) = self.verify_relational_current(&schema)?;
        self.show_rollback(&schema, "before_transfer")?;

        let snapshot = self.rpc("evm_snapshot", json!([]))?;
        let orphan_receipt = self.send(
            &token,
            "transfer(address,uint256)",
            &[accounts[1].clone(), "300".to_owned()],
            0,
        )?;
        let (orphan_height, orphan_hash) = self.wait_synced(&[(schema.as_str(), writer)])?;
        self.balances(
            &schema,
            &[(accounts[0].clone(), 700), (accounts[1].clone(), 300)],
            1000,
        )?;
        self.verify_relational_current(&schema)?;
        require(
            self.sql(&format!(
                "SELECT count(*) FROM {schema}.erc20_balance WHERE lower(block_range) = {orphan_height};"
            ))?.parse::<u64>()? >= 2,
            "branch A native versions were not persisted",
        )?;
        self.show_rollback(&schema, "branch_a_indexed")?;
        self.stop_process(writer)?;

        require(
            self.rpc("evm_revert", json!([snapshot]))? == Value::Bool(true),
            "owned Anvil snapshot could not be reverted",
        )?;
        self.send(
            &token,
            "transfer(address,uint256)",
            &[accounts[2].clone(), "120".to_owned()],
            0,
        )?;
        let (height, canonical_hash) = self.head()?;
        require(
            height == orphan_height && canonical_hash != orphan_hash,
            "fixture did not create a same-height fork",
        )?;

        writer = self.index("token", &schema, &token, start)?;
        self.wait_synced(&[(schema.as_str(), writer)])?;
        let recovered = self.balances(
            &schema,
            &[(accounts[0].clone(), 880), (accounts[2].clone(), 120)],
            1000,
        )?;
        self.verify_relational_current(&schema)?;
        self.transfer_history(&schema, &token)?;
        require(
            self.relational_entities(&schema, base_height)? == base_state,
            "rollback changed the historical deployment state",
        )?;
        require(
            self.sql(&format!(
                "SELECT count(*) FROM {schema}.blocks WHERE status = 0;"
            ))? == "1",
            "branch A was not marked orphaned",
        )?;
        require(
            self.sql(&format!(
                "SELECT count(*) FROM {schema}.erc20_balance WHERE account = '{}';",
                accounts[1].to_lowercase()
            ))? == "0",
            "branch A balance versions remain",
        )?;
        require(self.sql(&format!("SELECT to_regclass('{schema}.entities') IS NULL AND to_regclass('{schema}.entity_changes') IS NULL;"))? == "t",
                "JSON mirrors remain")?;
        let orphan_tx = text(
            orphan_receipt
                .get("transactionHash")
                .ok_or_else(|| missing("receipt lacks transaction hash"))?,
        )?
        .to_lowercase();
        require(
            self.sql(&format!(
                r#"SELECT count(*) FROM {schema}."transaction" WHERE id = '{orphan_tx}';"#
            ))? == "0",
            "branch A transaction remains",
        )?;
        self.show_rollback(&schema, "branch_b_after_automatic_rollback")?;
        self.check("restart automatically reverted branch A and indexed branch B");

        let clean = self.index("token", &clean_schema, &token, start)?;
        self.wait_synced(&[(clean_schema.as_str(), clean)])?;
        require(
            self.entities(&clean_schema)? == recovered,
            "rollback differs from clean replay",
        )?;
        require(
            self.relational_versions(&schema)? == self.relational_versions(&clean_schema)?,
            "rollback left orphaned native versions",
        )?;
        self.check("entities and all five relational tables match clean canonical replay");
        self.stop_process(clean)?;
        self.stop_process(writer)?;
        self.report["status"] = Value::String("passed".to_owned());
        self.report["canonical_block"] = Value::from(height);
        self.report["canonical_hash"] = Value::String(canonical_hash);
        self.report["orphan_hash"] = Value::String(orphan_hash);
        Ok(())
    }
}
