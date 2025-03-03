use std::path::PathBuf;

use clap::Parser;
use solana_client::client_error::reqwest::Url;
use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Parser)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: CommandsName,

    #[arg(long, env)]
    pub http_url: Url,

    #[arg(long, env)]
    pub ws_url: Url,

    #[arg(long, env)]
    pub payer_path: PathBuf,

    #[arg(long, env)]
    pub raydium_v3_program: Pubkey,

    #[arg(long, env)]
    pub slippage: f64,

    #[arg(long, env)]
    pub mint0: Pubkey,

    #[arg(long, env)]
    pub mint1: Pubkey,

    #[arg(long, env)]
    pub amm_config_index: u16,

    #[arg(long, env)]
    pub save_program: Pubkey,
}
#[derive(Debug, Parser)]
pub enum CommandsName {
    OpenPosition {
        tick_lower_price: f64,
        tick_upper_price: f64,
        #[arg(short, long)]
        is_base_0: bool,
        input_amount: u64,
    },
    ClosePosition {
        tick_lower_index: f64,
        tick_upper_index: f64,
    },

    Deposit {
        input_amount: u64,
    },
    Withdraw,
}
