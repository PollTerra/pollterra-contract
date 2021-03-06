use cosmwasm_std::{Binary, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub admins: Vec<String>,
    pub managing_token: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    UpdateAdmins {
        admins: Option<Vec<String>>,
    },
    RegisterDistribution {
        start_height: u64,
        end_height: u64,
        recipient: String,
        amount: Uint128,
        message: Option<Binary>,
    },
    UpdateDistribution {
        id: u64,
        start_height: Option<u64>,
        end_height: Option<u64>,
        amount: Option<Uint128>,
        message: Option<Binary>,
    },
    RemoveDistributionMessage {
        id: u64,
    },
    Distribute {
        id: Option<u64>,
    },
    Transfer {
        recipient: String,
        amount: Uint128,
    },
}
