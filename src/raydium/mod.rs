use std::rc::Rc;

use anchor_client::Client;
use raydium_amm_v3::states::POOL_TICK_ARRAY_BITMAP_SEED;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Keypair};

use crate::cli::{self, Cli};

pub mod close;
pub mod open;
pub mod position;
pub mod utils;

pub fn run(args: &Cli, rpc_client: RpcClient, anchor_client: Client<Rc<Keypair>>, payer: Keypair) {
    let program = anchor_client.program(args.raydium_v3_program).unwrap();

    let pool_id_account = {
        let mut mint0 = args.mint0;
        let mut mint1 = args.mint1;
        if mint0 > mint1 {
            let temp_mint = mint0;
            mint0 = mint1;
            mint1 = temp_mint;
        }

        let (amm_config_key, __bump) = Pubkey::find_program_address(
            &[
                raydium_amm_v3::states::AMM_CONFIG_SEED.as_bytes(),
                &args.amm_config_index.to_be_bytes(),
            ],
            &args.raydium_v3_program,
        );

        Pubkey::find_program_address(
            &[
                raydium_amm_v3::states::POOL_SEED.as_bytes(),
                amm_config_key.to_bytes().as_ref(),
                mint0.to_bytes().as_ref(),
                mint1.to_bytes().as_ref(),
            ],
            &args.raydium_v3_program,
        )
        .0
    };

    let tickarray_bitmap_extension = Pubkey::find_program_address(
        &[
            POOL_TICK_ARRAY_BITMAP_SEED.as_bytes(),
            pool_id_account.to_bytes().as_ref(),
        ],
        &args.raydium_v3_program,
    )
    .0;

    match args.command {
        cli::CommandsName::OpenPosition {
            tick_lower_price,
            tick_upper_price,
            is_base_0,
            input_amount,
        } => {
            open::open(
                anchor_client,
                rpc_client,
                payer,
                args.raydium_v3_program,
                program,
                pool_id_account,
                tickarray_bitmap_extension,
                args.mint0,
                args.mint1,
                tick_lower_price,
                tick_upper_price,
                is_base_0,
                input_amount,
                args.slippage,
            );
        }
        cli::CommandsName::ClosePosition {
            tick_lower_index,
            tick_upper_index,
        } => {
            close::close(
                Rc::new(anchor_client),
                rpc_client,
                payer,
                args.raydium_v3_program,
                program,
                pool_id_account,
                tickarray_bitmap_extension,
                args.mint0,
                args.mint1,
                tick_lower_index,
                tick_upper_index,
                args.slippage,
            );
        }
        _ => panic!("unhandled"),
    }
}
