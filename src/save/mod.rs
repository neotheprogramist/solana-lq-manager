use std::rc::Rc;

use anchor_client::Client;
use raydium_amm_v3::states::POOL_TICK_ARRAY_BITMAP_SEED;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Keypair};

use crate::cli::{self, Cli};

pub mod deposit;
pub mod withdraw;

pub fn run(args: &Cli, rpc_client: RpcClient, anchor_client: Client<Rc<Keypair>>, payer: Keypair) {
    let program = anchor_client.program(args.save_program).unwrap();

    match args.command {
        cli::CommandsName::Deposit { input_amount } => {}
        cli::CommandsName::Withdraw => {}
        _ => panic!("unhandled"),
    }
}
