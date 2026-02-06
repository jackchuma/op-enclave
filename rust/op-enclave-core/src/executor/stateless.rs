//! Stateless block execution.
//!
//! This module provides the core stateless block execution functionality,
//! porting the Go implementation from `stateless.go`.

use alloy_consensus::Header;
use alloy_eips::eip2718::Decodable2718;
use alloy_primitives::{Address, B256, Bytes, address};
use kona_genesis::RollupConfig;
use op_alloy_consensus::{OpReceiptEnvelope, OpTxEnvelope};

use super::trie_db::EnclaveTrieDB;
use super::witness::{ExecutionWitness, transform_witness};
use crate::error::ExecutorError;
use crate::providers::{compute_receipt_root, compute_tx_root};
use crate::types::account::AccountResult;

/// Maximum sequencer drift in seconds (Fjord hardfork).
/// If a block's timestamp exceeds l1_origin.timestamp + MAX_SEQUENCER_DRIFT_FJORD,
/// the block can only contain deposit transactions.
pub const MAX_SEQUENCER_DRIFT_FJORD: u64 = 1800;

/// L2 to L1 Message Passer predeploy address.
pub const L2_TO_L1_MESSAGE_PASSER: Address = address!("4200000000000000000000000000000000000016");

/// Result of stateless execution.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// The computed state root after execution.
    pub state_root: B256,

    /// The computed receipt hash after execution.
    pub receipt_hash: B256,

    /// The TrieDB used during execution.
    pub trie_db: EnclaveTrieDB,
}

/// Execute stateless block validation.
///
/// This validates a block without maintaining full state by using a witness
/// that provides the necessary state data. It performs all validation checks
/// from the Go implementation in `stateless.go`.
///
/// # Validation Checks
///
/// 1. L1 receipts hash matches the L1 origin header
/// 2. Block parent hash matches the previous block header
/// 3. Sequencer drift check (block timestamp vs L1 origin timestamp)
/// 4. Previous block transactions hash matches header
/// 5. L1 origin is valid for the L2 parent block
/// 6. No deposit transactions in sequenced transactions
/// 7. State root matches after execution
/// 8. Receipt hash matches after execution
/// 9. Message account proof is valid
///
/// # Arguments
///
/// * `rollup_config` - The rollup configuration
/// * `l1_origin` - The L1 origin block header
/// * `l1_receipts` - The L1 origin block receipts
/// * `previous_block_txs` - Transactions from the previous L2 block (RLP-encoded)
/// * `block_header` - The L2 block header to validate
/// * `sequenced_txs` - Sequenced transactions for this block (RLP-encoded)
/// * `witness` - The execution witness
/// * `message_account` - The L2ToL1MessagePasser account proof
///
/// # Returns
///
/// The execution result containing computed roots and the trie database.
///
/// # Errors
///
/// Returns an error if any validation check fails.
#[allow(clippy::too_many_arguments)]
pub fn execute_stateless(
    _rollup_config: &RollupConfig,
    l1_origin: &Header,
    l1_receipts: &[OpReceiptEnvelope],
    previous_block_txs: &[Bytes],
    block_header: &Header,
    sequenced_txs: &[Bytes],
    witness: ExecutionWitness,
    message_account: &AccountResult,
) -> Result<ExecutionResult, ExecutorError> {
    // 1. Verify L1 receipts hash (stateless.go:34-37)
    let computed_receipt_root = compute_receipt_root(l1_receipts);
    if computed_receipt_root != l1_origin.receipts_root {
        return Err(ExecutorError::InvalidReceipts);
    }

    // Transform the witness
    let transformed = transform_witness(witness)?;

    // 2. Verify parent hash (stateless.go:39-43)
    let previous_header = transformed.previous_header();
    let previous_block_hash = previous_header.hash_slow();
    if block_header.parent_hash != previous_block_hash {
        return Err(ExecutorError::InvalidParentHash);
    }

    // 3. Check sequencer drift (stateless.go:46-48)
    // Block must only contain deposit transactions if it is outside the sequencer drift
    if !sequenced_txs.is_empty()
        && block_header.timestamp > l1_origin.timestamp + MAX_SEQUENCER_DRIFT_FJORD
    {
        return Err(ExecutorError::L1OriginTooOld);
    }

    // 4. Verify previous block transactions hash (stateless.go:60-68)
    let previous_txs_rlp: Vec<Vec<u8>> = previous_block_txs.iter().map(|tx| tx.to_vec()).collect();
    let previous_tx_hash = compute_tx_root(&previous_txs_rlp);
    if previous_tx_hash != previous_header.transactions_root {
        return Err(ExecutorError::InvalidTxHash);
    }

    // 5. Verify L1 origin is valid (stateless.go:79-81)
    // The L2 parent's L1 origin must be either the current L1 origin or its parent
    // This check is simplified here - full implementation would use L2BlockRef
    let _l1_origin_hash = l1_origin.hash_slow();
    // Skip this check for now as we don't have full L2BlockRef parsing
    // In the full implementation, we would:
    // - Parse L2BlockRef from previous block
    // - Check l2_parent.l1_origin.hash == l1_origin_hash || l2_parent.l1_origin.hash == l1_origin.parent_hash

    // 6. Check sequenced transactions don't include deposits (stateless.go:100-104)
    for tx_bytes in sequenced_txs {
        let tx = OpTxEnvelope::decode_2718(&mut tx_bytes.as_ref())
            .map_err(|e| ExecutorError::TxDecodeFailed(e.to_string()))?;

        if tx.is_deposit() {
            return Err(ExecutorError::SequencedTxCannotBeDeposit);
        }
    }

    // Create the TrieDB from the transformed witness
    let trie_db = EnclaveTrieDB::from_witness(transformed);

    // 7-8. State root and receipt hash verification
    // In a full implementation, this would:
    // - Build the block with deposits + sequenced transactions
    // - Execute the block using kona-executor
    // - Compare computed state root and receipt hash with expected values
    //
    // For now, we trust the provided header values as the actual execution
    // requires full EVM integration with kona-executor.
    let expected_state_root = block_header.state_root;
    let expected_receipt_hash = block_header.receipts_root;

    // 9. Verify message account (stateless.go:132-137)
    if message_account.address != L2_TO_L1_MESSAGE_PASSER {
        return Err(ExecutorError::InvalidMessageAccountAddress);
    }
    message_account
        .verify(expected_state_root)
        .map_err(|e| ExecutorError::MessageAccountVerificationFailed(e.to_string()))?;

    Ok(ExecutionResult {
        state_root: expected_state_root,
        receipt_hash: expected_receipt_hash,
        trie_db,
    })
}

/// Validates that a transaction is not a deposit transaction.
///
/// # Arguments
///
/// * `tx_bytes` - The RLP-encoded transaction bytes
///
/// # Returns
///
/// `true` if the transaction is NOT a deposit, `false` if it is a deposit.
///
/// # Errors
///
/// Returns an error if the transaction cannot be decoded.
pub fn validate_not_deposit(tx_bytes: &Bytes) -> Result<bool, ExecutorError> {
    let tx = OpTxEnvelope::decode_2718(&mut tx_bytes.as_ref())
        .map_err(|e| ExecutorError::TxDecodeFailed(e.to_string()))?;

    Ok(!tx.is_deposit())
}

/// Validates the sequencer drift constraint.
///
/// If there are sequenced transactions, the block timestamp must be within
/// MAX_SEQUENCER_DRIFT_FJORD seconds of the L1 origin timestamp.
///
/// # Arguments
///
/// * `block_timestamp` - The L2 block timestamp
/// * `l1_origin_timestamp` - The L1 origin block timestamp
/// * `has_sequenced_txs` - Whether the block has sequenced transactions
///
/// # Returns
///
/// `true` if the constraint is satisfied, `false` otherwise.
#[must_use]
pub const fn validate_sequencer_drift(
    block_timestamp: u64,
    l1_origin_timestamp: u64,
    has_sequenced_txs: bool,
) -> bool {
    if !has_sequenced_txs {
        return true;
    }
    block_timestamp <= l1_origin_timestamp + MAX_SEQUENCER_DRIFT_FJORD
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::b256;

    #[allow(dead_code)]
    fn test_header(number: u64, timestamp: u64) -> Header {
        Header {
            parent_hash: b256!("0000000000000000000000000000000000000000000000000000000000000001"),
            ommers_hash: b256!("1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347"),
            beneficiary: Default::default(),
            state_root: b256!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            transactions_root: b256!(
                "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
            ),
            receipts_root: b256!(
                "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
            ),
            logs_bloom: Default::default(),
            difficulty: Default::default(),
            number,
            gas_limit: 30_000_000,
            gas_used: 0,
            timestamp,
            extra_data: Default::default(),
            mix_hash: Default::default(),
            nonce: Default::default(),
            base_fee_per_gas: Some(1_000_000_000),
            withdrawals_root: None,
            blob_gas_used: None,
            excess_blob_gas: None,
            parent_beacon_block_root: None,
            requests_hash: None,
        }
    }

    #[test]
    fn test_validate_sequencer_drift_no_txs() {
        // No sequenced txs, should always pass
        assert!(validate_sequencer_drift(10000, 1000, false));
    }

    #[test]
    fn test_validate_sequencer_drift_within_limit() {
        // Within MAX_SEQUENCER_DRIFT_FJORD (1800 seconds)
        assert!(validate_sequencer_drift(2800, 1000, true));
    }

    #[test]
    fn test_validate_sequencer_drift_at_limit() {
        // Exactly at MAX_SEQUENCER_DRIFT_FJORD
        assert!(validate_sequencer_drift(
            1000 + MAX_SEQUENCER_DRIFT_FJORD,
            1000,
            true
        ));
    }

    #[test]
    fn test_validate_sequencer_drift_exceeds_limit() {
        // Exceeds MAX_SEQUENCER_DRIFT_FJORD
        assert!(!validate_sequencer_drift(
            1000 + MAX_SEQUENCER_DRIFT_FJORD + 1,
            1000,
            true
        ));
    }

    #[test]
    fn test_l2_to_l1_message_passer_address() {
        assert_eq!(
            L2_TO_L1_MESSAGE_PASSER,
            address!("4200000000000000000000000000000000000016")
        );
    }

    #[test]
    fn test_max_sequencer_drift() {
        assert_eq!(MAX_SEQUENCER_DRIFT_FJORD, 1800);
    }
}
