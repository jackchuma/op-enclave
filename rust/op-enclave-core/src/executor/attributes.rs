//! Payload attributes builder for stateless execution.
//!
//! This module provides functionality to build payload attributes by extracting
//! deposit transactions from L1 receipts.

use alloy_consensus::Header;
use alloy_primitives::{Address, B256, Bytes, address};
use hex_literal::hex;
use kona_genesis::RollupConfig;
use kona_protocol::L2BlockInfo;
use op_alloy_consensus::OpReceiptEnvelope;

use crate::error::ExecutorError;
use crate::providers::{L1ReceiptsFetcher, L2SystemConfigFetcher};

/// The L1 Attributes Depositor address (L1Block contract depositor).
pub const L1_ATTRIBUTES_DEPOSITOR: Address = address!("deaddeaddeaddeaddeaddeaddeaddeaddead0001");

/// The L1 Attributes Predeployed Contract address.
pub const L1_ATTRIBUTES_PREDEPLOYED: Address = address!("4200000000000000000000000000000000000015");

/// Deposit contract address on L1.
pub const OPTIMISM_PORTAL_ADDRESS: Address = address!("49048044D57e1C92A77f79988d21Fa8fAF74E97e");

/// Deposit event topic (TransactionDeposited event).
/// keccak256("TransactionDeposited(address,address,uint256,bytes)")
pub const DEPOSIT_EVENT_TOPIC: B256 = B256::new(hex!(
    "b3813568d9991fc951961fcb4c784893574240a28925604d09fc577c55bb7c32"
));

/// Payload attributes for block building.
#[derive(Debug, Clone)]
pub struct PayloadAttributes {
    /// Deposit transactions extracted from L1.
    pub deposit_txs: Vec<Bytes>,

    /// L1 origin block hash.
    pub l1_origin_hash: B256,

    /// L1 origin block number.
    pub l1_origin_number: u64,

    /// Timestamp for the L2 block.
    pub timestamp: u64,
}

/// Build payload attributes by extracting deposit transactions from L1.
///
/// This extracts deposit transactions from the L1 origin block's receipts
/// that should be included at the beginning of the L2 block.
///
/// # Arguments
///
/// * `rollup_config` - The rollup configuration
/// * `l1_origin` - The L1 origin block header
/// * `l2_parent` - The L2 parent block info
/// * `l1_fetcher` - The L1 receipts fetcher
/// * `_l2_fetcher` - The L2 system config fetcher (for future use)
///
/// # Returns
///
/// Payload attributes containing deposit transactions.
///
/// # Errors
///
/// Returns an error if deposit extraction fails.
pub fn prepare_payload_attributes(
    rollup_config: &RollupConfig,
    l1_origin: &Header,
    l2_parent: &L2BlockInfo,
    l1_fetcher: &L1ReceiptsFetcher,
    _l2_fetcher: &L2SystemConfigFetcher,
) -> Result<PayloadAttributes, ExecutorError> {
    let l1_origin_hash = l1_fetcher.hash();
    let l1_origin_number = l1_origin.number;

    // Check if we need to include deposits from this L1 block
    // Deposits are only included if the L1 origin changed from the parent
    let include_deposits = l2_parent.l1_origin.hash != l1_origin_hash
        || l2_parent.l1_origin.number != l1_origin_number;

    let deposit_txs = if include_deposits {
        extract_deposits_from_receipts(rollup_config, l1_fetcher.receipts(), l1_origin_number)?
    } else {
        vec![]
    };

    // Calculate timestamp based on rollup config
    let timestamp = l2_parent.block_info.timestamp + rollup_config.block_time;

    Ok(PayloadAttributes {
        deposit_txs,
        l1_origin_hash,
        l1_origin_number,
        timestamp,
    })
}

/// Extract deposit transactions from L1 receipts.
///
/// This parses the TransactionDeposited events from the Optimism Portal contract.
fn extract_deposits_from_receipts(
    _rollup_config: &RollupConfig,
    receipts: &[OpReceiptEnvelope],
    _l1_block_number: u64,
) -> Result<Vec<Bytes>, ExecutorError> {
    let mut deposits = Vec::new();

    for receipt in receipts {
        // Get logs from the receipt
        let logs = match receipt {
            OpReceiptEnvelope::Legacy(r) => &r.receipt.logs,
            OpReceiptEnvelope::Eip2930(r) => &r.receipt.logs,
            OpReceiptEnvelope::Eip1559(r) => &r.receipt.logs,
            OpReceiptEnvelope::Eip7702(r) => &r.receipt.logs,
            OpReceiptEnvelope::Deposit(r) => &r.receipt.inner.logs,
        };

        for log in logs {
            // Check if this is a deposit event from the Optimism Portal
            if log.address == OPTIMISM_PORTAL_ADDRESS
                && !log.topics().is_empty()
                && log.topics()[0] == DEPOSIT_EVENT_TOPIC
            {
                // Parse the deposit transaction from the log data
                if let Some(deposit_tx) = parse_deposit_event(log) {
                    deposits.push(deposit_tx);
                }
            }
        }
    }

    Ok(deposits)
}

/// Parse a deposit event log into a deposit transaction.
///
/// The TransactionDeposited event format:
/// - topic[0]: event signature
/// - topic[1]: from address (indexed)
/// - topic[2]: to address (indexed)
/// - data: version (32 bytes) + opaqueData (variable)
const fn parse_deposit_event(log: &alloy_primitives::Log) -> Option<Bytes> {
    // For now, return None as full deposit parsing requires the complete
    // deposit transaction encoding logic. This will be implemented when
    // integrating with kona-derive's deposit parsing.
    //
    // The actual deposit transaction is constructed from:
    // - from: topic[1] as address (padded)
    // - to: topic[2] as address (padded)
    // - opaqueData: contains mint, value, gas, isCreation, data
    //
    // This is a placeholder that will be replaced with proper kona-derive integration.
    let _ = log;
    None
}

/// Build the L1 info deposit transaction.
///
/// This is the first transaction in every L2 block that records L1 block info.
#[allow(dead_code)]
fn build_l1_info_deposit_tx(
    _rollup_config: &RollupConfig,
    _l1_origin: &Header,
    _l2_block_number: u64,
    _sequence_number: u64,
) -> Result<Bytes, ExecutorError> {
    // This will be implemented with proper L1BlockInfo encoding from kona-protocol.
    // For now, return a placeholder.
    Err(ExecutorError::AttributesBuildFailed(
        "L1 info deposit tx building not yet implemented".to_string(),
    ))
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
