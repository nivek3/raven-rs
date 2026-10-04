//! Full indexing acceptance: persisted history, restart, fork recovery and replay.
use serde_json::{Value, json};

use crate::{
    Result,
    kit::Testkit,
    require,
    values::{hex_u64, text},
};

impl Testkit {
    /// Runs the full local acceptance workflow for both example indexers.
    pub(crate) fn verify(&mut self) -> Result<()> {
        self.build_and_test()?;
        self.connect_database()?;
        self.start_anvil()?;
        let mut test_env = self.rust_env.clone();
        test_env.insert("RAVEN_TEST_DATABASE_URL".into(), self.database_url.clone());
        for (package, target, log) in [
            ("raven-postgres", Some("store"), "postgres-tests.log"),
            ("raven-postgres", Some("sql"), "application-sql-tests.log"),
            (
                "raven-example-uniswap-v3",
                None,
                "uniswap-storage-tests.log",
            ),
        ] {
            let mut args = vec!["cargo".into(), "test".into(), "-p".into(), package.into()];
            if let Some(target) = target {
                args.extend(["--test".into(), target.into()]);
            } else {
                args.push("--lib".into());
            }
            args.extend([
                "--locked".into(),
                "--offline".into(),
                "--target".into(),
                self.host.clone(),
                "--".into(),
                "--ignored".into(),
            ]);
            self.command(&args, Some(&test_env), Some(log))?;
            if target == Some("store") {
                self.check("PostgreSQL atomic commit, rollback, cancellation and writer-ownership tests pass");
            }
        }
        self.check(
            "application SQL transactions, read snapshots and Uniswap native-history tests pass",
        );
        let (token, token_start) = self.deploy(
            "erc20",
            "RavenTestToken",
            &[self.accounts[0].clone(), "1000".into()],
            None,
        )?;
        let (pool, pool_start) = self.setup_pool()?;
        let token_schema = self.schema("erc20");
        let pool_schema = self.schema("uniswap_v3");
        let clean_token_schema = self.schema("erc20_clean");
        let clean_pool_schema = self.schema("uniswap_v3_clean");
        self.report["schemas"] = json!({
            "erc20": token_schema.clone(),
            "uniswap_v3": pool_schema.clone(),
            "erc20_clean": clean_token_schema.clone(),
            "uniswap_v3_clean": clean_pool_schema.clone(),
        });
        let mut token_process = self.index("token", &token_schema, &token, token_start)?;
        let mut pool_process = self.index("pool", &pool_schema, &pool, pool_start)?;
        let mut targets = [
            (token_schema.as_str(), token_process),
            (pool_schema.as_str(), pool_process),
        ];
        self.wait_synced(&targets)?;
        self.balances(&token_schema, &[(self.accounts[0].clone(), 1000)], 1000)?;
        let (initial_height, initial_state) = self.verify_relational_current(&token_schema)?;
        for schema in [&token_schema, &pool_schema] {
            let network: Value = serde_json::from_str(&self.sql(&format!(
                "SELECT json_build_array(chain_id, network_name) FROM {schema}.networks;"
            ))?)?;
            require(
                network == json!([31337, "anvil"]),
                "network metadata is not stored in separate columns",
            )?;
            require(
                self.sql(&format!(
                "SELECT count(*) FROM information_schema.columns WHERE table_schema = '{schema}' \
                 AND table_name = 'networks' AND column_name = 'chain_identity';"))?
                    == "0",
                "legacy JSON identity column remains",
            )?;
            require(
                self.sql(&format!(
                "SELECT data_type FROM information_schema.columns WHERE table_schema = '{schema}' \
                 AND table_name = 'blocks' AND column_name = 'status';"))?
                    == "smallint",
                "block status is not SMALLINT",
            )?;
            require(
                self.sql(&format!(
                    "SELECT count(*) FROM {schema}.blocks WHERE status <> 1;"
                ))? == "0",
                "initial canonical blocks do not use status 1",
            )?;
            require(
                self.sql(&format!(
                    "SELECT to_regclass('{schema}.entities') IS NULL AND \
                 to_regclass('{schema}.entity_changes') IS NULL;"
                ))? == "t",
                "framework JSON state or journal still exists",
            )?;
            require(
                self.sql(&format!(
                "SELECT data_type FROM information_schema.columns WHERE table_schema = '{schema}' \
                 AND table_name = 'blocks' AND column_name = 'number';"))?
                    == "bigint",
                "block number is not BIGINT",
            )?;
            require(self.sql(&format!(
                "SELECT count(*) FROM pg_constraint c JOIN pg_namespace n ON n.oid = c.connamespace \
                 WHERE n.nspname = '{schema}' AND c.contype = 'c';"))? == "0",
                "explicit CHECK constraints remain")?;
        }
        self.check(
            "application tables are the only entity storage; no entities or entity_changes tables",
        );
        self.check("block numbers use BIGINT, status uses SMALLINT and schemas have no explicit CHECK constraints");
        self.check("chain ID and configured network name use separate columns");
        require(
            self.sql(&format!(
            "SELECT count(*) FROM information_schema.columns WHERE table_schema = '{token_schema}' \
             AND data_type = 'bytea';"))?
                == "0",
            "ERC20 layout still contains binary bytea columns",
        )?;
        require(self.sql(&format!(
            "SELECT data_type FROM information_schema.columns WHERE table_schema = '{token_schema}' \
             AND table_name = 'account' AND column_name = 'id';"))? == "text",
            "ERC20 account IDs are not hexadecimal text")?;
        require(self.sql(&format!(
            "SELECT count(*) FROM information_schema.columns WHERE table_schema = '{token_schema}' \
             AND table_name IN ('erc20_contract', 'erc20_transfer', 'transaction') AND column_name = 'block_number';"))? == "3",
            "immutable tables do not share the block_number naming")?;
        let columns: Value = serde_json::from_str(&self.sql(&format!(
            "SELECT json_agg(column_name ORDER BY column_name) FROM information_schema.columns \
             WHERE table_schema = '{token_schema}' AND table_name = 'transaction';"
        ))?)?;
        require(
            columns == json!(["block_number", "id", "timestamp", "vid"]),
            "Transaction contains a duplicate creation-block column",
        )?;
        self.check("deployment Transfer and historical metadata are indexed by the actual ERC20 executable");
        self.send(
            &token,
            "transfer(address,uint256)",
            &[self.accounts[1].clone(), "300".into()],
            0,
        )?;
        self.send(
            &token,
            "transfer(address,uint256)",
            &[self.accounts[2].clone(), "120".into()],
            1,
        )?;
        self.send(&token, "burn(uint256)", &["50".into()], 2)?;
        self.send(
            &token,
            "mint(address,uint256)",
            &[self.accounts[1].clone(), "25".into()],
            0,
        )?;
        self.send(
            &token,
            "transfer(address,uint256)",
            &[self.accounts[1].clone(), "10".into()],
            1,
        )?;
        self.send(
            &token,
            "transfer(address,uint256)",
            &[self.accounts[3].clone(), "0".into()],
            0,
        )?;
        self.pool_lifecycle()?;
        self.rpc("evm_mine", json!([]))?;
        self.wait_synced(&targets)?;
        let balances = vec![
            (self.accounts[0].clone(), 700),
            (self.accounts[1].clone(), 205),
            (self.accounts[2].clone(), 70),
            (self.accounts[3].clone(), 0),
        ];
        let token_rows = self.balances(&token_schema, &balances, 975)?;
        let (history_height, history_state) = self.verify_relational_current(&token_schema)?;
        require(
            self.relational_entities(&token_schema, initial_height)? == initial_state,
            "historical typed tables lost the deployment state",
        )?;
        self.pool_events(&pool_schema)?;
        self.verify_pool_contract_state(&pool_schema, &pool)?;
        let (pool_history_height, _) = self.head()?;
        let pool_history = self.pool_history(&pool_schema, pool_history_height)?;
        let latest = self.rpc("eth_getBlockByNumber", json!(["latest", false]))?;
        let next_day_hour = (hex_u64(&text(&latest["timestamp"])?)? / 86_400 + 1) * 86_400 + 3_600;
        self.rpc("evm_setNextBlockTimestamp", json!([next_day_hour]))?;
        let bucket_swap = self.pool_swap(Some(next_day_hour + 3_600))?;
        self.wait_synced(&targets)?;
        let bucket_block = self.rpc(
            "eth_getBlockByNumber",
            json!([bucket_swap["blockNumber"], false]),
        )?;
        self.verify_new_time_buckets(
            &pool_schema,
            &pool,
            hex_u64(&text(&bucket_block["timestamp"])?)?,
        )?;
        require(
            self.pool_history(&pool_schema, pool_history_height)? == pool_history,
            "later official Swap changed the state visible at an older block",
        )?;
        let pool_rows = self.pool_events(&pool_schema)?;
        self.check("official V3 Pool, Token, Tick and NFT state match contract calls, receipt amounts and 34-digit pricing");
        self.check(
            "official Swap creates new day/hour aggregates while old SQL history remains stable",
        );
        require(
            token_rows
                .iter()
                .filter(|row| row[0] == "ERC20Transfer")
                .count()
                == 7,
            "mint, burn, self-transfer or zero Transfer history was lost",
        )?;
        self.transfer_history(&token_schema, &token)?;
        self.check(
            "real Transfer, Swap and Mint receipts become PostgreSQL entities with exact values",
        );
        self.check(
            "empty blocks advance progress and multiple event kinds in one transaction are indexed",
        );
        for (_, process) in targets {
            self.stop_process(process)?;
        }
        let future_start = self.head()?.0 + 100;
        token_process = self.index("token", &token_schema, &token, future_start)?;
        pool_process = self.index("pool", &pool_schema, &pool, future_start)?;
        targets = [
            (token_schema.as_str(), token_process),
            (pool_schema.as_str(), pool_process),
        ];
        self.rpc("evm_mine", json!([]))?;
        self.wait_synced(&targets)?;
        require(
            self.entities(&token_schema)? == token_rows
                && self.entities(&pool_schema)? == pool_rows,
            "restart duplicated or changed indexed entities",
        )?;
        self.check("restart resumes persisted progress despite a changed configured start block");
        let snapshot = self.rpc("evm_snapshot", json!([]))?;
        self.send(
            &token,
            "mint(address,uint256)",
            &[self.accounts[1].clone(), "75".into()],
            0,
        )?;
        let orphan_pool = self.create_official_pool(500)?;
        self.pool_lifecycle()?;
        self.rpc("evm_mine", json!([]))?;
        let (orphan_height, orphan_hash) = self.wait_synced(&targets)?;
        let mut orphan_balances = balances.clone();
        orphan_balances[1].1 = 280;
        self.balances(&token_schema, &orphan_balances, 1050)?;
        self.verify_relational_current(&token_schema)?;
        require(
            self.pool_rows(&pool_schema)?
                .contains_key(&("Pool".into(), orphan_pool.clone())),
            "orphan branch did not index the official extra-fee pool",
        )?;
        self.pool_events(&pool_schema)?;
        require(
            self.rpc("evm_revert", json!([snapshot]))? == json!(true),
            "owned Anvil snapshot could not be reverted",
        )?;
        self.send(
            &token,
            "transfer(address,uint256)",
            &[self.accounts[2].clone(), "20".into()],
            1,
        )?;
        // Match the orphan branch's extra PoolCreated block without creating that pool.
        self.rpc("evm_mine", json!([]))?;
        self.pool_lifecycle()?;
        self.rpc("evm_mine", json!([]))?;
        let (height, current_hash) = self.head()?;
        require(
            height == orphan_height && current_hash != orphan_hash,
            "fixture did not create a same-height fork",
        )?;
        self.wait_synced(&targets)?;
        let mut canonical_balances = balances;
        canonical_balances[1].1 = 185;
        canonical_balances[2].1 = 90;
        let recovered_token = self.balances(&token_schema, &canonical_balances, 975)?;
        self.verify_relational_current(&token_schema)?;
        require(
            self.relational_entities(&token_schema, history_height)? == history_state,
            "reorg changed previously canonical historical ERC20 state",
        )?;
        self.transfer_history(&token_schema, &token)?;
        require(
            !self
                .pool_rows(&pool_schema)?
                .contains_key(&("Pool".into(), orphan_pool)),
            "reorg retained the orphan official extra-fee pool",
        )?;
        let recovered_pool = self.pool_events(&pool_schema)?;
        for (schema, _) in targets {
            require(
                self.sql(&format!(
                    "SELECT count(*) FROM {schema}.blocks WHERE status = 0;"
                ))?
                .parse::<u64>()?
                    > 0,
                "forked blocks were not marked orphaned",
            )?;
        }
        self.check("same-height fork rolls back orphaned Transfer, Swap and Mint results");
        let clean_token = self.index("token", &clean_token_schema, &token, token_start)?;
        let clean_pool = self.index("pool", &clean_pool_schema, &pool, pool_start)?;
        self.wait_synced(&[
            (clean_token_schema.as_str(), clean_token),
            (clean_pool_schema.as_str(), clean_pool),
        ])?;
        require(
            self.entities(&clean_token_schema)? == recovered_token,
            "ERC20 rollback/replay differs from clean canonical indexing",
        )?;
        require(
            self.entities(&clean_pool_schema)? == recovered_pool,
            "Swap/Mint rollback/replay differs from clean canonical indexing",
        )?;
        self.verify_relational_current(&clean_token_schema)?;
        require(
            self.relational_versions(&token_schema)?
                == self.relational_versions(&clean_token_schema)?,
            "typed ERC20 versions after reorg differ from clean canonical replay",
        )?;
        let pool_versions = self.pool_versions(&pool_schema)?;
        require(
            pool_versions == self.pool_versions(&clean_pool_schema)?,
            "native pool event/counter versions after reorg differ from clean canonical replay",
        )?;
        self.verify_hex_text(
            &token_schema,
            &[
                "account",
                "erc20_contract",
                "erc20_balance",
                "erc20_transfer",
                "transaction",
            ],
        )?;
        let pool_tables: Vec<&str> = pool_versions
            .as_object()
            .ok_or("expected pool history tables")?
            .keys()
            .map(String::as_str)
            .collect();
        self.verify_hex_text(&pool_schema, &pool_tables)?;
        self.check("both examples store lowercase hexadecimal text and native pool history equals clean replay");
        self.check("native ERC20 columns and historical ranges match event results and clean replay after reorg");
        self.check(
            "recovered entities and their update hashes exactly equal clean canonical replay",
        );
        for process in [token_process, pool_process, clean_token, clean_pool] {
            self.stop_process(process)?;
        }
        for schema in [
            &token_schema,
            &pool_schema,
            &clean_token_schema,
            &clean_pool_schema,
        ] {
            require(
                self.sql(&format!(
                    "SELECT pg_try_advisory_xact_lock(hashtext(current_database()), hashtext('{schema}'));"
                ))? == "t",
                "indexer did not release writer ownership after cancellation",
            )?;
        }
        self.check("graceful cancellation releases PostgreSQL writer ownership");
        self.report["status"] = json!("passed");
        self.report["canonical_block"] = json!(height);
        self.report["canonical_hash"] = json!(current_hash);
        self.report["event_types"] = json!(["Transfer", "Swap", "Mint"]);
        self.report["mined_receipts"] = json!(self.receipts.len());
        self.report["token_entities"] = json!(recovered_token.len());
        self.report["pool_entities"] = json!(recovered_pool.len());
        Ok(())
    }
}
