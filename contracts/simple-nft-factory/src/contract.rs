#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    Addr, BankMsg, Coin, CosmosMsg, Decimal, DepsMut, Env, Fraction, MessageInfo, Reply, Response,
    StdError, SubMsg,
};
use cw2::set_contract_version;
use cw_storage_plus::Item;
use cw_utils::{must_pay, one_coin};
use osmosis_std::types::osmosis::tokenfactory::v1beta1::{
    MsgCreateDenom, MsgCreateDenomResponse, MsgMint, MsgSetBeforeSendHook,
};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, SudoMsg};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:simple-nft-factory";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const CREATE_DENOM_REPLY_ID: u64 = 1;

const DENOM: Item<String> = Item::new("denom");
const PRICE: Item<Coin> = Item::new("price");
const MINTER: Item<Addr> = Item::new("minter");
const MINTER_ROYALTY: Item<Decimal> = Item::new("minter_royalty");

/// Handling contract instantiation
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    MINTER.save(deps.storage, &info.sender)?;
    MINTER_ROYALTY.save(deps.storage, &msg.minter_royalty)?;

    let msg_create_denom = SubMsg::reply_always(
        MsgCreateDenom {
            sender: env.contract.address.to_string(),
            subdenom: msg.name,
        },
        CREATE_DENOM_REPLY_ID,
    );

    Ok(Response::new()
        .add_attribute("method", "instantiate")
        .add_attribute("owner", info.sender)
        .add_submessage(msg_create_denom))
}

/// Handling submessage reply.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        CREATE_DENOM_REPLY_ID => {
            // register created token denom
            let MsgCreateDenomResponse { new_token_denom } = msg.result.try_into()?;
            DENOM.save(deps.storage, &new_token_denom)?;

            let minter = MINTER.load(deps.storage)?;

            let msg_mint = MsgMint {
                sender: env.contract.address.to_string(),
                // mint only 1 token to minter and can perform only once since this denom is an NFT
                // there is no other entrypoint to mint more tokens
                amount: Some(Coin::new(1, new_token_denom.clone()).into()),
                mint_to_address: minter.to_string(),
            };

            // set beforesend listener to this contract
            // this will trigger sudo endpoint before any bank send
            // which makes token transfer pause possible
            let msg_set_beforesend_hook: CosmosMsg = MsgSetBeforeSendHook {
                sender: env.contract.address.to_string(),
                denom: new_token_denom.clone(),
                cosmwasm_address: env.contract.address.to_string(),
            }
            .into();

            Ok(Response::new()
                .add_attribute("new_token_denom", new_token_denom)
                .add_message(msg_mint)
                .add_message(msg_set_beforesend_hook))
        }
        _ => Err(StdError::not_found(format!("No reply handler found for: {:?}", msg)).into()),
    }
}

/// Handling contract execution
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Sell { price } => sell(deps, info, price),
        ExecuteMsg::Buy {} => buy(deps, info),
    }
}

fn sell(deps: DepsMut, info: MessageInfo, price: Coin) -> Result<Response, ContractError> {
    one_coin(&info)?;
    must_pay(&info, &DENOM.load(deps.storage)?)?; // this check is enough since this denom must only have amount 1

    PRICE.save(deps.storage, &price)?;

    Ok(Response::new()
        .add_attribute("method", "sell")
        .add_attribute("sender", info.sender)
        .add_attribute("price", price.to_string()))
}

fn buy(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let price = PRICE.load(deps.storage)?;
    let minter_royalty = MINTER_ROYALTY.load(deps.storage)?;
    valid_price(&info, &price)?;

    let minter = MINTER.load(deps.storage)?;

    // giving minter_royalty to the original owner
    let to_minter_amount = price
        .amount
        .checked_multiply_ratio(minter_royalty.numerator(), minter_royalty.denominator())
        .map_err(|e| StdError::generic_err(e.to_string()))?;

    let to_seller_amount = price
        .amount
        .checked_sub(to_minter_amount)
        .map_err(StdError::overflow)?;

    // remove price once the transaction has been completed
    PRICE.remove(deps.storage);

    Ok(Response::new()
        .add_attribute("method", "buy")
        .add_attribute("sender", info.sender.as_str())
        .add_attribute("price", price.to_string())
        // send the paid price - minter_royalty to the seller
        .add_message(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: vec![Coin::new(to_seller_amount.u128(), price.denom.clone())],
        })
        // send the minter_royalty to the original owner
        .add_message(BankMsg::Send {
            to_address: minter.to_string(),
            amount: vec![Coin::new(to_minter_amount.u128(), price.denom)],
        }))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(_deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        // Hook for bank send  this is called before the token is sent if this contract is registered with MsgSetBeforeSendHook
        SudoMsg::BlockBeforeSend { from, to, .. } => {
            // only authorize sending token
            // - to contract for selling or
            // - sending from contract for buying
            let is_sender_this_contract = from == env.contract.address;
            let is_receiver_this_contract = to == env.contract.address;

            if !(is_sender_this_contract || is_receiver_this_contract) {
                return Err(ContractError::Unauthorized {});
            }

            Ok(Response::new().add_attribute("hook", "block_before_send"))
        }
    }
}

fn valid_price(info: &MessageInfo, price: &Coin) -> Result<(), ContractError> {
    one_coin(info)?;
    must_pay(info, &price.denom)?;

    if info.funds[0].amount != price.amount {
        return Err(ContractError::InvalidPrice {
            expected: price.amount,
            actual: info.funds[0].amount,
        });
    };
    Ok(())
}
