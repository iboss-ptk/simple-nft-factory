use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Coin, Decimal};

/// Message type for `instantiate` entry_point
#[cw_serde]
pub struct InstantiateMsg {
    pub name: String,
    pub minter_royalty: Decimal,
}

/// Message type for `execute` entry_point
#[cw_serde]
pub enum ExecuteMsg {
    Sell { price: Coin },
    Buy {},
}

/// Message type for `migrate` entry_point
#[cw_serde]
pub enum MigrateMsg {}

/// Message type for `query` entry_point
#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    // This example query variant indicates that any client can query the contract
    // using `YourQuery` and it will return `YourQueryResponse`
    // This `returns` information will be included in contract's schema
    // which is used for client code generation.
    //
    // #[returns(YourQueryResponse)]
    // YourQuery {},
}

// We define a custom struct for each query response
// #[cw_serde]
// pub struct YourQueryResponse {}

#[cw_serde]
pub enum SudoMsg {
    /// tokenfactory hook
    BlockBeforeSend {
        from: String,
        to: String,
        amount: Coin,
    },
}
