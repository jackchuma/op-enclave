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

use alloy_consensus::{Header, ReceiptEnvelope};
use alloy_eips::eip2718::Decodable2718;
use alloy_primitives::{Address, B256, Bytes, keccak256};
use clap::Parser;
use kona_protocol::L1BlockInfoTx;
use op_alloy_consensus::OpTxEnvelope;
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
    pub l1_receipts: Vec<ReceiptEnvelope>,

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

/// L2 block response from RPC (with tx hashes).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct L2BlockHashesResponse {
    hash: B256,
    parent_hash: B256,
    number: String,
    timestamp: String,
    state_root: B256,
    receipts_root: B256,
    transactions_root: B256,
    gas_limit: String,
    gas_used: String,
    #[serde(default)]
    base_fee_per_gas: Option<String>,
    transactions: Vec<B256>,
    miner: Address,
    logs_bloom: alloy_primitives::Bloom,
    extra_data: Bytes,
    mix_hash: B256,
    #[allow(dead_code)]
    nonce: String,
    #[serde(default)]
    parent_beacon_block_root: Option<B256>,
    #[serde(default)]
    withdrawals_root: Option<B256>,
}

/// L2 block with raw transaction bytes.
#[derive(Debug)]
struct L2BlockResponse {
    hash: B256,
    parent_hash: B256,
    number: String,
    timestamp: String,
    state_root: B256,
    receipts_root: B256,
    transactions_root: B256,
    gas_limit: u64,
    gas_used: u64,
    base_fee_per_gas: Option<u64>,
    transactions: Vec<Bytes>,
    beneficiary: Address,
    logs_bloom: alloy_primitives::Bloom,
    extra_data: Bytes,
    mix_hash: B256,
    parent_beacon_block_root: Option<B256>,
    withdrawals_root: Option<B256>,
}

/// Execution witness response from debug_executionWitness RPC.
#[derive(Debug, Deserialize)]
struct ExecutionWitnessResponse {
    headers: Vec<Header>,
    codes: Vec<String>,
    state: Vec<String>,
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
        block,
    ).await?;

    // 7. Convert codes array to HashMap (code_hash -> bytecode)
    let codes_map: HashMap<String, String> = witness
        .codes
        .into_iter()
        .map(|code_hex| {
            // Decode the bytecode to compute its hash
            let code_bytes = hex::decode(code_hex.trim_start_matches("0x")).unwrap_or_default();
            let code_hash = keccak256(&code_bytes);
            (format!("{code_hash:?}"), format!("0x{}", hex::encode(&code_bytes)))
        })
        .collect();

    // Convert state array to HashMap (node_hash -> node)
    let state_map: HashMap<String, String> = witness
        .state
        .into_iter()
        .map(|node_hex| {
            // Decode the node to compute its hash
            let node_bytes = hex::decode(node_hex.trim_start_matches("0x")).unwrap_or_default();
            let node_hash = keccak256(&node_bytes);
            (format!("{node_hash:?}"), format!("0x{}", hex::encode(&node_bytes)))
        })
        .collect();

    // 8. Build the fixture
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
            codes: codes_map,
            state: state_map,
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

/// Fetch an L2 block by number with raw transaction bytes.
async fn fetch_l2_block(
    client: &reqwest::Client,
    rpc_url: &str,
    block_number: u64,
) -> Result<L2BlockResponse, Box<dyn std::error::Error>> {
    // First fetch block with transaction hashes
    let response = client
        .post(rpc_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getBlockByNumber",
            "params": [format!("0x{block_number:x}"), false],
            "id": 1
        }))
        .send()
        .await?
        .json::<RpcResponse<L2BlockHashesResponse>>()
        .await?;

    if let Some(error) = response.error {
        return Err(format!("RPC error {}: {}", error.code, error.message).into());
    }

    let block = response.result.ok_or("Block not found")?;

    // Fetch raw transactions
    let mut raw_txs = Vec::with_capacity(block.transactions.len());
    for tx_hash in &block.transactions {
        let raw_tx = fetch_raw_transaction(client, rpc_url, *tx_hash).await?;
        raw_txs.push(raw_tx);
    }

    // Parse hex values
    let gas_limit = u64::from_str_radix(block.gas_limit.trim_start_matches("0x"), 16)?;
    let gas_used = u64::from_str_radix(block.gas_used.trim_start_matches("0x"), 16)?;
    let base_fee = block.base_fee_per_gas
        .as_ref()
        .map(|s| u64::from_str_radix(s.trim_start_matches("0x"), 16))
        .transpose()?;

    Ok(L2BlockResponse {
        hash: block.hash,
        parent_hash: block.parent_hash,
        number: block.number,
        timestamp: block.timestamp,
        state_root: block.state_root,
        receipts_root: block.receipts_root,
        transactions_root: block.transactions_root,
        gas_limit,
        gas_used,
        base_fee_per_gas: base_fee,
        transactions: raw_txs,
        beneficiary: block.miner,
        logs_bloom: block.logs_bloom,
        extra_data: block.extra_data,
        mix_hash: block.mix_hash,
        parent_beacon_block_root: block.parent_beacon_block_root,
        withdrawals_root: block.withdrawals_root,
    })
}

/// Fetch raw transaction by hash.
async fn fetch_raw_transaction(
    client: &reqwest::Client,
    rpc_url: &str,
    tx_hash: B256,
) -> Result<Bytes, Box<dyn std::error::Error>> {
    let response = client
        .post(rpc_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_getRawTransactionByHash",
            "params": [format!("{tx_hash:?}")],
            "id": 1
        }))
        .send()
        .await?
        .json::<RpcResponse<Bytes>>()
        .await?;

    if let Some(error) = response.error {
        return Err(format!("RPC error {}: {}", error.code, error.message).into());
    }

    response
        .result
        .ok_or_else(|| format!("Raw transaction not found: {tx_hash:?}").into())
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
) -> Result<Vec<ReceiptEnvelope>, Box<dyn std::error::Error>> {
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
        .json::<RpcResponse<Vec<ReceiptEnvelope>>>()
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
    let first_tx = block.transactions.first()
        .ok_or("Block has no transactions")?;

    let tx = OpTxEnvelope::decode_2718(&mut first_tx.as_ref())
        .map_err(|e| format!("Failed to decode deposit tx: {e}"))?;

    let deposit = match &tx {
        OpTxEnvelope::Deposit(d) => d,
        _ => return Err("First tx is not a deposit".into())
    };

    let l1_info = L1BlockInfoTx::decode_calldata(deposit.input.as_ref())
        .map_err(|e| format!("Failed to decode L1BlockInfoTx: {e}"))?;

    Ok(l1_info.id().hash)
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

    Ok(Header {
        parent_hash: block.parent_hash,
        ommers_hash: alloy_primitives::b256!("1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347"),
        beneficiary: block.beneficiary,
        state_root: block.state_root,
        transactions_root: block.transactions_root,
        receipts_root: block.receipts_root,
        logs_bloom: block.logs_bloom,
        difficulty: Default::default(),
        number,
        gas_limit: block.gas_limit,
        gas_used: block.gas_used,
        timestamp,
        extra_data: block.extra_data.clone(),
        mix_hash: block.mix_hash,
        nonce: Default::default(),
        base_fee_per_gas: block.base_fee_per_gas,
        withdrawals_root: block.withdrawals_root,
        blob_gas_used: None,
        excess_blob_gas: None,
        parent_beacon_block_root: block.parent_beacon_block_root,
        requests_hash: None,
    })
}

/// Get Base Sepolia rollup configuration.
fn get_base_sepolia_rollup_config() -> kona_genesis::RollupConfig {
    use alloy_eips::eip1898::BlockNumHash;
    use alloy_primitives::b256;
    use kona_genesis::{BaseFeeConfig, ChainGenesis, HardForkConfig, SystemConfig};

    kona_genesis::RollupConfig {
        l1_chain_id: 11155111, // Sepolia
        l2_chain_id: alloy_chains::Chain::from_id(84532), // Base Sepolia

        genesis: ChainGenesis {
            l1: BlockNumHash {
                number: 4370868,
                hash: b256!("cac9a83291d4dec146d6f7f69ab2304f23f5be87b1789119a0c5b1e4482444ed"),
            },
            l2: BlockNumHash {
                number: 0,
                hash: b256!("0dcc9e089e30b90ddfc55be9a37dd15bc551aeee999d2e2b51414c54eaf934e4"),
            },
            l2_time: 1695768288,
            system_config: Some(SystemConfig {
                batcher_address: "0x6CDEbe940BC0F26850285cacA097C11c33103E47".parse().unwrap(),
                gas_limit: 25_000_000,
                ..SystemConfig::default()
            }),
        },

        block_time: 2,
        max_sequencer_drift: 600,
        seq_window_size: 3600,
        channel_timeout: 300,
        granite_channel_timeout: 50,

        // Base Sepolia contract addresses
        deposit_contract_address: "0x49f53e41452C74589E85cA1677426Ba426459e85".parse().unwrap(),
        l1_system_config_address: "0xf272670eb55e895584501d564AfEB048bEd26194".parse().unwrap(),
        batch_inbox_address: "0xfF00000000000000000000000000000000084532".parse().unwrap(),
        protocol_versions_address: Address::ZERO,
        da_challenge_address: None,
        superchain_config_address: None,

        blobs_enabled_l1_timestamp: Some(0),

        // Base Sepolia hardfork timestamps
        hardforks: HardForkConfig {
            regolith_time: Some(0),
            canyon_time: Some(1699981200),
            delta_time: Some(1703203200),
            ecotone_time: Some(1708534800),
            fjord_time: Some(1716998400),
            granite_time: Some(1723478400),
            holocene_time: Some(1732633200),
            pectra_blob_schedule_time: Some(1742486400),
            isthmus_time: Some(1744905600),
            jovian_time: Some(1763568001),
            interop_time: None,
        },

        interop_message_expiry_window: 0,
        alt_da_config: None,
        chain_op_config: BaseFeeConfig {
            eip1559_elasticity: 10,
            eip1559_denominator: 50,
            eip1559_denominator_canyon: 250,
        },
    }
}

/// Get Sepolia L1 chain configuration.
fn get_sepolia_l1_config() -> L1ChainConfig {
    // Use default L1 chain config - the specific chain doesn't matter
    // for deposit extraction, only the hardfork timestamps matter
    L1ChainConfig::default()
}
