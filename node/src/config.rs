use graph::prelude::{
    anyhow::{anyhow, bail, Context, Result},
    info,
    serde::{
        de::{self, value, SeqAccess, Visitor},
        Deserialize, Deserializer, Serialize,
    },
    serde_json, Logger, StoreError,
};
use std::fs::read_to_string;
// use graph_store_postgres::{DeploymentPlacer, Shard as ShardName, PRIMARY_SHARD};

pub struct Opt {
    pub postgres_url: Option<String>,
    pub config: Option<String>,
    // This is only used when we constructs a config purely from command
    // line options. When using a configuration file, pool sizes must be
    // set in the configuration file alone
    pub store_connection_pool_size: u32,
    pub postgres_secondary_hosts: Vec<String>,
    pub postgres_host_weights: Vec<usize>,
    pub disable_block_ingestor: bool,
    pub node_id: String,
    pub ethereum_rpc: Vec<String>,
    pub ethereum_ws: Vec<String>,
    pub ethereum_ipc: Vec<String>,
    pub unsafe_config: bool,
}
use regex::Regex;

const ANY_NAME: &str = ".*";
/// A regular expression that matches nothing
const NO_NAME: &str = ".^";

impl Default for Opt {
    fn default() -> Self {
        Opt {
            postgres_url: None,
            config: None,
            store_connection_pool_size: 10,
            postgres_secondary_hosts: vec![],
            postgres_host_weights: vec![],
            disable_block_ingestor: true,
            node_id: "default".to_string(),
            ethereum_rpc: vec![],
            ethereum_ws: vec![],
            ethereum_ipc: vec![],
            unsafe_config: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GeneralSection {
    #[serde(with = "serde_regex", default = "no_name")]
    query: Regex,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub general: Option<GeneralSection>,
}

impl Config {
    pub fn load(logger: &Logger, opt: &Opt) -> Result<Config> {
        if let Some(config) = &opt.config {
            info!(logger, "Reading configuration file `{}`", config);
            let config = read_to_string(config)?;
            let mut config: Config = toml::from_str(&config)?;
            config.validate()?;
            Ok(config)
        } else {
            info!(
                logger,
                "Generating configuration from command line arguments"
            );
            Self::from_opt(opt)
        }
    }

    fn from_opt(opt: &Opt) -> Result<Config> {
        Ok(Config { general: None })
    }
}

fn any_name() -> Regex {
    Regex::new(ANY_NAME).unwrap()
}

fn no_name() -> Regex {
    Regex::new(NO_NAME).unwrap()
}
