use std::{fs, path::PathBuf, str::FromStr};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use slipstream_cli::{
    build_swap_ix, build_swap_via_interceptor_ix, derive_router_authority, derive_swap_config,
    derive_transient_stake, DeriveInputs, InterceptorSwapAccounts, SwapAccounts,
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::Keypair,
    signer::{keypair::read_keypair_file, Signer},
    transaction::Transaction,
};

const DEFAULT_PROGRAM_ID: &str = "SL1p2N8iNBBo3uaUF92SGo8VfCkN6Xqdmq7tUTqz6cd";
const DEFAULT_RPC_URL: &str = "http://127.0.0.1:8899";
const DEFAULT_KEYPAIR: &str = "~/.config/solana/id.json";

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
    /// Fetch both pool states from RPC, derive every account, and submit the
    /// Swap transaction.
    Swap {
        /// Source stake-pool state account.
        #[arg(long)]
        pool_a: String,

        /// Destination stake-pool state account.
        #[arg(long)]
        pool_b: String,

        /// Validator vote account that anchors the routed stake. Must appear
        /// in both pools' validator lists.
        #[arg(long)]
        validator_vote: String,

        /// Amount of LST_A to burn.
        #[arg(long)]
        amount_in: u64,

        /// Minimum LST_B to receive. Tx reverts if `received < min_out`.
        #[arg(long)]
        min_out: u64,

        /// PDA nonce — picks a fresh transient stake PDA per swap. Bump if a
        /// prior swap left state behind.
        #[arg(long, default_value_t = 0)]
        nonce: u64,

        /// Slipstream program id.
        #[arg(long, default_value = DEFAULT_PROGRAM_ID)]
        program_id: String,

        /// Override pool A's stake-pool program. Defaults to canonical SPL
        /// stake-pool program (works for JitoSOL, bSOL, Sanctum LSTs).
        #[arg(long)]
        pool_a_program: Option<String>,

        /// Override pool B's stake-pool program.
        #[arg(long)]
        pool_b_program: Option<String>,

        /// JSON-RPC endpoint.
        #[arg(long, default_value = DEFAULT_RPC_URL)]
        rpc_url: String,

        /// Fee-payer / user keypair path.
        #[arg(long, default_value = DEFAULT_KEYPAIR)]
        keypair: String,

        /// Interceptor vault token account. Required when pool B's deposit
        /// authority is owned by the interceptor program (e.g., JitoSOL).
        #[arg(long)]
        vault: Option<String>,

        /// Print the resolved accounts and instruction without submitting.
        #[arg(long)]
        dry_run: bool,
    },

    /// Print the transient-stake + router-authority PDAs for a given
    /// program-id / user / nonce.
    Derive {
        #[arg(long, default_value = DEFAULT_PROGRAM_ID)]
        program_id: String,
        #[arg(long)]
        user: String,
        #[arg(long, default_value_t = 0)]
        nonce: u64,
    },

    /// Fetch both pool states from RPC and print the resolved config JSON
    /// (handy for inspection / sharing routes without submitting a tx).
    DeriveConfig {
        #[arg(long, default_value = DEFAULT_PROGRAM_ID)]
        program_id: String,
        #[arg(long)]
        pool_a: String,
        #[arg(long)]
        pool_b: String,
        #[arg(long)]
        validator_vote: String,
        #[arg(long)]
        user: String,
        #[arg(long)]
        pool_a_program: Option<String>,
        #[arg(long)]
        pool_b_program: Option<String>,
        #[arg(long)]
        vault: Option<String>,
        #[arg(long, default_value = DEFAULT_RPC_URL)]
        rpc_url: String,
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Swap {
            pool_a,
            pool_b,
            validator_vote,
            amount_in,
            min_out,
            nonce,
            program_id,
            pool_a_program,
            pool_b_program,
            rpc_url,
            keypair,
            vault,
            dry_run,
        } => run_swap(SwapArgs {
            pool_a,
            pool_b,
            validator_vote,
            amount_in,
            min_out,
            nonce,
            program_id,
            pool_a_program,
            pool_b_program,
            rpc_url,
            keypair,
            vault,
            dry_run,
        }),
        Command::Derive {
            program_id,
            user,
            nonce,
        } => run_derive(program_id, user, nonce),
        Command::DeriveConfig {
            program_id,
            pool_a,
            pool_b,
            validator_vote,
            user,
            pool_a_program,
            pool_b_program,
            vault,
            rpc_url,
            out,
        } => run_derive_config(
            program_id,
            pool_a,
            pool_b,
            validator_vote,
            user,
            pool_a_program,
            pool_b_program,
            vault,
            rpc_url,
            out,
        ),
    }
}

struct SwapArgs {
    pool_a: String,
    pool_b: String,
    validator_vote: String,
    amount_in: u64,
    min_out: u64,
    nonce: u64,
    program_id: String,
    pool_a_program: Option<String>,
    pool_b_program: Option<String>,
    rpc_url: String,
    keypair: String,
    vault: Option<String>,
    dry_run: bool,
}

fn run_swap(args: SwapArgs) -> Result<()> {
    let keypair_path = expand_tilde(&args.keypair);
    let payer = read_keypair_file(&keypair_path)
        .map_err(|e| anyhow!("read keypair {}: {e}", keypair_path))?;

    let inputs = DeriveInputs {
        slipstream_program_id: Pubkey::from_str(&args.program_id).context("--program-id")?,
        pool_a: Pubkey::from_str(&args.pool_a).context("--pool-a")?,
        pool_b: Pubkey::from_str(&args.pool_b).context("--pool-b")?,
        validator_vote: Pubkey::from_str(&args.validator_vote).context("--validator-vote")?,
        user: payer.pubkey(),
        pool_a_program: parse_opt_pubkey("--pool-a-program", args.pool_a_program)?,
        pool_b_program: parse_opt_pubkey("--pool-b-program", args.pool_b_program)?,
        vault: parse_opt_pubkey("--vault", args.vault)?,
    };

    let rpc = RpcClient::new_with_commitment(args.rpc_url, CommitmentConfig::confirmed());
    let cfg = derive_swap_config(&rpc, &inputs)?;
    let accounts = SwapAccounts::from_config(&cfg, payer.pubkey(), args.nonce)?;

    // Interceptor mode requires an ephemeral `base` keypair as a second
    // signer (seeds the DepositReceipt PDA).
    let (ix, base_keypair) = if cfg.interceptor.is_some() {
        let base = Keypair::new();
        let interceptor_accounts =
            InterceptorSwapAccounts::from_config(&cfg, payer.pubkey(), args.nonce, base.pubkey())?;
        let ix = build_swap_via_interceptor_ix(
            &interceptor_accounts,
            args.amount_in,
            args.min_out,
            args.nonce,
        );
        println!("(interceptor mode — pool B requires the interceptor deposit flow)");
        println!(
            "deposit_receipt:   {}",
            interceptor_accounts.deposit_receipt
        );
        println!("base:              {}", base.pubkey());
        (ix, Some(base))
    } else {
        (
            build_swap_ix(&accounts, args.amount_in, args.min_out, args.nonce),
            None,
        )
    };

    print_resolution(&accounts, &cfg, args.amount_in, args.min_out, args.nonce);

    if args.dry_run {
        println!("(dry run — not submitting)");
        return Ok(());
    }

    let blockhash = rpc
        .get_latest_blockhash()
        .context("fetch latest blockhash")?;
    let tx = if let Some(base) = base_keypair.as_ref() {
        Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer, base], blockhash)
    } else {
        Transaction::new_signed_with_payer(&[ix], Some(&payer.pubkey()), &[&payer], blockhash)
    };
    let sig = rpc
        .send_and_confirm_transaction(&tx)
        .context("send_and_confirm_transaction")?;
    println!("\nsignature: {sig}");
    Ok(())
}

fn print_resolution(
    accounts: &SwapAccounts,
    cfg: &slipstream_cli::SwapConfig,
    amount_in: u64,
    min_out: u64,
    nonce: u64,
) {
    println!("program_id:        {}", accounts.program_id);
    println!("user:              {}", accounts.user);
    println!("transient_stake:   {}", accounts.transient_stake);
    println!("router_authority:  {}", accounts.router_authority);
    println!("user_lst_a (ATA):  {}", accounts.user_lst_a);
    println!("user_lst_b (ATA):  {}", accounts.user_lst_b);
    println!("pool A mint:       {}", cfg.pool_a.mint);
    println!("pool B mint:       {}", cfg.pool_b.mint);
    println!("pool A validator:  {}", accounts.pool_a_validator_stake);
    println!("pool B validator:  {}", accounts.pool_b_validator_stake);
    println!("pool B deposit auth:{}", accounts.pool_b_deposit_authority);
    println!("amount_in:         {amount_in}");
    println!("min_amount_out:    {min_out}");
    println!("nonce:             {nonce}");
}

#[allow(clippy::too_many_arguments)]
fn run_derive_config(
    program_id: String,
    pool_a: String,
    pool_b: String,
    validator_vote: String,
    user: String,
    pool_a_program: Option<String>,
    pool_b_program: Option<String>,
    vault: Option<String>,
    rpc_url: String,
    out: Option<PathBuf>,
) -> Result<()> {
    let inputs = DeriveInputs {
        slipstream_program_id: Pubkey::from_str(&program_id).context("--program-id")?,
        pool_a: Pubkey::from_str(&pool_a).context("--pool-a")?,
        pool_b: Pubkey::from_str(&pool_b).context("--pool-b")?,
        validator_vote: Pubkey::from_str(&validator_vote).context("--validator-vote")?,
        user: Pubkey::from_str(&user).context("--user")?,
        pool_a_program: parse_opt_pubkey("--pool-a-program", pool_a_program)?,
        pool_b_program: parse_opt_pubkey("--pool-b-program", pool_b_program)?,
        vault: parse_opt_pubkey("--vault", vault)?,
    };

    let rpc = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());
    let cfg = derive_swap_config(&rpc, &inputs)?;
    let json = serde_json::to_string_pretty(&cfg)?;
    match out {
        Some(path) => {
            fs::write(&path, &json).with_context(|| format!("write {}", path.display()))?;
            eprintln!("wrote {}", path.display());
        }
        None => println!("{json}"),
    }
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

fn parse_opt_pubkey(field: &str, s: Option<String>) -> Result<Option<Pubkey>> {
    s.map(|v| Pubkey::from_str(&v).with_context(|| field.to_string()))
        .transpose()
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}/{rest}", home.to_string_lossy());
        }
    }
    path.to_string()
}
