use std::rc::Rc;

use anchor_client::{Client, Program};
use anchor_lang::prelude::AccountMeta;
use raydium_amm_v3::{
    accounts::{
        ClosePosition as ClosePositionAccount, DecreaseLiquidityV2 as DecreaseLiquidityV2Accounts,
    },
    instruction::{
        ClosePosition as ClosePositionInstruction,
        DecreaseLiquidityV2 as DecreaseLiquidityV2Instruction,
    },
    libraries::{liquidity_math, tick_math},
    states::{POSITION_SEED, TICK_ARRAY_SEED},
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    instruction::Instruction, pubkey::Pubkey, signature::Keypair, signer::Signer, system_program,
    transaction::Transaction,
};
use spl_associated_token_account::get_associated_token_address;

use crate::send_txn;

use super::{
    position::get_all_nft_and_position_by_owner,
    utils::{amount_with_slippage, deserialize_anchor_account, get_pool_mints_transfer_fee, price_to_sqrt_price_x64, tick_with_spacing},
};

pub fn close(
    client: Rc<Client<Rc<Keypair>>>,
    rpc_client: RpcClient,
    payer: Keypair,
    raydium_v3_program: Pubkey,
    program: Program<Rc<Keypair>>,
    pool_id_account: Pubkey,
    tickarray_bitmap_extension: Pubkey,
    mint0: Pubkey,
    mint1: Pubkey,
    tick_lower_price: f64,
    tick_upper_price: f64,
    slippage: f64,
) {
    // let tick_lower_index = -tick_lower_index;
    // load pool to get observation
    let pool: raydium_amm_v3::states::PoolState = program.account(pool_id_account).unwrap();

    let tick_lower_price_x64 =
        price_to_sqrt_price_x64(tick_lower_price, pool.mint_decimals_0, pool.mint_decimals_1);
    let tick_upper_price_x64 =
        price_to_sqrt_price_x64(tick_upper_price, pool.mint_decimals_0, pool.mint_decimals_1);
    let tick_lower_index = tick_with_spacing(
        tick_math::get_tick_at_sqrt_price(tick_lower_price_x64).unwrap(),
        pool.tick_spacing.into(),
    );
    let tick_upper_index = tick_with_spacing(
        tick_math::get_tick_at_sqrt_price(tick_upper_price_x64).unwrap(),
        pool.tick_spacing.into(),
    );

    let tick_array_lower_start_index =
        raydium_amm_v3::states::TickArrayState::get_array_start_index(
            tick_lower_index,
            pool.tick_spacing.into(),
        );
    let tick_array_upper_start_index =
        raydium_amm_v3::states::TickArrayState::get_array_start_index(
            tick_upper_index,
            pool.tick_spacing.into(),
        );
    // load position
    let position_nft_infos =
        get_all_nft_and_position_by_owner(&rpc_client, &payer.pubkey(), &raydium_v3_program);
    let positions: Vec<Pubkey> = position_nft_infos
        .iter()
        .map(|item| item.position)
        .collect();
    let rsps = rpc_client.get_multiple_accounts(&positions).unwrap();
    let mut user_positions = Vec::new();
    for rsp in rsps {
        match rsp {
            None => continue,
            Some(rsp) => {
                let position = deserialize_anchor_account::<
                    raydium_amm_v3::states::PersonalPositionState,
                >(&rsp)
                .unwrap();
                user_positions.push(position);
            }
        }
    }
    let mut find_position = raydium_amm_v3::states::PersonalPositionState::default();
    for position in user_positions {
        if position.pool_id == pool_id_account
            && position.tick_lower_index == tick_lower_index
            && position.tick_upper_index == tick_upper_index
        {
            find_position = position.clone();
            println!("liquidity:{:?}", find_position);
        }
    }
    if find_position.nft_mint != Pubkey::default() && find_position.pool_id == pool_id_account {
        let user_nft_token_info = position_nft_infos
            .iter()
            .find(|&nft_info| nft_info.mint == find_position.nft_mint)
            .unwrap();
        let mut reward_vault_with_user_vault: Vec<Pubkey> = Vec::new();
        for item in pool.reward_infos.into_iter() {
            if item.token_mint != Pubkey::default() {
                reward_vault_with_user_vault.push(item.token_vault);
                reward_vault_with_user_vault.push(get_associated_token_address(
                    &payer.pubkey(),
                    &item.token_mint,
                ));
                reward_vault_with_user_vault.push(item.token_mint);
            }
        }
        let liquidity = find_position.liquidity;
        let (amount_0, amount_1) = liquidity_math::get_delta_amounts_signed(
            pool.tick_current,
            pool.sqrt_price_x64,
            tick_lower_index,
            tick_upper_index,
            -(liquidity as i128),
        )
        .unwrap();
        let amount_0_with_slippage = amount_with_slippage(amount_0, slippage, false);
        let amount_1_with_slippage = amount_with_slippage(amount_1, slippage, false);
        let transfer_fee = get_pool_mints_transfer_fee(
            &rpc_client,
            pool.token_mint_0,
            pool.token_mint_1,
            amount_0_with_slippage,
            amount_1_with_slippage,
        );
        let amount_0_min = amount_0_with_slippage
            .checked_sub(transfer_fee.0.transfer_fee)
            .unwrap();
        let amount_1_min = amount_1_with_slippage
            .checked_sub(transfer_fee.1.transfer_fee)
            .unwrap();

        let mut remaining_accounts = Vec::new();
        remaining_accounts.push(AccountMeta::new(tickarray_bitmap_extension, false));

        let mut accounts = reward_vault_with_user_vault
            .into_iter()
            .map(|item| AccountMeta::new(item, false))
            .collect();
        remaining_accounts.append(&mut accounts);
        // personal position exist
        let mut decrease_instr = decrease_liquidity_instr(
            client.clone(),
            raydium_v3_program,
            pool_id_account,
            pool.token_vault_0,
            pool.token_vault_1,
            pool.token_mint_0,
            pool.token_mint_1,
            find_position.nft_mint,
            user_nft_token_info.key,
            spl_associated_token_account::get_associated_token_address_with_program_id(
                &payer.pubkey(),
                &mint0,
                &transfer_fee.0.owner,
            ),
            spl_associated_token_account::get_associated_token_address_with_program_id(
                &payer.pubkey(),
                &mint1,
                &transfer_fee.1.owner,
            ),
            remaining_accounts,
            liquidity,
            amount_0_min,
            amount_1_min,
            tick_lower_index,
            tick_upper_index,
            tick_array_lower_start_index,
            tick_array_upper_start_index,
        );
        if liquidity == find_position.liquidity {
            let close_position_instr = close_personal_position_instr(
                client,
                raydium_v3_program,
                find_position.nft_mint,
                user_nft_token_info.key,
                user_nft_token_info.program,
            );
            decrease_instr.extend(close_position_instr);
        }
        // send
        let signers = vec![&payer];
        let recent_hash = rpc_client.get_latest_blockhash().unwrap();
        let txn = Transaction::new_signed_with_payer(
            &decrease_instr,
            Some(&payer.pubkey()),
            &signers,
            recent_hash,
        );

        let signature = send_txn(&rpc_client, &txn, true);
        println!("{}", signature);
    } else {
        // personal position not exist
        println!("personal position exist:{:?}", find_position);
    }
}

pub fn decrease_liquidity_instr(
    client: Rc<Client<Rc<Keypair>>>,
    raydium_v3_program: Pubkey,
    pool_account_key: Pubkey,
    token_vault_0: Pubkey,
    token_vault_1: Pubkey,
    token_mint_0: Pubkey,
    token_mint_1: Pubkey,
    nft_mint_key: Pubkey,
    nft_token_key: Pubkey,
    user_token_account_0: Pubkey,
    user_token_account_1: Pubkey,
    remaining_accounts: Vec<AccountMeta>,
    liquidity: u128,
    amount_0_min: u64,
    amount_1_min: u64,
    tick_lower_index: i32,
    tick_upper_index: i32,
    tick_array_lower_start_index: i32,
    tick_array_upper_start_index: i32,
) -> Vec<Instruction> {
    let program = client.program(raydium_v3_program).unwrap();
    let (personal_position_key, __bump) = Pubkey::find_program_address(
        &[POSITION_SEED.as_bytes(), nft_mint_key.to_bytes().as_ref()],
        &program.id(),
    );
    let (protocol_position_key, __bump) = Pubkey::find_program_address(
        &[
            POSITION_SEED.as_bytes(),
            pool_account_key.to_bytes().as_ref(),
            &tick_lower_index.to_be_bytes(),
            &tick_upper_index.to_be_bytes(),
        ],
        &program.id(),
    );
    let (tick_array_lower, __bump) = Pubkey::find_program_address(
        &[
            TICK_ARRAY_SEED.as_bytes(),
            pool_account_key.to_bytes().as_ref(),
            &tick_array_lower_start_index.to_be_bytes(),
        ],
        &program.id(),
    );
    let (tick_array_upper, __bump) = Pubkey::find_program_address(
        &[
            TICK_ARRAY_SEED.as_bytes(),
            pool_account_key.to_bytes().as_ref(),
            &tick_array_upper_start_index.to_be_bytes(),
        ],
        &program.id(),
    );
    let instructions = program
        .request()
        .accounts(DecreaseLiquidityV2Accounts {
            nft_owner: program.payer(),
            nft_account: nft_token_key,
            personal_position: personal_position_key,
            pool_state: pool_account_key,
            protocol_position: protocol_position_key,
            token_vault_0,
            token_vault_1,
            tick_array_lower,
            tick_array_upper,
            recipient_token_account_0: user_token_account_0,
            recipient_token_account_1: user_token_account_1,
            token_program: spl_token::id(),
            token_program_2022: spl_token_2022::id(),
            memo_program: spl_memo::id(),
            vault_0_mint: token_mint_0,
            vault_1_mint: token_mint_1,
        })
        .accounts(remaining_accounts)
        .args(DecreaseLiquidityV2Instruction {
            liquidity,
            amount_0_min,
            amount_1_min,
        })
        .instructions()
        .unwrap();
    instructions
}

pub fn close_personal_position_instr(
    client: Rc<Client<Rc<Keypair>>>,
    raydium_v3_program: Pubkey,
    nft_mint_key: Pubkey,
    nft_token_key: Pubkey,
    nft_token_program: Pubkey,
) -> Vec<Instruction> {
    let program = client.program(raydium_v3_program).unwrap();
    let (personal_position_key, __bump) = Pubkey::find_program_address(
        &[POSITION_SEED.as_bytes(), nft_mint_key.to_bytes().as_ref()],
        &program.id(),
    );
    let instructions = program
        .request()
        .accounts(ClosePositionAccount {
            nft_owner: program.payer(),
            position_nft_mint: nft_mint_key,
            position_nft_account: nft_token_key,
            personal_position: personal_position_key,
            system_program: system_program::ID,
            token_program: nft_token_program,
        })
        .args(ClosePositionInstruction)
        .instructions()
        .unwrap();
    instructions
}
