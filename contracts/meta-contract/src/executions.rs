use crate::state::{read_config, store_config, Config, Cw20HookMsg};
use cosmwasm_std::{
    from_binary, to_binary, Addr, CosmosMsg, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, SubMsg, Uint128, WasmMsg,
};
use cw20::Cw20ReceiveMsg;

use messages::msg::PollInstantiateMsg;

// reply_id is only one for now
pub const INSTANTIATE_REPLY_ID: u64 = 1;

pub fn receive_cw20(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, StdError> {
    let config: Config = read_config(deps.storage).unwrap();
    if config.token_contract != deps.api.addr_validate(info.sender.as_str())? {
        return Err(StdError::generic_err("Incorrect token contract"));
    }

    let creation_deposit: Uint128 = config.creation_deposit;
    if creation_deposit > cw20_msg.amount {
        return Err(StdError::generic_err("Insufficient token amount"));
    }

    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::InitPoll {
            code_id,
            poll_name,
            bet_end_time,
            resolution_time,
        }) => init_poll(
            deps,
            info,
            code_id,
            cw20_msg.sender,
            cw20_msg.amount,
            poll_name,
            bet_end_time,
            resolution_time,
        ),
        _ => Err(StdError::generic_err("Cw20Msg doesn't match")),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn init_poll(
    deps: DepsMut,
    _info: MessageInfo,
    code_id: u64,
    generator: String,
    deposit_amount: Uint128,
    poll_name: String,
    bet_end_time: u64,
    resolution_time: u64,
) -> StdResult<Response> {
    let config: Config = read_config(deps.storage).unwrap();
    let contract_owner: Addr = config.owner;

    if config.creation_deposit != deposit_amount {
        return Err(StdError::generic_err(format!(
            "deposit amount should be {}",
            config.creation_deposit
        )));
    }

    let msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Instantiate {
        admin: Some(contract_owner.to_string()),
        code_id,
        msg: to_binary(&PollInstantiateMsg {
            generator: deps.api.addr_validate(&generator)?,
            token_contract: config.token_contract,
            deposit_amount,
            reclaimable_threshold: config.reclaimable_threshold,
            poll_name: poll_name.clone(),
            bet_end_time,
            resolution_time,
            minimum_bet_amount: Some(config.minimum_bet_amount),
            tax_percentage: Some(config.tax_percentage),
        })?,
        funds: vec![],
        label: poll_name,
    });

    let submsg = SubMsg::reply_on_success(msg, INSTANTIATE_REPLY_ID);

    Ok(Response::new()
        .add_attribute("method", "try_init_poll")
        .add_submessage(submsg))
}

pub fn register_token_contract(
    deps: DepsMut,
    info: MessageInfo,
    token_contract: String,
    creation_deposit: Uint128,
) -> StdResult<Response> {
    let mut config: Config = read_config(deps.storage).unwrap();
    if !String::new().eq(&config.token_contract) {
        return Err(StdError::generic_err("already registered"));
    }

    if info.sender != config.owner {
        return Err(StdError::generic_err(
            "only the original owner can register a token contract",
        ));
    }

    config.token_contract = deps.api.addr_validate(&token_contract)?.to_string();
    config.creation_deposit = creation_deposit;
    store_config(deps.storage, &config)?;

    Ok(Response::new().add_attribute("method", "register_token_contract"))
}

// TODO : update config at once
pub fn update_creation_deposit(
    deps: DepsMut,
    info: MessageInfo,
    creation_deposit: Uint128,
) -> StdResult<Response> {
    let mut config: Config = read_config(deps.storage).unwrap();
    if String::new().eq(&config.token_contract) {
        return Err(StdError::generic_err("token not registered"));
    }

    if info.sender != config.owner {
        return Err(StdError::generic_err(
            "only the original owner can update creation deposit amount",
        ));
    }

    config.creation_deposit = creation_deposit;
    store_config(deps.storage, &config)?;

    Ok(Response::new().add_attribute("method", "update_creatoin_deposit"))
}

pub fn update_reclaimable_threshold(
    deps: DepsMut,
    info: MessageInfo,
    reclaimable_threshold: Uint128,
) -> StdResult<Response> {
    let mut config: Config = read_config(deps.storage).unwrap();

    if info.sender != config.owner {
        return Err(StdError::generic_err(
            "only the original owner can update reclaimable threshold amount",
        ));
    }

    config.reclaimable_threshold = reclaimable_threshold;
    store_config(deps.storage, &config)?;

    Ok(Response::new().add_attribute("method", "update_reclaimable_threshold"))
}

pub fn try_transfer_owner(
    deps: DepsMut,
    info: MessageInfo,
    new_owner: String,
) -> StdResult<Response> {
    let mut config = read_config(deps.storage)?;
    if info.sender != config.owner {
        return Err(StdError::generic_err(
            "only the original owner can transfer the ownership",
        ));
    }
    config.owner = deps.api.addr_validate(&new_owner)?;
    store_config(deps.storage, &config)?;

    Ok(Response::new().add_attribute("method", "try_transfer_owner"))
}