use std::{
    env,
    path::{Path, PathBuf},
    rc::Rc,
};

use anchor_client::{Client, Cluster};
use clap::Parser;
use solana_client::{rpc_client::RpcClient, rpc_config::RpcSendTransactionConfig};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::{Keypair, Signature},
    transaction::Transaction,
};

pub mod cli;
pub mod error;
pub mod raydium;
pub mod save;

fn main() {
    let args = cli::Cli::parse();

    // solana rpc client
    let rpc_client = RpcClient::new(&args.http_url);

    // anchor client.
    let url = Cluster::Custom(args.http_url.to_string(), args.ws_url.to_string());
    let payer = read_keypair_file(&args.payer_path);
    let anchor_client = Client::new(url, Rc::new(read_keypair_file(&args.payer_path)));

    match args.command {
        cli::CommandsName::OpenPosition { .. } | cli::CommandsName::ClosePosition { .. } => {
            raydium::run(&args, rpc_client, anchor_client, payer)
        }
        cli::CommandsName::Deposit { .. } | cli::CommandsName::Withdraw => {
            save::run(&args, rpc_client, anchor_client, payer);
        }
    }
}

fn expand_home_dir(path: &Path) -> PathBuf {
    if let Some(str_path) = path.to_str() {
        if str_path.starts_with('~') {
            if let Ok(home) = env::var("HOME") {
                return Path::new(&home).join(str_path.trim_start_matches('~'));
            }
        }
    }
    path.to_path_buf()
}

fn read_keypair_file(path: &Path) -> Keypair {
    let expanded_path = expand_home_dir(path);
    let mut file = std::fs::File::open(expanded_path).unwrap();
    solana_sdk::signature::read_keypair(&mut file).unwrap()
}

pub fn send_txn(client: &RpcClient, txn: &Transaction, wait_confirm: bool) -> Signature {
    client
        .send_and_confirm_transaction_with_spinner_and_config(
            txn,
            if wait_confirm {
                CommitmentConfig::confirmed()
            } else {
                CommitmentConfig::processed()
            },
            RpcSendTransactionConfig {
                skip_preflight: true,
                ..RpcSendTransactionConfig::default()
            },
        )
        .unwrap()
}
