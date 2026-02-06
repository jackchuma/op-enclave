//! EVM execution wrapper for stateless block execution.
//!
//! This module provides a wrapper around the kona-executor's `StatelessL2Builder`
//! for executing L2 blocks in a stateless manner within an enclave.

use alloy_consensus::{Header, Sealed};
use alloy_eips::eip2718::Decodable2718;
use alloy_primitives::{Address, B256, Bytes, U256};
use kona_genesis::RollupConfig;
use kona_mpt::TrieHinter;
use op_alloy_consensus::OpTxEnvelope;
use op_alloy_rpc_types_engine::OpPayloadAttributes;

use super::trie_db::EnclaveTrieDB;
use crate::error::ExecutorError;

/// Result of EVM block execution.
#[derive(Debug, Clone)]
pub struct BlockExecutionResult {
    /// The computed state root after execution.
    pub state_root: B256,

    /// The computed receipts root after execution.
    pub receipts_root: B256,

    /// Gas used by the block.
    pub gas_used: u64,
}

/// No-op trie hinter for enclave execution.
///
/// In the enclave context, we don't need to hint for state access
/// since all required state is provided in the witness.
#[derive(Debug, Clone, Copy, Default)]
pub struct EnclaveTrieHinter;

impl TrieHinter for EnclaveTrieHinter {
    type Error = String;

    fn hint_trie_node(&self, _hash: B256) -> Result<(), Self::Error> {
        // No-op: all state is pre-loaded from witness
        Ok(())
    }

    fn hint_account_proof(&self, _address: Address, _block_number: u64) -> Result<(), Self::Error> {
        // No-op: all state is pre-loaded from witness
        Ok(())
    }

    fn hint_storage_proof(
        &self,
        _address: Address,
        _slot: U256,
        _block_number: u64,
    ) -> Result<(), Self::Error> {
        // No-op: all state is pre-loaded from witness
        Ok(())
    }

    fn hint_execution_witness(
        &self,
        _parent_hash: B256,
        _op_payload_attributes: &OpPayloadAttributes,
    ) -> Result<(), Self::Error> {
        // No-op: witness is already provided
        Ok(())
    }
}

/// Execute a block using the stateless L2 executor.
///
/// This executes the given transactions against the parent state provided
/// by the `EnclaveTrieDB` and returns the computed state root and receipts root.
///
/// # Arguments
///
/// * `rollup_config` - The rollup configuration
/// * `parent_header` - The parent block header (sealed)
/// * `transactions` - The transactions to execute (EIP-2718 encoded)
/// * `_trie_db` - The trie database with pre-loaded state
///
/// # Returns
///
/// The execution result containing computed roots.
///
/// # Note
///
/// This is a placeholder implementation. Full EVM execution requires:
/// 1. Building OpPayloadAttributes from the transactions
/// 2. Executing via StatelessL2Builder
/// 3. Extracting the final state root from the executed block
///
/// The actual integration with `StatelessL2Builder` requires more setup
/// including proper EVM factory configuration and payload attribute building.
pub fn execute_block(
    _rollup_config: &RollupConfig,
    _parent_header: &Sealed<Header>,
    transactions: &[Bytes],
    _trie_db: &EnclaveTrieDB,
) -> Result<BlockExecutionResult, ExecutorError> {
    // Decode and validate all transactions
    for (i, tx_bytes) in transactions.iter().enumerate() {
        let _tx = OpTxEnvelope::decode_2718(&mut tx_bytes.as_ref()).map_err(|e| {
            ExecutorError::TxDecodeFailed(format!("failed to decode tx {i}: {e}"))
        })?;
    }

    // TODO: Full EVM execution integration
    //
    // The full implementation would:
    // 1. Create OpPayloadAttributes from the block parameters
    // 2. Initialize StatelessL2Builder with the trie_db
    // 3. Execute the block via builder.build_block(attrs)
    // 4. Extract state_root and receipts_root from the result
    //
    // Example (requires additional setup):
    // ```
    // let attrs = OpPayloadAttributes {
    //     payload_attributes: PayloadAttributes {
    //         timestamp: block_header.timestamp,
    //         prev_randao: block_header.mix_hash,
    //         suggested_fee_recipient: block_header.beneficiary,
    //         withdrawals: None,
    //         parent_beacon_block_root: block_header.parent_beacon_block_root,
    //     },
    //     transactions: Some(transactions.to_vec()),
    //     no_tx_pool: Some(true),
    //     gas_limit: Some(block_header.gas_limit),
    //     eip_1559_params: None,
    // };
    //
    // let mut builder = StatelessL2Builder::new(
    //     rollup_config,
    //     OpEvmFactory::default(),
    //     trie_db.clone(),
    //     EnclaveTrieHinter,
    //     parent_header.clone(),
    // );
    //
    // let outcome = builder.build_block(attrs)?;
    // ```
    //
    // For now, this returns a placeholder indicating execution is not yet implemented.
    Err(ExecutorError::ExecutionFailed(
        "EVM execution not yet fully integrated - block validation only".to_string(),
    ))
}

/// Verify that a block's execution results match the expected values.
///
/// This compares the computed roots from EVM execution against the
/// values in the block header.
///
/// # Arguments
///
/// * `expected_state_root` - The state root from the block header
/// * `expected_receipts_root` - The receipts root from the block header
/// * `actual` - The actual execution result
///
/// # Errors
///
/// Returns an error if either root doesn't match.
pub fn verify_execution_result(
    expected_state_root: B256,
    expected_receipts_root: B256,
    actual: &BlockExecutionResult,
) -> Result<(), ExecutorError> {
    if actual.state_root != expected_state_root {
        return Err(ExecutorError::InvalidStateRoot {
            expected: expected_state_root,
            computed: actual.state_root,
        });
    }

    if actual.receipts_root != expected_receipts_root {
        return Err(ExecutorError::InvalidReceiptHash {
            expected: expected_receipts_root,
            computed: actual.receipts_root,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enclave_trie_hinter() {
        let hinter = EnclaveTrieHinter;

        // All hint methods should succeed (no-op)
        assert!(hinter.hint_trie_node(B256::ZERO).is_ok());
        assert!(hinter.hint_account_proof(Address::ZERO, 0).is_ok());
        assert!(hinter.hint_storage_proof(Address::ZERO, U256::ZERO, 0).is_ok());
    }

    #[test]
    fn test_verify_execution_result_success() {
        let state_root = B256::repeat_byte(0xAA);
        let receipts_root = B256::repeat_byte(0xBB);

        let result = BlockExecutionResult {
            state_root,
            receipts_root,
            gas_used: 21000,
        };

        assert!(verify_execution_result(state_root, receipts_root, &result).is_ok());
    }

    #[test]
    fn test_verify_execution_result_state_mismatch() {
        let expected_state_root = B256::repeat_byte(0xAA);
        let actual_state_root = B256::repeat_byte(0xCC);
        let receipts_root = B256::repeat_byte(0xBB);

        let result = BlockExecutionResult {
            state_root: actual_state_root,
            receipts_root,
            gas_used: 21000,
        };

        let err = verify_execution_result(expected_state_root, receipts_root, &result);
        assert!(matches!(err, Err(ExecutorError::InvalidStateRoot { .. })));
    }

    #[test]
    fn test_verify_execution_result_receipts_mismatch() {
        let state_root = B256::repeat_byte(0xAA);
        let expected_receipts_root = B256::repeat_byte(0xBB);
        let actual_receipts_root = B256::repeat_byte(0xDD);

        let result = BlockExecutionResult {
            state_root,
            receipts_root: actual_receipts_root,
            gas_used: 21000,
        };

        let err = verify_execution_result(state_root, expected_receipts_root, &result);
        assert!(matches!(err, Err(ExecutorError::InvalidReceiptHash { .. })));
    }
}
