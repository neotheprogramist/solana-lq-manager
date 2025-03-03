use std::rc::Rc;

use anchor_client::{Client, Program};
use anchor_lang::prelude::AccountMeta;
use rand::rngs::OsRng;
use raydium_amm_v3::{
    accounts::OpenPositionWithToken22Nft as OpenPositionWithToken22NftAccounts,
    instruction::OpenPositionWithToken22Nft as OpenPositionWithToken22NftInstruction,
    libraries::{liquidity_math, tick_math},
    states::{POSITION_SEED, TICK_ARRAY_SEED},
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction, instruction::Instruction, pubkey::Pubkey,
    signature::Keypair, signer::Signer, system_program, sysvar, transaction::Transaction,
};

use crate::{
    raydium::{
        position::get_all_nft_and_position_by_owner,
        utils::{
            amount_with_slippage, deserialize_anchor_account, get_pool_mints_inverse_fee,
            price_to_sqrt_price_x64, tick_with_spacing,
        },
    },
    send_txn,
};

pub fn open(
    client: Client<Rc<Keypair>>,
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
    is_base_0: bool,
    input_amount: u64,
    slippage: f64,
) {
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
    println!(
        "tick_lower_index:{}, tick_upper_index:{}",
        tick_lower_index, tick_upper_index
    );
    let tick_lower_price_x64 = tick_math::get_sqrt_price_at_tick(tick_lower_index).unwrap();
    let tick_upper_price_x64 = tick_math::get_sqrt_price_at_tick(tick_upper_index).unwrap();
    let liquidity = if is_base_0 {
        liquidity_math::get_liquidity_from_single_amount_0(
            pool.sqrt_price_x64,
            tick_lower_price_x64,
            tick_upper_price_x64,
            input_amount,
        )
    } else {
        liquidity_math::get_liquidity_from_single_amount_1(
            pool.sqrt_price_x64,
            tick_lower_price_x64,
            tick_upper_price_x64,
            input_amount,
        )
    };
    let (amount_0, amount_1) = liquidity_math::get_delta_amounts_signed(
        pool.tick_current,
        pool.sqrt_price_x64,
        tick_lower_index,
        tick_upper_index,
        liquidity as i128,
    )
    .unwrap();
    println!(
        "amount_0:{}, amount_1:{}, liquidity:{}",
        amount_0, amount_1, liquidity
    );
    // calc with slippage
    let amount_0_with_slippage = amount_with_slippage(amount_0, slippage, true);
    let amount_1_with_slippage = amount_with_slippage(amount_1, slippage, true);
    // calc with transfer_fee
    let transfer_fee = get_pool_mints_inverse_fee(
        &rpc_client,
        pool.token_mint_0,
        pool.token_mint_1,
        amount_0_with_slippage,
        amount_1_with_slippage,
    );
    println!(
        "transfer_fee_0:{}, transfer_fee_1:{}",
        transfer_fee.0.transfer_fee, transfer_fee.1.transfer_fee
    );
    let amount_0_max = (amount_0_with_slippage as u64)
        .checked_add(transfer_fee.0.transfer_fee)
        .unwrap();
    let amount_1_max = (amount_1_with_slippage as u64)
        .checked_add(transfer_fee.1.transfer_fee)
        .unwrap();

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
        }
    }
    if find_position.nft_mint == Pubkey::default() {
        // personal position not exist
        // new nft mint
        let nft_mint = Keypair::generate(&mut OsRng);
        let mut remaining_accounts = Vec::new();
        remaining_accounts.push(AccountMeta::new(tickarray_bitmap_extension, false));

        let mut instructions = Vec::new();
        let request_inits_instr = ComputeBudgetInstruction::set_compute_unit_limit(1400_000u32);
        instructions.push(request_inits_instr);
        let open_position_instr = open_position_with_token22_nft_instr(
            client,
            raydium_v3_program,
            pool_id_account,
            pool.token_vault_0,
            pool.token_vault_1,
            pool.token_mint_0,
            pool.token_mint_1,
            nft_mint.pubkey(),
            payer.pubkey(),
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
            amount_0_max,
            amount_1_max,
            tick_lower_index,
            tick_upper_index,
            tick_array_lower_start_index,
            tick_array_upper_start_index,
            false,
        );
        instructions.extend(open_position_instr);
        // send
        let signers = vec![&payer, &nft_mint];
        let recent_hash = rpc_client.get_latest_blockhash().unwrap();
        let txn = Transaction::new_signed_with_payer(
            &instructions,
            Some(&payer.pubkey()),
            &signers,
            recent_hash,
        );
        let signature = send_txn(&rpc_client, &txn, true);
        println!("{}", signature);
    } else {
        // personal position exist
        println!("personal position exist:{:?}", find_position);
    }
}

pub fn open_position_with_token22_nft_instr(
    client: Client<Rc<Keypair>>,
    raydium_v3_program: Pubkey,
    pool_account_key: Pubkey,
    token_vault_0: Pubkey,
    token_vault_1: Pubkey,
    token_mint_0: Pubkey,
    token_mint_1: Pubkey,
    nft_mint_key: Pubkey,
    nft_to_owner: Pubkey,
    user_token_account_0: Pubkey,
    user_token_account_1: Pubkey,
    remaining_accounts: Vec<AccountMeta>,
    liquidity: u128,
    amount_0_max: u64,
    amount_1_max: u64,
    tick_lower_index: i32,
    tick_upper_index: i32,
    tick_array_lower_start_index: i32,
    tick_array_upper_start_index: i32,
    with_metadata: bool,
) -> Vec<Instruction> {
    let program = client.program(raydium_v3_program).unwrap();
    let nft_ata_token_account =
        spl_associated_token_account::get_associated_token_address_with_program_id(
            &program.payer(),
            &nft_mint_key,
            &spl_token_2022::id(),
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
    let (personal_position_key, __bump) = Pubkey::find_program_address(
        &[POSITION_SEED.as_bytes(), nft_mint_key.to_bytes().as_ref()],
        &program.id(),
    );
    let instructions = program
        .request()
        .accounts(OpenPositionWithToken22NftAccounts {
            payer: program.payer(),
            position_nft_owner: nft_to_owner,
            position_nft_mint: nft_mint_key,
            position_nft_account: nft_ata_token_account,
            pool_state: pool_account_key,
            protocol_position: protocol_position_key,
            tick_array_lower,
            tick_array_upper,
            personal_position: personal_position_key,
            token_account_0: user_token_account_0,
            token_account_1: user_token_account_1,
            token_vault_0,
            token_vault_1,
            rent: sysvar::rent::id(),
            system_program: system_program::id(),
            token_program: spl_token::id(),
            associated_token_program: spl_associated_token_account::id(),
            token_program_2022: spl_token_2022::id(),
            vault_0_mint: token_mint_0,
            vault_1_mint: token_mint_1,
        })
        .accounts(remaining_accounts)
        .args(OpenPositionWithToken22NftInstruction {
            liquidity,
            amount_0_max,
            amount_1_max,
            tick_lower_index,
            tick_upper_index,
            tick_array_lower_start_index,
            tick_array_upper_start_index,
            with_metadata,
            base_flag: None,
        })
        .instructions()
        .unwrap();
    instructions
}
