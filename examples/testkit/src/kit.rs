use crate::{
    Result, require,
    signal::check_cancelled,
    values::{call_values, hex_u64, text},
};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    env, fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use url::Url;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(900);

/// The local resources shared by the ERC20 and Uniswap acceptance scenarios.
///
/// Connection strings stay in memory and child process environments.
/// Command logs contain captured output only.
pub struct Testkit {
    pub root: PathBuf,
    pub work: PathBuf,
    pub database_name: String,
    pub database_url: String,
    pub rust_env: BTreeMap<String, String>,
    pub pg_env: BTreeMap<String, String>,
    pub host: String,
    pub binaries: PathBuf,
    pub rpc_url: String,
    pub accounts: Vec<String>,
    pub receipts: Vec<Value>,
    pub report: Value,
    pub pool_factory: String,
    pub pool_address: String,
    pub pool_tokens: [String; 2],
    pub position_manager: String,
    pub swap_router: String,
    pub pool_config: PathBuf,

    processes: Vec<Child>,
    process_names: HashMap<String, usize>,
    event_topics: HashMap<String, String>,
    schema_prefix: String,
}

impl Testkit {
    /// Creates isolated paths, schema names, and process state for a local scenario.
    pub fn new() -> Result<Self> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .ok_or("could not resolve workspace root from testkit crate directory")?
            .to_path_buf();
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let work = env::temp_dir().join(format!("raven-indexing-{stamp}"));
        fs::create_dir(&work)?;
        let rpc_port = unused_port()?;
        let mut rust_env: BTreeMap<_, _> = env::vars().collect();
        rust_env.insert(
            "CARGO_TARGET_DIR".to_owned(),
            root.join("target").display().to_string(),
        );
        Ok(Self {
            root,
            work: work.clone(),
            database_name: String::new(),
            database_url: env::var("RAVEN_DATABASE_URL").unwrap_or_default(),
            rust_env,
            pg_env: env::vars().collect(),
            host: String::new(),
            binaries: PathBuf::new(),
            rpc_url: format!("http://127.0.0.1:{rpc_port}"),
            accounts: Vec::new(),
            receipts: Vec::new(),
            report: json!({"status": "incomplete", "checks": []}),
            pool_factory: String::new(),
            pool_address: String::new(),
            pool_tokens: [String::new(), String::new()],
            position_manager: String::new(),
            swap_router: String::new(),
            pool_config: work.join("chain-policy.json"),
            processes: Vec::new(),
            process_names: HashMap::new(),
            event_topics: HashMap::new(),
            schema_prefix: format!("raven_{stamp}"),
        })
    }

    /// Returns a schema name isolated to this run for one verification role.
    pub fn schema(&self, name: &str) -> String {
        format!("{}_{name}", self.schema_prefix)
    }

    /// Runs a cancellable foreground command with a timeout and optional output log.
    pub fn command(
        &self,
        args: &[String],
        command_env: Option<&BTreeMap<String, String>>,
        log: Option<&str>,
    ) -> Result<String> {
        require(!args.is_empty(), "command is empty")?;
        let mut child = Command::new(&args[0]);
        child.args(&args[1..]).current_dir(&self.root);
        if let Some(command_env) = command_env {
            child.env_clear().envs(command_env);
        }
        child.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = child.spawn()?;
        let stdout = match child.stdout.take() {
            Some(stdout) => spawn_reader(stdout),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("command stdout is unavailable".into());
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => spawn_reader(stderr),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout.join();
                return Err("command stderr is unavailable".into());
            }
        };
        let deadline = Instant::now() + COMMAND_TIMEOUT;
        let status = loop {
            if let Err(error) = check_cancelled() {
                reap_command(&mut child, stdout, stderr);
                return Err(error);
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(_) => {
                    reap_command(&mut child, stdout, stderr);
                    return Err(format!(
                        "{} execution failed; inspect {}",
                        program(args),
                        self.log_path(log).display()
                    )
                    .into());
                }
            }
            if Instant::now() >= deadline {
                reap_command(&mut child, stdout, stderr);
                return Err(format!(
                    "{} timed out; inspect {}",
                    program(args),
                    self.log_path(log).display()
                )
                .into());
            }
            thread::sleep(Duration::from_millis(50));
        };
        let (stdout, stderr) = match join_readers(stdout, stderr) {
            Ok(output) => output,
            Err(()) => {
                // The child is already reaped, but retain the same cleanup path
                // if a reader reports an I/O or thread failure.
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} output capture failed; inspect {}",
                    program(args),
                    self.log_path(log).display()
                )
                .into());
            }
        };
        if let Some(name) = log {
            let mut file = fs::File::create(self.work.join(name))?;
            file.write_all(&stdout)?;
            file.write_all(&stderr)?;
        }
        require(
            status.success(),
            format!(
                "{} failed; inspect {}",
                program(args),
                self.log_path(log).display()
            ),
        )?;
        Ok(String::from_utf8_lossy(&stdout).trim().to_owned())
    }

    /// Invokes a local JSON-RPC method through cast.
    pub fn rpc(&self, method: &str, params: Value) -> Result<Value> {
        let args = vec![
            "cast".into(),
            "rpc".into(),
            "--rpc-url".into(),
            self.rpc_url.clone(),
            "--no-proxy".into(),
            "--rpc-timeout".into(),
            "10".into(),
            "--raw".into(),
            method.into(),
            params.to_string(),
        ];
        let output: Value =
            serde_json::from_str(&self.command(&args, None, Some("last-rpc.log"))?)?;
        if output.get("error").is_some() {
            return Err(format!("local RPC method {method} failed").into());
        }
        Ok(output.get("result").cloned().unwrap_or(output))
    }

    /// Starts a named background process and redirects its output to a log.
    pub fn start_process(
        &mut self,
        args: &[String],
        name: &str,
        command_env: Option<&BTreeMap<String, String>>,
    ) -> Result<usize> {
        require(!args.is_empty(), "process command is empty")?;
        let count = self
            .process_names
            .entry(name.to_owned())
            .and_modify(|n| *n += 1)
            .or_insert(1);
        let log = self.work.join(format!("{name}-{count}.log"));
        let file = fs::File::create(&log)?;
        let mut command = Command::new(&args[0]);
        command.args(&args[1..]).current_dir(&self.root);
        if let Some(command_env) = command_env {
            command.env_clear().envs(command_env);
        }
        command
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file));
        self.processes.push(command.spawn()?);
        Ok(self.processes.len() - 1)
    }

    /// Reports whether a tracked child process is still running.
    pub fn process_running(&mut self, id: usize) -> Result<bool> {
        Ok(self.process(id)?.try_wait()?.is_none())
    }

    /// Stops a tracked process and requires successful termination.
    pub fn stop_process(&mut self, id: usize) -> Result<()> {
        self.stop_process_with_status(id, true)
    }

    /// Terminates a process, escalating to kill if graceful shutdown times out.
    fn stop_process_with_status(&mut self, id: usize, require_success: bool) -> Result<()> {
        let child = self.process(id)?;
        if child.try_wait()?.is_none() {
            unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
            let deadline = Instant::now() + Duration::from_secs(15);
            while child.try_wait()?.is_none() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(50));
            }
            if child.try_wait()?.is_none() {
                child.kill()?;
                let kill_deadline = Instant::now() + Duration::from_secs(5);
                while child.try_wait()?.is_none() && Instant::now() < kill_deadline {
                    thread::sleep(Duration::from_millis(50));
                }
                return Err("local indexer did not stop after cancellation".into());
            }
        }
        let status = child.wait()?;
        if require_success {
            require(status.success(), "local indexer exited unsuccessfully")?;
        }
        Ok(())
    }

    /// Connects to the configured database using private psql environment values.
    pub fn connect_database(&mut self) -> Result<()> {
        require(
            !self.database_url.is_empty(),
            "set RAVEN_DATABASE_URL in the workspace .env or environment",
        )?;
        let url = Url::parse(&self.database_url)
            .map_err(|_| "RAVEN_DATABASE_URL is not a valid PostgreSQL URL")?;
        require(
            matches!(url.scheme(), "postgres" | "postgresql"),
            "RAVEN_DATABASE_URL must use postgres or postgresql",
        )?;
        // psql uses the same URL components as the SQLx-backed indexers.
        self.pg_env.remove("PGSERVICE");
        self.pg_env.remove("PGSERVICEFILE");
        if let Some(host) = url.host_str() {
            self.pg_env.remove("PGHOSTADDR");
            self.pg_env.insert(
                "PGHOST".into(),
                decode_url_component(host.trim_start_matches('[').trim_end_matches(']'))?,
            );
        }
        if let Some(port) = url.port() {
            self.pg_env.insert("PGPORT".into(), port.to_string());
        }
        if !url.username().is_empty() {
            self.pg_env
                .insert("PGUSER".into(), decode_url_component(url.username())?);
        }
        if let Some(password) = url.password() {
            self.pg_env
                .insert("PGPASSWORD".into(), decode_url_component(password)?);
        }
        let database = url.path().trim_start_matches('/');
        if !database.is_empty() {
            self.pg_env
                .insert("PGDATABASE".into(), decode_url_component(database)?);
        }
        for (key, value) in url.query_pairs() {
            let parameter = match key.as_ref() {
                "host" | "hostaddr" => "PGHOST",
                "port" => "PGPORT",
                "user" => "PGUSER",
                "password" => "PGPASSWORD",
                "dbname" => "PGDATABASE",
                "sslmode" | "ssl-mode" => "PGSSLMODE",
                "sslrootcert" | "ssl-root-cert" | "ssl-ca" => "PGSSLROOTCERT",
                "sslcert" | "ssl-cert" => "PGSSLCERT",
                "sslkey" | "ssl-key" => "PGSSLKEY",
                "application_name" => "PGAPPNAME",
                "options" => "PGOPTIONS",
                _ => continue,
            };
            if parameter == "PGHOST" {
                self.pg_env.remove("PGHOSTADDR");
            }
            if parameter == "PGOPTIONS" {
                let options = self.pg_env.entry(parameter.into()).or_default();
                if !options.is_empty() {
                    options.push(' ');
                }
                options.push_str(&value);
            } else {
                self.pg_env.insert(parameter.into(), value.into_owned());
            }
        }
        self.database_name = self.sql("SELECT current_database();")?;
        self.report["database"] = Value::String(self.database_name.clone());
        self.report["database_retained"] = Value::Bool(true);
        self.report["postgres_version"] = Value::String(self.sql("SHOW server_version;")?);
        Ok(())
    }

    /// Starts Anvil and verifies its local chain identity and accounts.
    pub fn start_anvil(&mut self) -> Result<()> {
        let port = self
            .rpc_url
            .rsplit(':')
            .next()
            .ok_or("invalid local RPC URL")?;
        let args = vec![
            "anvil".into(),
            "--quiet".into(),
            "--host".into(),
            "127.0.0.1".into(),
            "--port".into(),
            port.into(),
            "--chain-id".into(),
            "31337".into(),
        ];
        let process = self.start_process(&args, "anvil", None)?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            check_cancelled()?;
            require(
                self.process_running(process)?,
                "owned Anvil process exited before startup",
            )?;
            let ready = self.rpc("eth_chainId", json!([])).and_then(|chain| {
                require(
                    hex_u64(&text(&chain)?)? == 31337,
                    "unexpected local chain ID",
                )?;
                let client = self.rpc("web3_clientVersion", json!([]))?;
                require(
                    text(&client)?.to_lowercase().contains("anvil"),
                    "local endpoint is not Anvil",
                )
            });
            if ready.is_ok() {
                break;
            }
            require(
                Instant::now() < deadline,
                "local Anvil did not become ready",
            )?;
            thread::sleep(Duration::from_millis(100));
        }
        self.accounts = self
            .rpc("eth_accounts", json!([]))?
            .as_array()
            .ok_or("local RPC returned invalid accounts")?
            .iter()
            .map(text)
            .collect::<Result<Vec<_>>>()?;
        self.accounts.truncate(4);
        require(
            self.accounts.len() == 4,
            "local Anvil needs four unlocked fixture accounts",
        )
    }

    /// Executes SQL against the scenario database.
    pub fn sql(&self, query: &str) -> Result<String> {
        self.command(
            &vec![
                "psql".into(),
                "--no-psqlrc".into(),
                "--no-password".into(),
                "-At".into(),
                "-v".into(),
                "ON_ERROR_STOP=1".into(),
                "-c".into(),
                query.into(),
            ],
            Some(&self.pg_env),
            Some("last-query.log"),
        )
    }

    /// Records a completed verification in the scenario report.
    pub fn check(&mut self, label: &str) {
        self.report["checks"]
            .as_array_mut()
            .expect("testkit report checks is an array")
            .push(Value::String(label.into()));
        println!("Verified: {label}");
    }

    /// Verifies required tools and builds the workspace test artifacts.
    pub fn build_and_test(&mut self) -> Result<()> {
        for tool in ["cargo", "rustc", "forge", "cast", "anvil", "psql"] {
            require(
                find_program(tool),
                format!("required local tool is unavailable: {tool}"),
            )?;
        }
        let rustc = self.command(
            &vec!["rustc".into(), "-vV".into()],
            None,
            Some("rustc-version.log"),
        )?;
        self.host = rustc
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .ok_or("rustc did not report a host target")?
            .to_owned();
        let common = vec![
            "--locked".into(),
            "--offline".into(),
            "--target".into(),
            self.host.clone(),
        ];
        let mut test = vec![
            "cargo".into(),
            "test".into(),
            "--workspace".into(),
            "--all-targets".into(),
        ];
        test.extend(common.clone());
        self.command(&test, Some(&self.rust_env), Some("rust-tests.log"))?;
        let mut build = vec![
            "cargo".into(),
            "build".into(),
            "-p".into(),
            "raven-example-erc20".into(),
            "-p".into(),
            "raven-example-uniswap-v3".into(),
        ];
        build.extend(common);
        self.command(&build, Some(&self.rust_env), Some("rust-build.log"))?;
        self.binaries = self.root.join("target").join(&self.host).join("debug");
        self.check("current workspace builds and registered Rust tests pass");
        Ok(())
    }

    /// Builds only the ERC20 example and resolves its executable path.
    pub fn build_erc20(&mut self) -> Result<()> {
        for tool in ["cargo", "rustc", "forge", "cast", "anvil", "psql"] {
            require(
                find_program(tool),
                format!("required local tool is unavailable: {tool}"),
            )?;
        }
        let rustc = self.command(
            &vec!["rustc".into(), "-vV".into()],
            None,
            Some("rustc-version.log"),
        )?;
        self.host = rustc
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .ok_or("rustc did not report a host target")?
            .to_owned();
        let mut args = vec![
            "cargo".into(),
            "build".into(),
            "-p".into(),
            "raven-example-erc20".into(),
            "--locked".into(),
            "--offline".into(),
        ];
        args.extend(["--target".into(), self.host.clone()]);
        self.command(&args, Some(&self.rust_env), Some("erc20-build.log"))?;
        self.binaries = self.root.join("target").join(&self.host).join("debug");
        Ok(())
    }

    /// Deploys a compiled contract and returns its address and deployment block.
    pub fn deploy(
        &mut self,
        example: &str,
        contract: &str,
        constructor: &[String],
        source: Option<&str>,
    ) -> Result<(String, u64)> {
        let fixture = self.root.join("examples").join(example).join("anvil");
        let artifacts = self.work.join(format!("{example}-artifacts"));
        let options = vec![
            "--root".into(),
            fixture.display().to_string(),
            "--out".into(),
            artifacts.display().to_string(),
            "--cache-path".into(),
            self.work
                .join(format!("{example}-cache.json"))
                .display()
                .to_string(),
            "--offline".into(),
        ];
        let mut build = vec!["forge".into(), "build".into()];
        build.extend(options.clone());
        self.command(&build, None, Some(&format!("{example}-build.log")))?;
        let source = source
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{contract}.sol"));
        let mut args = vec![
            "forge".into(),
            "create".into(),
            format!("src/{source}:{contract}"),
        ];
        args.extend(options);
        args.extend([
            "--rpc-url".into(),
            self.rpc_url.clone(),
            "--broadcast".into(),
            "--unlocked".into(),
            "--from".into(),
            self.accounts[0].clone(),
            "--json".into(),
        ]);
        if !constructor.is_empty() {
            args.push("--constructor-args".into());
            args.extend_from_slice(constructor);
        }
        let deployed: Value = serde_json::from_str(&self.command(
            &args,
            None,
            Some(&format!("{example}-deploy.log")),
        )?)?;
        let hash = text(&deployed["transactionHash"])?;
        let receipt = self.rpc("eth_getTransactionReceipt", json!([hash]))?;
        require(
            !receipt.is_null() && hex_u64(&text(&receipt["status"])?)? == 1,
            "local fixture deployment failed",
        )?;
        self.receipts.push(receipt.clone());
        Ok((
            text(&deployed["deployedTo"])?.to_lowercase(),
            hex_u64(&text(&receipt["blockNumber"])?)?,
        ))
    }

    /// Loads a pinned official Uniswap artifact from the fixture directory.
    pub fn official_artifact(&self, relative: &str) -> Result<Value> {
        let fixture = self.root.join("examples/uniswap-v3/anvil");
        let lock: Value =
            serde_json::from_slice(&fs::read(fixture.join("official-artifacts.lock.json"))?)?;
        let artifact = fixture.join("official-artifacts").join(relative);
        let expected = text(&lock["artifacts"][relative])?;
        let actual = format!("{:x}", Sha256::digest(fs::read(&artifact)?));
        require(
            actual == expected,
            format!("official artifact hash differs: {relative}"),
        )?;
        Ok(serde_json::from_slice(&fs::read(artifact)?)?)
    }

    /// Deploys a contract from an official artifact with encoded constructor input.
    pub fn deploy_artifact(
        &mut self,
        relative: &str,
        constructor: Option<(&str, &[String])>,
        links: &[(String, String)],
    ) -> Result<(String, u64)> {
        let artifact = self.official_artifact(relative)?;
        let mut bytecode = text(&artifact["bytecode"])?;
        for (placeholder, address) in links {
            require(
                bytecode.contains(placeholder),
                format!("missing link placeholder in {relative}"),
            )?;
            bytecode = bytecode.replace(placeholder, address.strip_prefix("0x").unwrap_or(address));
        }
        require(
            !bytecode.contains("__$"),
            format!("unlinked library in {relative}"),
        )?;
        if let Some((signature, values)) = constructor {
            let mut encode = vec!["cast".into(), "abi-encode".into(), signature.into()];
            encode.extend_from_slice(values);
            bytecode.push_str(
                self.command(&encode, None, Some("last-constructor.log"))?
                    .trim_start_matches("0x"),
            );
        }
        let args = vec![
            "cast".into(),
            "send".into(),
            "--rpc-url".into(),
            self.rpc_url.clone(),
            "--unlocked".into(),
            "--from".into(),
            self.accounts[0].clone(),
            "--json".into(),
            "--create".into(),
            bytecode,
        ];
        let log_name = format!(
            "{}-deploy.log",
            Path::new(relative)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("artifact")
        );
        let output = self.command(&args, None, Some(&log_name))?;
        let receipt: Value = serde_json::from_str(&output)?;
        require(
            hex_u64(&text(&receipt["status"])?)? == 1,
            format!("official contract deployment failed: {relative}"),
        )?;
        self.receipts.push(receipt.clone());
        Ok((
            text(&receipt["contractAddress"])?.to_lowercase(),
            hex_u64(&text(&receipt["blockNumber"])?)?,
        ))
    }

    /// Sends a contract transaction from a selected local fixture account.
    pub fn send(
        &mut self,
        address: &str,
        signature: &str,
        args: &[String],
        sender: usize,
    ) -> Result<Value> {
        let mut command = vec![
            "cast".into(),
            "send".into(),
            address.into(),
            signature.into(),
        ];
        command.extend_from_slice(args);
        command.extend([
            "--rpc-url".into(),
            self.rpc_url.clone(),
            "--unlocked".into(),
            "--from".into(),
            self.accounts
                .get(sender)
                .ok_or("fixture sender is unavailable")?
                .clone(),
            "--json".into(),
        ]);
        let receipt: Value =
            serde_json::from_str(&self.command(&command, None, Some("last-transaction.log"))?)?;
        require(
            hex_u64(&text(&receipt["status"])?)? == 1,
            "local transaction reverted",
        )?;
        self.receipts.push(receipt.clone());
        Ok(receipt)
    }

    /// Executes a read-only contract call and decodes its JSON result.
    pub fn call(&self, address: &str, signature: &str, args: &[String]) -> Result<Value> {
        let mut command = vec![
            "cast".into(),
            "call".into(),
            address.into(),
            signature.into(),
        ];
        command.extend_from_slice(args);
        command.extend(["--rpc-url".into(), self.rpc_url.clone(), "--json".into()]);
        let values: Value =
            serde_json::from_str(&self.command(&command, None, Some("last-call.log"))?)?;
        call_values(values)
    }

    /// Returns and caches the topic hash for an event signature.
    pub fn event_topic(&mut self, signature: &str) -> Result<String> {
        if let Some(topic) = self.event_topics.get(signature) {
            return Ok(topic.clone());
        }
        let topic = self
            .command(
                &vec!["cast".into(), "keccak".into(), signature.into()],
                None,
                Some("event-topic.log"),
            )?
            .to_lowercase();
        self.event_topics.insert(signature.into(), topic.clone());
        Ok(topic)
    }

    /// Fetches canonical logs for a contract event signature.
    pub fn canonical_logs(&mut self, address: &str, signature: &str) -> Result<Vec<Value>> {
        let topic = self.event_topic(signature)?;
        let mut logs = Vec::new();
        for receipt in &self.receipts {
            for log in receipt["logs"].as_array().into_iter().flatten() {
                let topics = log["topics"].as_array();
                if text(&log["address"])?.eq_ignore_ascii_case(address)
                    && topics
                        .and_then(|v| v.first())
                        .map(text)
                        .transpose()?
                        .map(|v| v.eq_ignore_ascii_case(&topic))
                        .unwrap_or(false)
                {
                    let block = self.rpc(
                        "eth_getBlockByNumber",
                        json!([text(&log["blockNumber"])?, false]),
                    )?;
                    if !block.is_null() && block["hash"] == log["blockHash"] {
                        logs.push(log.clone());
                    }
                }
            }
        }
        Ok(logs)
    }

    /// Starts an example indexer for the requested contract and schema.
    pub fn index(&mut self, kind: &str, schema: &str, address: &str, start: u64) -> Result<usize> {
        let mut command_env: BTreeMap<_, _> = env::vars()
            .filter(|(key, _)| !key.starts_with("RAVEN_"))
            .collect();
        command_env.extend([
            ("RAVEN_RPC_URL".into(), self.rpc_url.clone()),
            ("RAVEN_DATABASE_URL".into(), self.database_url.clone()),
            ("RAVEN_SCHEMA".into(), schema.into()),
            ("RAVEN_NETWORK_NAME".into(), "anvil".into()),
            ("RAVEN_START_BLOCK".into(), start.to_string()),
            ("RAVEN_CONFIRMATIONS".into(), "0".into()),
            ("RUST_LOG".into(), "info".into()),
        ]);
        let binary = match kind {
            "token" => {
                command_env.insert("RAVEN_TOKEN".into(), address.into());
                "raven-example-erc20"
            }
            "pool" => {
                command_env.insert("RAVEN_FACTORY".into(), self.pool_factory.clone());
                command_env.insert(
                    "RAVEN_CHAIN_CONFIG".into(),
                    self.pool_config.display().to_string(),
                );
                command_env.insert(
                    "RAVEN_POSITION_MANAGER".into(),
                    self.position_manager.clone(),
                );
                "raven-example-uniswap-v3"
            }
            _ => return Err("unknown testkit indexer kind".into()),
        };
        self.start_process(
            &[self.binaries.join(binary).display().to_string()],
            schema,
            Some(&command_env),
        )
    }

    /// Returns the current local chain height and hash.
    pub fn head(&self) -> Result<(u64, String)> {
        let block = self.rpc("eth_getBlockByNumber", json!(["latest", false]))?;
        Ok((hex_u64(&text(&block["number"])?)?, text(&block["hash"])?))
    }

    /// Waits until each indexer has committed the current chain head.
    pub fn wait_synced(&mut self, targets: &[(&str, usize)]) -> Result<(u64, String)> {
        let (height, hash) = self.head()?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            check_cancelled()?;
            let mut complete = true;
            for (schema, process) in targets {
                require(
                    self.process_running(*process)?,
                    format!("indexer {schema} stopped; inspect its log"),
                )?;
                let exists = self.sql(&format!(
                    "SELECT to_regclass('{schema}.networks') IS NOT NULL;"
                ))? == "t";
                let pointer = if exists {
                    let output = self.sql(&format!("SELECT json_build_array(latest_block_number::text, latest_block_hash) FROM {schema}.networks;"))?;
                    if output.is_empty() {
                        Value::Null
                    } else {
                        serde_json::from_str(&output)?
                    }
                } else {
                    Value::Null
                };
                if pointer != json!([height.to_string(), hash.clone()]) {
                    complete = false;
                }
            }
            if complete {
                return Ok((height, hash));
            }
            require(
                Instant::now() < deadline,
                format!("indexing did not reach canonical block {height}; inspect logs"),
            )?;
            thread::sleep(Duration::from_millis(100));
        }
    }

    /// Returns all application entities from a schema in stable order.
    pub fn entities(&self, schema: &str) -> Result<Vec<Value>> {
        let view = if self.sql(&format!(
            "SELECT to_regclass('{schema}.erc20_state') IS NOT NULL;"
        ))? == "t"
        {
            "erc20_state"
        } else {
            "pool_state"
        };
        let rows = self.sql(&format!("SELECT COALESCE(json_agg(json_build_array(s.entity_type, s.entity_id, s.data, b.hash) ORDER BY s.entity_type, s.entity_id), '[]'::json) FROM {schema}.{view} s JOIN {schema}.blocks b ON b.number = s.block_number AND b.status = 1;"))?;
        Ok(serde_json::from_str(&rows)?)
    }

    /// Stops the Anvil and indexer processes owned by this scenario.
    pub fn cleanup(&mut self) -> Result<()> {
        let mut failure: Option<Box<dyn std::error::Error>> = None;
        // Anvil may exit with a nonzero status when cleanup sends SIGTERM.
        for index in (0..self.processes.len()).rev() {
            if let Err(error) = self.stop_process_with_status(index, false) {
                failure.get_or_insert(error);
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Returns a tracked child process by identifier.
    fn process(&mut self, id: usize) -> Result<&mut Child> {
        self.processes
            .get_mut(id)
            .ok_or_else(|| "unknown owned process".into())
    }
    /// Resolves a command log path, including the default log file.
    fn log_path(&self, log: Option<&str>) -> PathBuf {
        log.map(|name| self.work.join(name))
            .unwrap_or_else(|| self.work.clone())
    }
}

impl Drop for Testkit {
    /// Performs best-effort cleanup when a scenario leaves scope.
    fn drop(&mut self) {
        for child in self.processes.iter_mut().rev() {
            if child.try_wait().ok().flatten().is_none() {
                unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
                let deadline = Instant::now() + Duration::from_secs(15);
                while child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(50));
                }
                if child.try_wait().ok().flatten().is_none() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }
}

/// Reserves an available loopback TCP port number.
fn unused_port() -> Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}
/// Returns a display name for a command argument vector.
fn program(args: &[String]) -> &str {
    Path::new(&args[0])
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("command")
}
/// Reports whether a command can be resolved from PATH.
fn find_program(program: &str) -> bool {
    env::var_os("PATH")
        .map(|path| env::split_paths(&path).any(|dir| dir.join(program).is_file()))
        .unwrap_or(false)
}
/// Decodes a URL component without including its value in an error message.
fn decode_url_component(value: &str) -> Result<String> {
    Ok(percent_encoding::percent_decode_str(value)
        .decode_utf8()
        .map_err(|_| "invalid UTF-8 encoding in RAVEN_DATABASE_URL")?
        .into_owned())
}

/// Starts a reader thread that captures one child output stream.
fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

/// Joins stdout and stderr capture threads into their byte buffers.
fn join_readers(
    stdout: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stderr: thread::JoinHandle<std::io::Result<Vec<u8>>>,
) -> std::result::Result<(Vec<u8>, Vec<u8>), ()> {
    let stdout = stdout.join().ok().and_then(std::result::Result::ok);
    let stderr = stderr.join().ok().and_then(std::result::Result::ok);
    match (stdout, stderr) {
        (Some(stdout), Some(stderr)) => Ok((stdout, stderr)),
        _ => Err(()),
    }
}

/// Kills, waits for, and drains a failed foreground command.
fn reap_command(
    child: &mut Child,
    stdout: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stderr: thread::JoinHandle<std::io::Result<Vec<u8>>>,
) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = stdout.join();
    let _ = stderr.join();
}
