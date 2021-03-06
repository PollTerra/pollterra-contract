use crate::error::ContractError;
use config::config::PollType;
use cosmwasm_std::{
    from_binary, to_binary, Addr, BankMsg, Coin, CosmosMsg, DepsMut, Env, MessageInfo, Response,
    StdResult, SubMsg, Uint128, WasmMsg,
};
use messages::meta_contract::execute_msgs::Cw20HookMsg;
use messages::meta_contract::state::{Config, State, CONTRACTS};
use messages::opinion_poll::execute_msgs::ExecuteMsg as OpinionPollExecuteMsg;
use messages::prediction_poll::execute_msgs::ExecuteMsg as PredictionPollExecuteMsg;

use cw20::Cw20ReceiveMsg;

use messages::msg::PollInstantiateMsg;

// reply_id is only one for now
pub const INSTANTIATE_REPLY_ID: u64 = 1;
const DENOM: &str = "uusd";

pub fn receive_cw20(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = Config::load(deps.storage)?;

    if config.token_contract != deps.api.addr_validate(info.sender.as_str())? {
        return Err(ContractError::IncorrectTokenContract {});
    }

    let creation_deposit: Uint128 = config.creation_deposit;
    if creation_deposit > cw20_msg.amount {
        return Err(ContractError::InsufficientTokenDeposit(creation_deposit));
    }

    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::InitPoll {
            code_id,
            poll_name,
            poll_type,
            end_time,
            resolution_time,
            poll_admin,
            num_side,
        }) => init_poll(
            deps,
            info,
            code_id,
            cw20_msg.sender,
            cw20_msg.amount,
            poll_name,
            poll_type,
            end_time,
            resolution_time,
            poll_admin,
            num_side,
        ),
        _ => Err(ContractError::InvalidCw20Msg {}),
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
    poll_type: String,
    end_time: u64,
    resolution_time: Option<u64>,
    poll_admin: Option<String>,
    num_side: Option<u64>,
) -> Result<Response, ContractError> {
    let config = Config::load(deps.storage)?;

    if config.creation_deposit != deposit_amount {
        return Err(ContractError::InvalidTokenDeposit(config.creation_deposit));
    }

    let poll_type = match poll_type.as_str() {
        "prediction" => Ok(PollType::Prediction),
        "opinion" => Ok(PollType::Opinion),
        _ => Err(ContractError::InvalidPollType {}),
    };

    match poll_type {
        Ok(PollType::Prediction) => {
            if resolution_time.is_none() {
                return Err(ContractError::ShouldHaveResolutionTime {});
            }
            if resolution_time.unwrap() < end_time {
                return Err(ContractError::ShouldEndBeforeResolution {});
            }
        }
        Ok(PollType::Opinion) => {
            if resolution_time.is_some() {
                return Err(ContractError::ShouldNotHaveResolutionTime {});
            }
        }
        _ => {}
    }

    let msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Instantiate {
        admin: poll_admin,
        code_id,
        msg: to_binary(&PollInstantiateMsg {
            generator: deps.api.addr_validate(&generator)?,
            token_contract: config.token_contract,
            deposit_amount,
            reclaimable_threshold: config.reclaimable_threshold,
            poll_name: poll_name.clone(),
            poll_type: poll_type?,
            end_time,
            num_side: num_side.unwrap_or(2),
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
) -> Result<Response, ContractError> {
    let mut config = Config::load(deps.storage)?;

    if !String::new().eq(&config.token_contract) {
        return Err(ContractError::TokenAlreadyRegistered {});
    }

    if !config.is_admin(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }

    config.token_contract = deps.api.addr_validate(&token_contract)?.to_string();
    config.creation_deposit = creation_deposit;
    config.save(deps.storage)?;

    Ok(Response::new().add_attribute("method", "register_token_contract"))
}

pub fn finish_poll(
    deps: DepsMut,
    info: MessageInfo,
    poll_contract: String,
    poll_type: String,
    winner: Option<u64>,
    forced: bool, // TODO : only for internal QA
) -> Result<Response, ContractError> {
    let config = Config::load(deps.storage)?;

    if !config.is_admin(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }

    let poll_type = match poll_type.as_str() {
        "prediction" => Ok(PollType::Prediction),
        "opinion" => Ok(PollType::Opinion),
        _ => Err(ContractError::InvalidPollType {}),
    }?;

    if poll_type == PollType::Prediction && winner.is_none() {
        return Err(ContractError::EmptyWinner {});
    }

    let message: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.addr_validate(&poll_contract)?.to_string(),
        msg: match forced {
            // TODO : only for internal QA
            true => match poll_type {
                PollType::Prediction => to_binary(&PredictionPollExecuteMsg::ForceFinishPoll {
                    winner: winner.unwrap(),
                })?,
                PollType::Opinion => {
                    let addr = &deps.api.addr_validate(poll_contract.as_str())?;
                    CONTRACTS.remove(deps.storage, addr);

                    let mut state: State = State::load(deps.storage)?;
                    state.num_contract -= 1;
                    state.save(deps.storage)?;

                    to_binary(&OpinionPollExecuteMsg::ForceFinishPoll {})?
                }
            },
            false => match poll_type {
                PollType::Prediction => to_binary(&PredictionPollExecuteMsg::FinishPoll {
                    winner: winner.unwrap(),
                })?,
                PollType::Opinion => {
                    let addr = &deps.api.addr_validate(poll_contract.as_str())?;
                    CONTRACTS.remove(deps.storage, addr);

                    let mut state: State = State::load(deps.storage)?;
                    state.num_contract -= 1;
                    state.save(deps.storage)?;

                    to_binary(&OpinionPollExecuteMsg::FinishPoll {})?
                }
            },
        },
        funds: vec![],
    });

    Ok(Response::new()
        .add_message(message)
        .add_attribute("method", "finish_poll"))
}

pub fn transfer(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = Config::load(deps.storage)?;

    if !config.is_admin(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }

    if amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let contract_balance = deps.querier.query_balance(&env.contract.address, DENOM)?;

    if contract_balance.amount < amount {
        return Err(ContractError::InsufficientBalance {});
    }

    let remain_amount = contract_balance.amount - amount;

    let transfer_msg: CosmosMsg = CosmosMsg::Bank(BankMsg::Send {
        to_address: deps.api.addr_validate(recipient.as_str())?.to_string(),
        amount: vec![Coin {
            denom: DENOM.to_string(),
            amount,
        }],
    });

    Ok(Response::new()
        .add_attribute("method", "transfer")
        .add_attribute("requester", info.sender.as_str())
        .add_attribute("recipient", recipient)
        .add_attribute("amount", amount)
        .add_attribute("remain_amount", remain_amount)
        .add_message(transfer_msg))
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    creation_deposit: Option<Uint128>,
    reclaimable_threshold: Option<Uint128>,
    new_admins: Option<Vec<String>>,
) -> Result<Response, ContractError> {
    let mut config = Config::load(deps.storage)?;

    if !config.is_admin(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(creation_deposit) = creation_deposit {
        if String::new().eq(&config.token_contract) {
            return Err(ContractError::TokenNotRegistered {});
        }
        config.creation_deposit = creation_deposit;
    }

    if let Some(reclaimable_threshold) = reclaimable_threshold {
        config.reclaimable_threshold = reclaimable_threshold;
    }

    if let Some(new_admins) = new_admins.as_ref() {
        config.admins = new_admins
            .iter()
            .map(|v| deps.api.addr_validate(v))
            .collect::<StdResult<Vec<Addr>>>()?;
    }

    config.save(deps.storage)?;

    Ok(Response::new().add_attribute("method", "update_config"))
}
