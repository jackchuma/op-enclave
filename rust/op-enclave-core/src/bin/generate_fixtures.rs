//! Fixture generator for stateless execution integration tests.
//!
//! This binary generates test fixtures from Base Sepolia testnet by fetching
//! real block data, execution witnesses, and expected roots.
//!
//! # Usage
//!
//! ```bash
//! cargo run --bin generate_fixtures -- \
//!   --l2-rpc https://sepolia.base.org \
//!   --l1-rpc https://sepolia.drpc.org \
//!   --block 12345 \
//!   --output tests/fixtures/base_sepolia_12345.json
//! ```
//!
//! # Requirements
//!
//! - Access to Base Sepolia L2 RPC endpoint
//! - Access to Sepolia L1 RPC endpoint
//! - The L2 node must support `debug_executionWitness` RPC method

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use alloy_consensus::Header;
use alloy_primitives::{Address, B256, Bytes};
use clap::Parser;
use op_alloy_consensus::OpReceiptEnvelope;
use serde::{Deserialize, Serialize};

use op_enclave_core::executor::ExecutionWitness;
use op_enclave_core::types::account::AccountResult;
use op_enclave_core::L1ChainConfig;

/// Command-line arguments for the fixture generator.
#[derive(Parser, Debug)]
#[command(name = "generate_fixtures")]
#[command(about = "Generate test fixtures from Base Sepolia testnet")]
struct Args {
    /// L2 RPC endpoint URL (e.g., https://sepolia.base.org)
    #[arg(long)]
    l2_rpc: String,

    /// L1 RPC endpoint URL (e.g., https://sepolia.drpc.org)
    #[arg(long)]
    l1_rpc: String,

    /// L2 block number to generate fixture for
    #[arg(long)]
    block: u64,

    /// Output file path (defaults to stdout if not specified)
    #[arg(long, short)]
    output: Option<PathBuf>,

    /// Pretty print JSON output
    #[arg(long, default_value = "true")]
    pretty: bool,
}

/// Complete test fixture for stateless execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatelessTestFixture {
    /// The rollup configuration.
    pub rollup_config: kona_genesis::RollupConfig,

    /// The L1 chain configuration.
    pub l1_config: L1ChainConfig,

    /// The L1 origin block header.
    pub l1_origin: Header,

    /// The L1 origin block receipts.
    pub l1_receipts: Vec<OpReceiptEnvelope>,

    /// Transactions from the previous L2 block (RLP-encoded).
    pub previous_block_txs: Vec<Bytes>,

    /// The L2 block header to validate.
    pub block_header: Header,

    /// Sequenced transactions for this block (RLP-encoded).
    pub sequenced_txs: Vec<Bytes>,

    /// The execution witness.
    pub witness: ExecutionWitness,

    /// The L2ToL1MessagePasser account proof.
    pub message_account: AccountResult,

    /// Expected state root after execution.
    pub expected_state_root: B256,

    /// Expected receipts root after execution.
    pub expected_receipts_root: B256,
}

/// RPC response wrapper for JSON-RPC calls.
#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: u64,
    result: Option<T>,
    error: Option<RpcError>,
}

/// RPC error response.
#[derive(Debug, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

/// L2 block response from RPC.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct L2BlockResponse {
    hash: B256,
    parent_hash: B256,
    number: String,
    timestamp: String,
    state_root: B256,
    receipts_root: B256,
    transactions_root: B256,
    transactions: Vec<Bytes>,
}

/// Execution witness response from debug_executionWitness RPC.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ExecutionWitnessResponse {
    headers: Vec<Header>,
    codes: HashMap<String, String>,
    state: HashMap<String, String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let block = args.block;
    eprintln!("Generating fixture for block {block} ...");
    eprintln!("L2 RPC: {}", args.l2_rpc);
    eprintln!("L1 RPC: {}", args.l1_rpc);

    // Create HTTP client
    let client = reqwest::Client::new();

    // 1. Fetch L2 block header
    eprintln!("Fetching L2 block {block}...");
    let block_response = fetch_l2_block(&client, &args.l2_rpc, block).await?;
    let block_hash = block_response.hash;
    eprintln!("  Block hash: {block_hash:?}");

    // 2. Fetch previous L2 block
    let prev_block = block - 1;
    eprintln!("Fetching previous L2 block {prev_block}...");
    let prev_block_response = fetch_l2_block(&client, &args.l2_rpc, prev_block).await?;
    let prev_hash = prev_block_response.hash;
    eprintln!("  Previous block hash: {prev_hash:?}");

    // 3. Fetch execution witness using debug_executionWitness
    eprintln!("Fetching execution witness...");
    let witness = fetch_execution_witness(&client, &args.l2_rpc, block).await?;
    let codes_len = witness.codes.len();
    let state_len = witness.state.len();
    eprintln!("  Codes: {codes_len}, State nodes: {state_len}");

    // 4. Get L1 origin from L1 info deposit tx in previous block
    eprintln!("Extracting L1 origin from previous block...");
    let l1_origin_hash = extract_l1_origin_hash(&prev_block_response)?;
    eprintln!("  L1 origin hash: {l1_origin_hash:?}");

    // 5. Fetch L1 origin block and receipts
    eprintln!("Fetching L1 origin block...");
    let l1_origin = fetch_l1_block(&client, &args.l1_rpc, l1_origin_hash).await?;
    let l1_number = l1_origin.number;
    eprintln!("  L1 block number: {l1_number}");

    eprintln!("Fetching L1 receipts...");
    let l1_receipts = fetch_l1_receipts(&client, &args.l1_rpc, l1_origin_hash).await?;
    let receipts_len = l1_receipts.len();
    eprintln!("  L1 receipts: {receipts_len}");

    // 6. Fetch message account proof
    eprintln!("Fetching message account proof...");
    let message_passer_address = "0x4200000000000000000000000000000000000016".parse::<Address>()?;
    let message_account = fetch_account_proof(
        &client,
        &args.l2_rpc,
        message_passer_address,
        block_response.state_root,
        block,
    ).await?;

    // 7. Build the fixture
    let fixture = StatelessTestFixture {
        rollup_config: get_base_sepolia_rollup_config(),
        l1_config: get_sepolia_l1_config(),
        l1_origin,
        l1_receipts,
        previous_block_txs: prev_block_response.transactions,
        block_header: block_response_to_header(&block_response)?,
        sequenced_txs: extract_sequenced_txs(&block_response),
        witness: ExecutionWitness {
            headers: witness.headers,
            codes: witness.codes,
            state: witness.state,
        },
        message_account,
        expected_state_root: block_response.state_root,
        expected_receipts_root: block_response.receipts_root,
    };

    // 8. Output the fixture
    let json = if args.pretty {
        serde_json::to_string_pretty(&fixture)?
    } else {
        serde_json::to_string(&fixture)?
    };

    if let Some(output_path) = args.output {
        fs::write(&output_path, &json)?;
        eprintln!("Fixture written to: {output_path:?}");
    } else {
        println!("{json}");
    }

    eprintln!("Done!");
    Ok(())
}

/// Fetch an L2 block by number.
async fn fetch_l2_block(
    client: &reqwest::Client,
    rpc_url: &str,
    block_number: u64,
) -> Result<L2BlockResponse, Box<dyn std::error::Error>> {
    let response = client
        .post(rpc_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getBlockByNumber",
            "params": [format!("0x{block_number:x}"), true],
            "id": 1
        }))
        .send()
        .await?
        .json::<RpcResponse<L2BlockResponse>>()
        .await?;

    if let Some(error) = response.error {
        return Err(format!("RPC error {}: {}", error.code, error.message).into());
    }

    response
        .result
        .ok_or_else(|| "Block not found".into())
}

/// Fetch execution witness using debug_executionWitness RPC.
async fn fetch_execution_witness(
    client: &reqwest::Client,
    rpc_url: &str,
    block_number: u64,
) -> Result<ExecutionWitnessResponse, Box<dyn std::error::Error>> {
    let response = client
        .post(rpc_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "debug_executionWitness",
            "params": [format!("0x{block_number:x}")],
            "id": 1
        }))
        .send()
        .await?
        .json::<RpcResponse<ExecutionWitnessResponse>>()
        .await?;

    if let Some(error) = response.error {
        return Err(format!(
            "debug_executionWitness error {}: {} (ensure the node supports this method)",
            error.code, error.message
        ).into());
    }

    response
        .result
        .ok_or_else(|| "Execution witness not found".into())
}

/// Fetch an L1 block by hash.
async fn fetch_l1_block(
    client: &reqwest::Client,
    rpc_url: &str,
    block_hash: B256,
) -> Result<Header, Box<dyn std::error::Error>> {
    let response = client
        .post(rpc_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getBlockByHash",
            "params": [format!("{block_hash:?}"), false],
            "id": 1
        }))
        .send()
        .await?
        .json::<RpcResponse<Header>>()
        .await?;

    if let Some(error) = response.error {
        return Err(format!("RPC error {}: {}", error.code, error.message).into());
    }

    response
        .result
        .ok_or_else(|| "L1 block not found".into())
}

/// Fetch L1 receipts for a block.
async fn fetch_l1_receipts(
    client: &reqwest::Client,
    rpc_url: &str,
    block_hash: B256,
) -> Result<Vec<OpReceiptEnvelope>, Box<dyn std::error::Error>> {
    let response = client
        .post(rpc_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getBlockReceipts",
            "params": [format!("{block_hash:?}")],
            "id": 1
        }))
        .send()
        .await?
        .json::<RpcResponse<Vec<OpReceiptEnvelope>>>()
        .await?;

    if let Some(error) = response.error {
        return Err(format!("RPC error {}: {}", error.code, error.message).into());
    }

    Ok(response.result.unwrap_or_default())
}

/// Fetch account proof using eth_getProof.
async fn fetch_account_proof(
    client: &reqwest::Client,
    rpc_url: &str,
    address: Address,
    _state_root: B256,
    block_number: u64,
) -> Result<AccountResult, Box<dyn std::error::Error>> {
    let response = client
        .post(rpc_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getProof",
            "params": [
                format!("{address:?}"),
                [],
                format!("0x{block_number:x}")
            ],
            "id": 1
        }))
        .send()
        .await?
        .json::<RpcResponse<AccountResult>>()
        .await?;

    if let Some(error) = response.error {
        return Err(format!("RPC error {}: {}", error.code, error.message).into());
    }

    response
        .result
        .ok_or_else(|| "Account proof not found".into())
}

/// Extract L1 origin hash from L1 info deposit tx in the block.
fn extract_l1_origin_hash(
    block: &L2BlockResponse,
) -> Result<B256, Box<dyn std::error::Error>> {
    // The first transaction in every L2 block is the L1 info deposit
    let first_tx = block
        .transactions
        .first()
        .ok_or("Block has no transactions")?;

    // The L1 block hash is embedded in the L1 info deposit tx data
    // For now, return a placeholder - in a real implementation, decode the deposit tx
    // and extract the L1 block hash from the L1BlockInfoTx data

    // This is a simplified extraction - the actual implementation would:
    // 1. Decode the deposit transaction
    // 2. Parse the L1BlockInfoTx from the calldata
    // 3. Extract the L1 block hash

    // For Ecotone format (post-Ecotone hardfork):
    // - Method selector: 0x440a5e20
    // - L1 block hash is at bytes 36-68 (after 4 byte selector + 32 byte offset)

    let tx_data = first_tx.as_ref();
    if tx_data.len() < 69 {
        return Err("L1 info deposit tx too short".into());
    }

    // Skip deposit tx prefix (0x7E) and RLP decode to get calldata
    // For simplicity, assume the L1 block hash location (this is approximate)
    // A proper implementation would fully decode the deposit tx

    // Placeholder: Return error indicating manual extraction needed
    Err("L1 origin extraction requires proper deposit tx decoding - please provide L1 origin hash manually or enhance this function".into())
}

/// Extract sequenced transactions (all txs except the first deposit).
fn extract_sequenced_txs(block: &L2BlockResponse) -> Vec<Bytes> {
    // Skip the first transaction (L1 info deposit) and any other deposits
    block
        .transactions
        .iter()
        .skip(1) // Skip L1 info deposit
        .filter(|tx| {
            // Skip deposit transactions (type 0x7E = 126)
            tx.first().copied() != Some(0x7E)
        })
        .cloned()
        .collect()
}

/// Convert L2BlockResponse to Header.
fn block_response_to_header(
    block: &L2BlockResponse,
) -> Result<Header, Box<dyn std::error::Error>> {
    // Parse hex strings to u64
    let number = u64::from_str_radix(block.number.trim_start_matches("0x"), 16)?;
    let timestamp = u64::from_str_radix(block.timestamp.trim_start_matches("0x"), 16)?;

    // Build header from response
    // Note: This is a simplified conversion - full implementation would parse all fields
    Ok(Header {
        parent_hash: block.parent_hash,
        ommers_hash: B256::ZERO,
        beneficiary: Address::ZERO,
        state_root: block.state_root,
        transactions_root: block.transactions_root,
        receipts_root: block.receipts_root,
        logs_bloom: Default::default(),
        difficulty: Default::default(),
        number,
        gas_limit: 30_000_000, // Default, should parse from response
        gas_used: 0, // Should parse from response
        timestamp,
        extra_data: Default::default(),
        mix_hash: B256::ZERO,
        nonce: Default::default(),
        base_fee_per_gas: Some(1_000_000_000), // Should parse from response
        withdrawals_root: None,
        blob_gas_used: None,
        excess_blob_gas: None,
        parent_beacon_block_root: None,
        requests_hash: None,
    })
}

/// Get Base Sepolia rollup configuration.
fn get_base_sepolia_rollup_config() -> kona_genesis::RollupConfig {
    // Base Sepolia configuration
    // These values should match the actual Base Sepolia rollup config
    kona_genesis::RollupConfig::default()
}

/// Get Sepolia L1 chain configuration.
fn get_sepolia_l1_config() -> L1ChainConfig {
    L1ChainConfig::default()
}
