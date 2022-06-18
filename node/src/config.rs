use graph::prelude::{
    anyhow::{anyhow, bail, Result},
    info,
    serde::{Deserialize, Serialize},
    Logger,
};

pub struct Opt {
    pub postgres_url: Option<String>,
    pub config: Option<String>,
    // This is only used when we constructs a config purely from command
    // line options. When using a configuration file, pool sizes must be
    // set in the configuration file alone
    pub store_connection_pool_size: u32,
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

impl PoolSizeRule {
    fn matches(&self, name: &str) -> bool {
        match self.node.find(name) {
            None => false,
            Some(m) => m.as_str() == name,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PoolSizeRule {
    #[serde(with = "serde_regex", default = "any_name")]
    node: Regex,
    size: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PoolSize {
    None,
    Fixed(u32),
    Rule(Vec<PoolSizeRule>),
}

impl Default for PoolSize {
    fn default() -> Self {
        Self::None
    }
}

impl PoolSize {
    fn five() -> Self {
        Self::Fixed(5)
    }

    fn validate(&self, connection: &str) -> Result<()> {
        use PoolSize::*;

        let pool_size = match self {
            None => bail!("missing pool size for {}", connection),
            Fixed(s) => *s,
            Rule(rules) => rules.iter().map(|rule| rule.size).min().unwrap_or(0u32),
        };

        if pool_size < 2 {
            Err(anyhow!(
                "connection pool size must be at least 2, but is {} for {}",
                pool_size,
                connection
            ))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Shard {
    pub connection: String,
    #[serde(default = "one")]
    pub weight: usize,
    #[serde(default)]
    pub pool_size: PoolSize,
    #[serde(default = "PoolSize::five")]
    pub fdw_pool_size: PoolSize,
}

impl Shard {
    fn from_opt(opt: &Opt) -> Result<Self> {
        let postgres_url = opt
            .postgres_url
            .as_ref()
            .expect("validation checked that postgres_url is set");
        let pool_size = PoolSize::Fixed(opt.store_connection_pool_size);
        pool_size.validate(&postgres_url)?;

        Ok(Self {
            connection: postgres_url.clone(),
            weight: opt.postgres_host_weights.get(0).cloned().unwrap_or(1),
            pool_size,
            fdw_pool_size: PoolSize::five(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub general: Option<GeneralSection>,
    pub shard: Shard,
}

impl Config {
    pub fn load(logger: &Logger, opt: &Opt) -> Result<Config> {
        info!(
            logger,
            "Generating configuration from command line arguments"
        );
        Self::from_opt(opt)
    }

    fn from_opt(opt: &Opt) -> Result<Config> {
        let shard = Shard::from_opt(opt)?;
        Ok(Config {
            general: None,
            shard,
        })
    }
}

fn one() -> usize {
    1
}

fn any_name() -> Regex {
    Regex::new(ANY_NAME).unwrap()
}

fn no_name() -> Regex {
    Regex::new(NO_NAME).unwrap()
}
