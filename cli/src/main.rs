use std::{fs, path::PathBuf, str::FromStr};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use slipstream_cli::{
    build_swap_ix, derive_router_authority, derive_transient_stake, SwapAccounts, SwapConfig,
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signer::{keypair::read_keypair_file, Signer},
    transaction::Transaction,
};

#[derive(Parser, Debug)]
#[command(
    name = "slipstream",
    about = "Build & submit stake-pool routed LST swaps",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Build and submit a Swap transaction.
    Swap {
        /// Path to a JSON file containing the pool/account addresses (see
        /// `slipstream example-config` for a template).
        #[arg(long)]
        config: PathBuf,

        /// Amount of LST_A to burn.
        #[arg(long)]
        amount_in: u64,

        /// Minimum LST_B to receive. Tx reverts if `received < min_out`.
        #[arg(long)]
        min_out: u64,

        /// PDA nonce. Lets a caller route concurrent swaps without colliding
        /// on the per-user transient stake PDA.
        #[arg(long, default_value_t = 0)]
        nonce: u64,

        /// JSON-RPC endpoint. Defaults to local validator.
        #[arg(long, default_value = "http://127.0.0.1:8899")]
        rpc_url: String,

        /// Fee-payer / user keypair path.
        #[arg(long, default_value = "~/.config/solana/id.json")]
        keypair: String,

        /// Print the instruction and exit without submitting.
        #[arg(long)]
        dry_run: bool,
    },

    /// Print the transient stake + router authority PDAs for a given
    /// program-id / user / nonce. Useful for funding the transient stake
    /// before submission or for off-chain pre-flight checks.
    Derive {
        #[arg(long)]
        program_id: String,
        #[arg(long)]
        user: String,
        #[arg(long, default_value_t = 0)]
        nonce: u64,
    },

    /// Print an empty config template to stdout.
    ExampleConfig,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Swap {
            config,
            amount_in,
            min_out,
            nonce,
            rpc_url,
            keypair,
            dry_run,
        } => run_swap(config, amount_in, min_out, nonce, rpc_url, keypair, dry_run),
        Command::Derive {
            program_id,
            user,
            nonce,
        } => run_derive(program_id, user, nonce),
        Command::ExampleConfig => {
            print!("{}", EXAMPLE_CONFIG);
            Ok(())
        }
    }
}

fn run_swap(
    config_path: PathBuf,
    amount_in: u64,
    min_out: u64,
    nonce: u64,
    rpc_url: String,
    keypair_path: String,
    dry_run: bool,
) -> Result<()> {
    let raw = fs::read_to_string(&config_path)
        .with_context(|| format!("read config: {}", config_path.display()))?;
    let cfg: SwapConfig = serde_json::from_str(&raw).context("parse config JSON")?;

    let keypair_path = expand_tilde(&keypair_path);
    let payer = read_keypair_file(&keypair_path)
        .map_err(|e| anyhow!("read keypair {}: {e}", keypair_path))?;

    let accounts = SwapAccounts::from_config(&cfg, payer.pubkey(), nonce)?;
    let ix = build_swap_ix(&accounts, amount_in, min_out, nonce);

    println!("program_id:        {}", accounts.program_id);
    println!("user:              {}", accounts.user);
    println!("transient_stake:   {}", accounts.transient_stake);
    println!("router_authority:  {}", accounts.router_authority);
    println!("amount_in:         {amount_in}");
    println!("min_amount_out:    {min_out}");
    println!("nonce:             {nonce}");
    println!("accounts:          {}", ix.accounts.len());
    println!("data bytes:        {}", ix.data.len());

    if dry_run {
        println!("(dry run — not submitting)");
        return Ok(());
    }

    let rpc = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());
    let blockhash = rpc
        .get_latest_blockhash()
        .context("fetch latest blockhash")?;
    let tx = Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer], blockhash);

    let sig = rpc
        .send_and_confirm_transaction(&tx)
        .context("send_and_confirm_transaction")?;
    println!("\nsignature: {sig}");
    Ok(())
}

fn run_derive(program_id: String, user: String, nonce: u64) -> Result<()> {
    let program_id = Pubkey::from_str(&program_id).context("--program-id")?;
    let user = Pubkey::from_str(&user).context("--user")?;
    let (transient, t_bump) = derive_transient_stake(&program_id, &user, nonce);
    let (router, r_bump) = derive_router_authority(&program_id);
    println!("transient_stake:  {transient}  (bump {t_bump})");
    println!("router_authority: {router}  (bump {r_bump})");
    Ok(())
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}/{rest}", home.to_string_lossy());
        }
    }
    path.to_string()
}

const EXAMPLE_CONFIG: &str = r#"{
  "program_id": "SL1p2N8iNBBo3uaUF92SGo8VfCkN6Xqdmq7tUTqz6cd",
  "user_lst_a": "<bSOL token account owned by user>",
  "user_lst_b": "<JitoSOL token account owned by user>",
  "pool_a": {
    "program":            "<SPL stake-pool program id for pool A>",
    "address":            "<pool A state account>",
    "validator_list":     "<pool A validator list account>",
    "withdraw_authority": "<pool A withdraw authority PDA>",
    "validator_stake":    "<pool A validator-stake PDA for the chosen validator V>",
    "manager_fee":        "<pool A manager fee token account>",
    "mint":               "<bSOL mint>"
  },
  "pool_b": {
    "program":            "<SPL stake-pool program id for pool B>",
    "address":            "<pool B state account>",
    "validator_list":     "<pool B validator list account>",
    "deposit_authority":  "<pool B deposit authority (PDA or custom)>",
    "withdraw_authority": "<pool B withdraw authority PDA>",
    "validator_stake":    "<pool B validator-stake PDA for the same V>",
    "reserve_stake":      "<pool B reserve stake account>",
    "manager_fee":        "<pool B manager fee token account>",
    "referral_fee":       "<pool B referral fee token account (can equal manager_fee)>",
    "mint":               "<JitoSOL mint>"
  }
}
"#;
