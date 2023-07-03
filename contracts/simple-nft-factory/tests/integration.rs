use std::path::PathBuf;

use cosmwasm_std::{Coin, Decimal};
use osmosis_std::types::cosmos::bank::v1beta1::{MsgSend, MsgSendResponse};
use osmosis_test_tube::{
    cosmrs::proto::cosmos::bank::v1beta1::{QueryBalanceRequest, QueryTotalSupplyRequest},
    Account, Bank, Module, OsmosisTestApp, Runner, Wasm,
};
use simple_nft_factory::msg::{ExecuteMsg, InstantiateMsg};

#[test]
fn test() {
    // init app and module wrappers
    // module wrappers provide convenient methods for interacting with the module
    let app = OsmosisTestApp::new();
    let wasm = Wasm::new(&app);
    let bank = Bank::new(&app);

    // init accounts
    let minter = app
        .init_account(&[Coin::new(1_000_000_000_000_000, "uosmo")])
        .unwrap();
    let users = app
        .init_accounts(&[Coin::new(1_000_000_000_000_000, "uosmo")], 2)
        .unwrap();

    let code_id = wasm
        .store_code(&get_wasm_byte_code(), None, &minter)
        .unwrap()
        .data
        .code_id;

    let contract_addr = wasm
        .instantiate(
            code_id,
            &InstantiateMsg {
                name: "test".to_string(),
                minter_royalty: Decimal::percent(1),
            },
            None,
            None,
            &[],
            &minter,
        )
        .unwrap()
        .data
        .address;

    let nft_denom = format!("factory/{}/test", contract_addr);

    // minter should have 1 NFT
    let amount = bank
        .query_balance(&QueryBalanceRequest {
            address: minter.address(),
            denom: nft_denom.clone(),
        })
        .unwrap()
        .balance
        .unwrap()
        .amount;

    assert_eq!(amount, "1");

    // total supply should be 1
    let supply = bank
        .query_total_supply(&QueryTotalSupplyRequest { pagination: None })
        .unwrap()
        .supply
        .into_iter()
        .find(|coin| coin.denom == nft_denom)
        .unwrap();

    assert_eq!(supply.amount, "1");

    // sending NFT other user should be blocked
    let err = app
        .execute::<_, MsgSendResponse>(
            MsgSend {
                from_address: minter.address(),
                to_address: users[0].address(),
                amount: vec![Coin::new(1, nft_denom.clone()).into()],
            },
            MsgSend::TYPE_URL,
            &minter,
        )
        .unwrap_err();

    assert_eq!(err.to_string(), format!("execute error: failed to execute message; message index: 0: failed to call before send hook for denom {nft_denom}: Unauthorized: execute wasm contract failed"));

    // only allow for selling through the contract
    wasm.execute(
        &contract_addr,
        &ExecuteMsg::Sell {
            price: Coin::new(100, "uosmo"),
        },
        &[Coin::new(1, nft_denom.clone())],
        &minter,
    )
    .unwrap();

    let minter_osmo_before_bought = bank
        .query_balance(&QueryBalanceRequest {
            address: minter.address(),
            denom: "uosmo".to_string(),
        })
        .unwrap()
        .balance
        .unwrap()
        .amount
        .parse::<u128>()
        .unwrap();

    wasm.execute(
        &contract_addr,
        &ExecuteMsg::Buy {},
        &[Coin::new(100, "uosmo")],
        &users[0],
    )
    .unwrap();

    // minter should have 0 NFT
    let amount = bank
        .query_balance(&QueryBalanceRequest {
            address: minter.address(),
            denom: nft_denom.clone(),
        })
        .unwrap()
        .balance
        .unwrap()
        .amount;

    assert_eq!(amount, "0");

    // user[0] should have 1 NFT
    let amount = bank
        .query_balance(&QueryBalanceRequest {
            address: users[0].address(),
            denom: nft_denom.clone(),
        })
        .unwrap()
        .balance
        .unwrap()
        .amount;

    assert_eq!(amount, "1");

    let minter_osmo_after_bought = bank
        .query_balance(&QueryBalanceRequest {
            address: minter.address(),
            denom: "uosmo".to_string(),
        })
        .unwrap()
        .balance
        .unwrap()
        .amount
        .parse::<u128>()
        .unwrap();

    assert_eq!(
        minter_osmo_after_bought,
        minter_osmo_before_bought + 100u128
    );

    // selling from user[0] to user[1] – minter should get 1% royalty
    wasm.execute(
        &contract_addr,
        &ExecuteMsg::Sell {
            price: Coin::new(100, "uosmo"),
        },
        &[Coin::new(1, nft_denom)],
        &users[0],
    )
    .unwrap();

    let minter_osmo_before_bought = bank
        .query_balance(&QueryBalanceRequest {
            address: minter.address(),
            denom: "uosmo".to_string(),
        })
        .unwrap()
        .balance
        .unwrap()
        .amount
        .parse::<u128>()
        .unwrap();

    let user_0_osmo_before_bought = bank
        .query_balance(&QueryBalanceRequest {
            address: users[0].address(),
            denom: "uosmo".to_string(),
        })
        .unwrap()
        .balance
        .unwrap()
        .amount
        .parse::<u128>()
        .unwrap();

    wasm.execute(
        &contract_addr,
        &ExecuteMsg::Buy {},
        &[Coin::new(100, "uosmo")],
        &users[1],
    )
    .unwrap();

    let minter_osmo_after_bought = bank
        .query_balance(&QueryBalanceRequest {
            address: minter.address(),
            denom: "uosmo".to_string(),
        })
        .unwrap()
        .balance
        .unwrap()
        .amount
        .parse::<u128>()
        .unwrap();

    let user_0_osmo_after_bought = bank
        .query_balance(&QueryBalanceRequest {
            address: users[0].address(),
            denom: "uosmo".to_string(),
        })
        .unwrap()
        .balance
        .unwrap()
        .amount
        .parse::<u128>()
        .unwrap();

    assert_eq!(minter_osmo_after_bought, minter_osmo_before_bought + 1u128);
    assert_eq!(user_0_osmo_after_bought, user_0_osmo_before_bought + 99u128);
}

fn get_wasm_byte_code() -> Vec<u8> {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    std::fs::read(
        manifest_path
            .join("..")
            .join("..")
            .join("target")
            .join("wasm32-unknown-unknown")
            .join("release")
            .join("simple_nft_factory.wasm"),
    )
    .unwrap()
}
