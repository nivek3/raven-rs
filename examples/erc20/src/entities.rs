//! Stored ERC-20 fields. Reverse relations are queried
//! through the corresponding foreign IDs instead of persisting duplicate arrays.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Account {
    pub id: String,
    #[serde(rename = "asERC20")]
    pub as_erc20: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ERC20Contract {
    pub id: String,
    pub as_account: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub decimals: u8,
    pub total_supply: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ERC20Balance {
    pub id: String,
    pub contract: String,
    pub account: Option<String>,
    pub value: String,
    pub value_exact: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ERC20Transfer {
    pub id: String,
    pub emitter: String,
    pub transaction: String,
    pub timestamp: String,
    pub contract: String,
    pub from: Option<String>,
    pub from_balance: Option<String>,
    pub to: Option<String>,
    pub to_balance: Option<String>,
    pub value: String,
    pub value_exact: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Transaction {
    pub id: String,
    pub timestamp: String,
    pub block_number: String,
}

impl raven_engine::Entity for Account {
    const ENTITY_NAME: &'static str = "Account";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for ERC20Contract {
    const ENTITY_NAME: &'static str = "ERC20Contract";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for ERC20Balance {
    const ENTITY_NAME: &'static str = "ERC20Balance";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for ERC20Transfer {
    const ENTITY_NAME: &'static str = "ERC20Transfer";

    fn id(&self) -> &str {
        &self.id
    }
}

impl raven_engine::Entity for Transaction {
    const ENTITY_NAME: &'static str = "Transaction";

    fn id(&self) -> &str {
        &self.id
    }
}
