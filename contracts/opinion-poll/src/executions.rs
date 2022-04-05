use cosmwasm_std::{
    to_binary, CosmosMsg, DepsMut, Env, MessageInfo, Order, Response, StdError, StdResult,
    Timestamp, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use std::cmp::Ordering;
use std::convert::TryInto;

use crate::state::{read_config, read_state, store_config, store_state, BetStatus, SIDES, VOTES};

pub fn vote(deps: DepsMut, env: Env, info: MessageInfo, side: u64) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    // current block time is less than start time or larger than bet end time
    if env.block.time >= Timestamp::from_seconds(config.bet_end_time) {
        return Err(StdError::generic_err(format!(
            "Bet is not live. current block time: {}, bet end time: {}",
            env.block.time, config.bet_end_time
        )));
    }

    // Check if already participated
    if VOTES.has(deps.storage, &info.sender) {
        return Err(StdError::generic_err("already participated"));
    }

    // TODO : participation requirements

    // Check if some funds are sent
    if !info.funds.is_empty() {
        return Err(StdError::generic_err("you'd better not send ust"));
    }

    SIDES.update(
        deps.storage,
        &side.to_be_bytes(),
        |exists| -> StdResult<u64> {
            match exists {
                Some(count) => Ok(count + 1),
                None => Ok(1),
            }
        },
    )?;

    VOTES.update(deps.storage, &info.sender, |exists| -> StdResult<u64> {
        match exists {
            None => Ok(side),
            Some(_) => Ok(side),
        }
    })?;

    // Save the new state
    let mut state = read_state(deps.storage)?;
    state.total_amount += Uint128::from(1u8);
    store_state(deps.storage, &state)?;

    Ok(Response::new().add_attributes(vec![
        ("action", "try_bet"),
        ("address", info.sender.as_str()),
        ("side", &side.to_string()),
    ]))
}

pub fn finish_poll(deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;

    // only contract's owner can finish poll
    if info.sender != config.owner {
        return Err(StdError::generic_err(
            "only the original owner can finish poll",
        ));
    }

    // already finished
    if state.status != BetStatus::Voting {
        return Err(StdError::generic_err("already finished poll"));
    }

    // cannot finish before poll ends
    if env.block.time < Timestamp::from_seconds(config.bet_end_time) {
        return Err(StdError::generic_err(
            "Vote is live now, The poll cannot be finished before the end time",
        ));
    }

    let mut winning_sides: Vec<u64> = Vec::new();
    let mut count_max: u64 = 0;

    SIDES
        .range(deps.storage, None, None, Order::Ascending)
        .map(|item| {
            let (side_vec, count) = item.unwrap();
            let side_arr: [u8; 8] = side_vec.try_into().unwrap();
            (u64::from_be_bytes(side_arr), count)
        })
        .for_each(|(side, count)| match count_max.cmp(&count) {
            Ordering::Less => {
                winning_sides.clear();
                winning_sides.push(side);
                count_max = count;
            }
            Ordering::Equal => {
                winning_sides.push(side);
            }
            _ => {}
        });
    state.winning_side = Some(winning_sides);
    state.status = BetStatus::Closed;

    let mut cw20_msg = Cw20ExecuteMsg::Transfer {
        recipient: config.generator.to_string(),
        amount: state.deposit_amount,
    };
    if state.total_amount < config.reclaimable_threshold {
        // TODO : transfer 50% to the community fund
        cw20_msg = Cw20ExecuteMsg::Burn {
            amount: state.deposit_amount,
        };
    }
    state.deposit_reclaimed = true;
    store_state(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "finish_poll")
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.token_contract,
            msg: to_binary(&cw20_msg)?,
            funds: vec![],
        })))
}

pub fn reclaim_deposit(deps: DepsMut) -> StdResult<Response> {
    let config = read_config(deps.storage)?;
    let mut state = read_state(deps.storage)?;
    if state.deposit_reclaimed {
        return Err(StdError::generic_err("Already reclaimed".to_string()));
    }

    if state.total_amount < config.reclaimable_threshold {
        return Err(StdError::generic_err("Not enough total amount".to_string()));
    }

    state.deposit_reclaimed = true;
    store_state(deps.storage, &state)?;

    Ok(Response::new()
        .add_attribute("method", "try_reclaim_deposit")
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.token_contract.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: config.generator.to_string(),
                amount: state.deposit_amount,
            })?,
            funds: vec![],
        })))
}

// TODO : create update_config function
pub fn transfer_owner(deps: DepsMut, info: MessageInfo, new_owner: String) -> StdResult<Response> {
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