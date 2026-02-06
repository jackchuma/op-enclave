//! Payload attributes builder for stateless execution.
//!
//! This module provides functionality to build payload attributes by extracting
//! deposit transactions from L1 receipts.

use alloy_consensus::{Header, ReceiptEnvelope};
use alloy_eips::eip2718::{Decodable2718, Encodable2718};
use alloy_primitives::{Address, B256, Bytes, Log, U256, address};
use hex_literal::hex;
use kona_genesis::{L1ChainConfig, RollupConfig, SystemConfig};
use kona_protocol::{L1BlockInfoTx, decode_deposit};
use op_alloy_consensus::OpTxEnvelope;

use crate::error::ExecutorError;

/// The L1 Attributes Depositor address (L1Block contract depositor).
pub const L1_ATTRIBUTES_DEPOSITOR: Address = address!("deaddeaddeaddeaddeaddeaddeaddeaddead0001");

/// The L1 Attributes Predeployed Contract address.
pub const L1_ATTRIBUTES_PREDEPLOYED: Address = address!("4200000000000000000000000000000000000015");

/// Deposit event topic (TransactionDeposited event).
/// keccak256("TransactionDeposited(address,address,uint256,bytes)")
pub const DEPOSIT_EVENT_TOPIC: B256 = B256::new(hex!(
    "b3813568d9991fc951961fcb4c784893574240a28925604d09fc577c55bb7c32"
));

/// Extract deposit transactions from L1 receipts.
///
/// This builds the complete deposit transaction list for an L2 block:
/// 1. First transaction: L1 info deposit tx (records L1 block info on L2)
/// 2. Remaining transactions: User deposits from TransactionDeposited events
///
/// # Arguments
///
/// * `rollup_config` - The rollup configuration
/// * `l1_config` - The L1 chain configuration
/// * `system_config` - The current system configuration
/// * `l1_origin` - The L1 origin block header
/// * `l1_origin_hash` - The L1 origin block hash
/// * `receipts` - The L1 origin block receipts
/// * `l2_block_number` - The L2 block number being built
/// * `l2_timestamp` - The L2 block timestamp
/// * `sequence_number` - The sequence number (0 if new L1 origin, else parent.seq_num + 1)
#[allow(clippy::too_many_arguments)]
pub fn extract_deposits_from_receipts(
    rollup_config: &RollupConfig,
    l1_config: &L1ChainConfig,
    system_config: &SystemConfig,
    l1_origin: &Header,
    l1_origin_hash: B256,
    receipts: &[ReceiptEnvelope],
    l2_block_number: u64,
    l2_timestamp: u64,
    sequence_number: u64,
) -> Result<Vec<Bytes>, ExecutorError> {
    let mut deposits = Vec::new();

    // 1. Build L1 info deposit transaction (always first)
    let l1_info_deposit = build_l1_info_deposit_tx(
        rollup_config,
        l1_config,
        system_config,
        l1_origin,
        l2_block_number,
        l2_timestamp,
        sequence_number,
    )?;
    deposits.push(l1_info_deposit);

    // 2. Extract user deposits from L1 receipts
    let deposit_contract_address = rollup_config.deposit_contract_address;
    let mut log_index: usize = 0;

    for receipt in receipts {
        // Get logs from the receipt
        let logs = get_receipt_logs(receipt);

        for log in logs {
            // Check if this is a deposit event from the deposit contract
            if log.address == deposit_contract_address
                && !log.topics().is_empty()
                && log.topics()[0] == DEPOSIT_EVENT_TOPIC
            {
                // Parse the deposit transaction using kona-protocol
                let deposit_tx = decode_deposit(l1_origin_hash, log_index, log).map_err(|e| {
                    ExecutorError::AttributesBuildFailed(format!("failed to decode deposit: {e}"))
                })?;
                deposits.push(deposit_tx);
            }
            log_index += 1;
        }
    }

    Ok(deposits)
}

/// Get logs from a receipt envelope.
fn get_receipt_logs(receipt: &ReceiptEnvelope) -> &[Log] {
    match receipt {
        ReceiptEnvelope::Legacy(r) => &r.receipt.logs,
        ReceiptEnvelope::Eip2930(r) => &r.receipt.logs,
        ReceiptEnvelope::Eip1559(r) => &r.receipt.logs,
        ReceiptEnvelope::Eip4844(r) => &r.receipt.logs,
        ReceiptEnvelope::Eip7702(r) => &r.receipt.logs,
    }
}

/// Build the L1 info deposit transaction.
///
/// This is the first transaction in every L2 block that records L1 block info.
/// Uses the appropriate format based on the active hardfork (Bedrock/Ecotone/Isthmus/Jovian).
fn build_l1_info_deposit_tx(
    rollup_config: &RollupConfig,
    l1_config: &L1ChainConfig,
    system_config: &SystemConfig,
    l1_origin: &Header,
    _l2_block_number: u64,
    l2_timestamp: u64,
    sequence_number: u64,
) -> Result<Bytes, ExecutorError> {
    // Use kona-protocol's L1BlockInfoTx to build the deposit transaction
    let (_l1_info, deposit_tx) = L1BlockInfoTx::try_new_with_deposit_tx(
        rollup_config,
        l1_config,
        system_config,
        sequence_number,
        l1_origin,
        l2_timestamp,
    )
    .map_err(|e| {
        ExecutorError::AttributesBuildFailed(format!("failed to build L1 info deposit tx: {e}"))
    })?;

    // Encode the deposit transaction
    let mut encoded = Vec::new();
    deposit_tx.encode_2718(&mut encoded);

    Ok(Bytes::from(encoded))
}

/// Result of comparing two deposit transactions.
#[derive(Debug, Clone)]
pub struct DepositComparison {
    /// Whether the deposits match exactly.
    pub matches: bool,
    /// Length comparison.
    pub length_match: bool,
    pub actual_length: usize,
    pub generated_length: usize,
    /// Source hash comparison.
    pub source_hash_match: bool,
    pub actual_source_hash: Option<B256>,
    pub generated_source_hash: Option<B256>,
    /// From address comparison.
    pub from_match: bool,
    pub actual_from: Option<Address>,
    pub generated_from: Option<Address>,
    /// To address comparison.
    pub to_match: bool,
    pub actual_to: Option<Address>,
    pub generated_to: Option<Address>,
    /// Mint value comparison.
    pub mint_match: bool,
    pub actual_mint: Option<u128>,
    pub generated_mint: Option<u128>,
    /// Value comparison.
    pub value_match: bool,
    pub actual_value: Option<U256>,
    pub generated_value: Option<U256>,
    /// Gas comparison.
    pub gas_match: bool,
    pub actual_gas: Option<u64>,
    pub generated_gas: Option<u64>,
    /// Is system tx comparison.
    pub is_system_tx_match: bool,
    pub actual_is_system_tx: Option<bool>,
    pub generated_is_system_tx: Option<bool>,
    /// Input/calldata comparison.
    pub input_match: bool,
    pub actual_input_len: usize,
    pub generated_input_len: usize,
    /// Decoded L1BlockInfo comparison (if decodable).
    pub l1_info_comparison: Option<L1BlockInfoComparison>,
}

/// Comparison of decoded L1BlockInfo from deposit calldata.
#[derive(Debug, Clone)]
pub struct L1BlockInfoComparison {
    /// Whether all L1BlockInfo fields match.
    pub matches: bool,
    /// Block number comparison.
    pub number_match: bool,
    pub actual_number: u64,
    pub generated_number: u64,
    /// Base fee comparison.
    pub base_fee_match: bool,
    pub actual_base_fee: U256,
    pub generated_base_fee: U256,
    /// Block hash comparison.
    pub block_hash_match: bool,
    pub actual_block_hash: B256,
    pub generated_block_hash: B256,
    /// Sequence number comparison.
    pub sequence_number_match: bool,
    pub actual_sequence_number: u64,
    pub generated_sequence_number: u64,
    /// Batcher address comparison.
    pub batcher_match: bool,
    pub actual_batcher: Address,
    pub generated_batcher: Address,
    /// L1 fee scalar comparison.
    pub l1_fee_scalar_match: bool,
    pub actual_l1_fee_scalar: U256,
    pub generated_l1_fee_scalar: U256,
    /// Blob base fee comparison.
    pub blob_base_fee_match: bool,
    pub actual_blob_base_fee: U256,
    pub generated_blob_base_fee: U256,
    /// Blob base fee scalar comparison.
    pub blob_base_fee_scalar_match: bool,
    pub actual_blob_base_fee_scalar: U256,
    pub generated_blob_base_fee_scalar: U256,
    /// Operator fee scalar comparison (Isthmus+).
    pub operator_fee_scalar_match: bool,
    pub actual_operator_fee_scalar: u32,
    pub generated_operator_fee_scalar: u32,
    /// Operator fee constant comparison (Isthmus+).
    pub operator_fee_constant_match: bool,
    pub actual_operator_fee_constant: u64,
    pub generated_operator_fee_constant: u64,
    /// DA footprint gas scalar comparison (Jovian+).
    pub da_footprint_match: bool,
    pub actual_da_footprint: Option<u16>,
    pub generated_da_footprint: Option<u16>,
}

/// Compare two deposit transactions field-by-field.
///
/// This is a diagnostic function to identify exactly which fields differ
/// between an actual block's deposit transaction and a regenerated one.
///
/// # Arguments
///
/// * `actual` - The actual deposit transaction from the block
/// * `generated` - The regenerated deposit transaction
///
/// # Returns
///
/// A comparison result showing which fields match and their values.
pub fn compare_deposits(actual: &Bytes, generated: &Bytes) -> DepositComparison {
    let length_match = actual.len() == generated.len();

    // Try to decode both as OpTxEnvelope
    let actual_tx = OpTxEnvelope::decode_2718(&mut actual.as_ref()).ok();
    let generated_tx = OpTxEnvelope::decode_2718(&mut generated.as_ref()).ok();

    // Extract deposit fields if both decode
    let (actual_deposit, generated_deposit) = match (&actual_tx, &generated_tx) {
        (Some(OpTxEnvelope::Deposit(a)), Some(OpTxEnvelope::Deposit(g))) => (Some(a), Some(g)),
        _ => (None, None),
    };

    let source_hash_match = actual_deposit
        .zip(generated_deposit)
        .is_some_and(|(a, g)| a.source_hash == g.source_hash);

    let from_match = actual_deposit
        .zip(generated_deposit)
        .is_some_and(|(a, g)| a.from == g.from);

    let to_match = actual_deposit
        .zip(generated_deposit)
        .is_some_and(|(a, g)| a.to == g.to);

    let mint_match = actual_deposit
        .zip(generated_deposit)
        .is_some_and(|(a, g)| a.mint == g.mint);

    let value_match = actual_deposit
        .zip(generated_deposit)
        .is_some_and(|(a, g)| a.value == g.value);

    let gas_match = actual_deposit
        .zip(generated_deposit)
        .is_some_and(|(a, g)| a.gas_limit == g.gas_limit);

    let is_system_tx_match = actual_deposit
        .zip(generated_deposit)
        .is_some_and(|(a, g)| a.is_system_transaction == g.is_system_transaction);

    let input_match = actual_deposit
        .zip(generated_deposit)
        .is_some_and(|(a, g)| a.input == g.input);

    // Try to decode L1BlockInfo from both calldatas
    let l1_info_comparison = actual_deposit
        .zip(generated_deposit)
        .and_then(|(a, g)| compare_l1_block_info(&a.input, &g.input));

    let matches = length_match
        && source_hash_match
        && from_match
        && to_match
        && mint_match
        && value_match
        && gas_match
        && is_system_tx_match
        && input_match;

    DepositComparison {
        matches,
        length_match,
        actual_length: actual.len(),
        generated_length: generated.len(),
        source_hash_match,
        actual_source_hash: actual_deposit.map(|d| d.source_hash),
        generated_source_hash: generated_deposit.map(|d| d.source_hash),
        from_match,
        actual_from: actual_deposit.map(|d| d.from),
        generated_from: generated_deposit.map(|d| d.from),
        to_match,
        actual_to: actual_deposit.and_then(|d| d.to.to()).copied(),
        generated_to: generated_deposit.and_then(|d| d.to.to()).copied(),
        mint_match,
        actual_mint: actual_deposit.map(|d| d.mint),
        generated_mint: generated_deposit.map(|d| d.mint),
        value_match,
        actual_value: actual_deposit.map(|d| d.value),
        generated_value: generated_deposit.map(|d| d.value),
        gas_match,
        actual_gas: actual_deposit.map(|d| d.gas_limit),
        generated_gas: generated_deposit.map(|d| d.gas_limit),
        is_system_tx_match,
        actual_is_system_tx: actual_deposit.map(|d| d.is_system_transaction),
        generated_is_system_tx: generated_deposit.map(|d| d.is_system_transaction),
        input_match,
        actual_input_len: actual_deposit.map_or(0, |d| d.input.len()),
        generated_input_len: generated_deposit.map_or(0, |d| d.input.len()),
        l1_info_comparison,
    }
}

/// Compare L1BlockInfo from two calldatas.
fn compare_l1_block_info(actual_calldata: &Bytes, generated_calldata: &Bytes) -> Option<L1BlockInfoComparison> {
    let actual_l1_info = L1BlockInfoTx::decode_calldata(actual_calldata).ok()?;
    let generated_l1_info = L1BlockInfoTx::decode_calldata(generated_calldata).ok()?;

    let number_match = actual_l1_info.id().number == generated_l1_info.id().number;
    let base_fee_match = actual_l1_info.l1_base_fee() == generated_l1_info.l1_base_fee();
    let block_hash_match = actual_l1_info.id().hash == generated_l1_info.id().hash;
    let sequence_number_match = actual_l1_info.sequence_number() == generated_l1_info.sequence_number();
    let batcher_match = actual_l1_info.batcher_address() == generated_l1_info.batcher_address();
    let l1_fee_scalar_match = actual_l1_info.l1_fee_scalar() == generated_l1_info.l1_fee_scalar();
    let blob_base_fee_match = actual_l1_info.blob_base_fee() == generated_l1_info.blob_base_fee();
    let blob_base_fee_scalar_match = actual_l1_info.blob_base_fee_scalar() == generated_l1_info.blob_base_fee_scalar();
    let operator_fee_scalar_match = actual_l1_info.operator_fee_scalar() == generated_l1_info.operator_fee_scalar();
    let operator_fee_constant_match = actual_l1_info.operator_fee_constant() == generated_l1_info.operator_fee_constant();
    let da_footprint_match = actual_l1_info.da_footprint() == generated_l1_info.da_footprint();

    let matches = number_match
        && base_fee_match
        && block_hash_match
        && sequence_number_match
        && batcher_match
        && l1_fee_scalar_match
        && blob_base_fee_match
        && blob_base_fee_scalar_match
        && operator_fee_scalar_match
        && operator_fee_constant_match
        && da_footprint_match;

    Some(L1BlockInfoComparison {
        matches,
        number_match,
        actual_number: actual_l1_info.id().number,
        generated_number: generated_l1_info.id().number,
        base_fee_match,
        actual_base_fee: actual_l1_info.l1_base_fee(),
        generated_base_fee: generated_l1_info.l1_base_fee(),
        block_hash_match,
        actual_block_hash: actual_l1_info.id().hash,
        generated_block_hash: generated_l1_info.id().hash,
        sequence_number_match,
        actual_sequence_number: actual_l1_info.sequence_number(),
        generated_sequence_number: generated_l1_info.sequence_number(),
        batcher_match,
        actual_batcher: actual_l1_info.batcher_address(),
        generated_batcher: generated_l1_info.batcher_address(),
        l1_fee_scalar_match,
        actual_l1_fee_scalar: actual_l1_info.l1_fee_scalar(),
        generated_l1_fee_scalar: generated_l1_info.l1_fee_scalar(),
        blob_base_fee_match,
        actual_blob_base_fee: actual_l1_info.blob_base_fee(),
        generated_blob_base_fee: generated_l1_info.blob_base_fee(),
        blob_base_fee_scalar_match,
        actual_blob_base_fee_scalar: actual_l1_info.blob_base_fee_scalar(),
        generated_blob_base_fee_scalar: generated_l1_info.blob_base_fee_scalar(),
        operator_fee_scalar_match,
        actual_operator_fee_scalar: actual_l1_info.operator_fee_scalar(),
        generated_operator_fee_scalar: generated_l1_info.operator_fee_scalar(),
        operator_fee_constant_match,
        actual_operator_fee_constant: actual_l1_info.operator_fee_constant(),
        generated_operator_fee_constant: generated_l1_info.operator_fee_constant(),
        da_footprint_match,
        actual_da_footprint: actual_l1_info.da_footprint(),
        generated_da_footprint: generated_l1_info.da_footprint(),
    })
}

/// Print a detailed deposit comparison to stderr for debugging.
pub fn print_deposit_comparison(comparison: &DepositComparison) {
    eprintln!("=== Deposit Comparison ===");
    eprintln!("  Overall match: {}", comparison.matches);
    eprintln!("  Length: actual={}, generated={}, match={}",
        comparison.actual_length, comparison.generated_length, comparison.length_match);

    if let (Some(actual), Some(generated)) = (&comparison.actual_source_hash, &comparison.generated_source_hash) {
        eprintln!("  Source hash: match={}", comparison.source_hash_match);
        if !comparison.source_hash_match {
            eprintln!("    actual:    {actual}");
            eprintln!("    generated: {generated}");
        }
    }

    if !comparison.from_match {
        eprintln!("  From: actual={:?}, generated={:?}",
            comparison.actual_from, comparison.generated_from);
    }

    if !comparison.to_match {
        eprintln!("  To: actual={:?}, generated={:?}",
            comparison.actual_to, comparison.generated_to);
    }

    if !comparison.gas_match {
        eprintln!("  Gas: actual={:?}, generated={:?}",
            comparison.actual_gas, comparison.generated_gas);
    }

    if !comparison.input_match {
        eprintln!("  Input length: actual={}, generated={}",
            comparison.actual_input_len, comparison.generated_input_len);
    }

    if let Some(l1_info) = &comparison.l1_info_comparison {
        eprintln!("  L1BlockInfo comparison:");
        eprintln!("    Overall L1Info match: {}", l1_info.matches);

        if !l1_info.number_match {
            eprintln!("    Number: actual={}, generated={}",
                l1_info.actual_number, l1_info.generated_number);
        }
        if !l1_info.base_fee_match {
            eprintln!("    Base fee: actual={}, generated={}",
                l1_info.actual_base_fee, l1_info.generated_base_fee);
        }
        if !l1_info.block_hash_match {
            eprintln!("    Block hash: actual={}, generated={}",
                l1_info.actual_block_hash, l1_info.generated_block_hash);
        }
        if !l1_info.sequence_number_match {
            eprintln!("    Sequence number: actual={}, generated={}",
                l1_info.actual_sequence_number, l1_info.generated_sequence_number);
        }
        if !l1_info.batcher_match {
            eprintln!("    Batcher: actual={}, generated={}",
                l1_info.actual_batcher, l1_info.generated_batcher);
        }
        if !l1_info.l1_fee_scalar_match {
            eprintln!("    L1 fee scalar: actual={}, generated={}",
                l1_info.actual_l1_fee_scalar, l1_info.generated_l1_fee_scalar);
        }
        if !l1_info.blob_base_fee_match {
            eprintln!("    Blob base fee: actual={}, generated={}",
                l1_info.actual_blob_base_fee, l1_info.generated_blob_base_fee);
        }
        if !l1_info.blob_base_fee_scalar_match {
            eprintln!("    Blob base fee scalar: actual={}, generated={}",
                l1_info.actual_blob_base_fee_scalar, l1_info.generated_blob_base_fee_scalar);
        }
        if !l1_info.operator_fee_scalar_match {
            eprintln!("    Operator fee scalar: actual={}, generated={}",
                l1_info.actual_operator_fee_scalar, l1_info.generated_operator_fee_scalar);
        }
        if !l1_info.operator_fee_constant_match {
            eprintln!("    Operator fee constant: actual={}, generated={}",
                l1_info.actual_operator_fee_constant, l1_info.generated_operator_fee_constant);
        }
        if !l1_info.da_footprint_match {
            eprintln!("    DA footprint: actual={:?}, generated={:?}",
                l1_info.actual_da_footprint, l1_info.generated_da_footprint);
        }
    }
    eprintln!("===========================");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit_event_topic() {
        // Verify the deposit event topic is correct
        // keccak256("TransactionDeposited(address,address,uint256,bytes)")
        let expected = B256::new(hex_literal::hex!(
            "b3813568d9991fc951961fcb4c784893574240a28925604d09fc577c55bb7c32"
        ));
        assert_eq!(DEPOSIT_EVENT_TOPIC, expected);
    }

    #[test]
    fn test_l1_attributes_addresses() {
        // Verify the predefined addresses are correct
        assert_eq!(
            L1_ATTRIBUTES_DEPOSITOR,
            address!("deaddeaddeaddeaddeaddeaddeaddeaddead0001")
        );
        assert_eq!(
            L1_ATTRIBUTES_PREDEPLOYED,
            address!("4200000000000000000000000000000000000015")
        );
    }
}
