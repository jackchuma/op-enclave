//! Trie root computation utilities.
//!
//! This module provides functions to compute Merkle Patricia Trie roots
//! for receipts and transactions, matching Ethereum's `DeriveSha` algorithm.

use alloy_primitives::B256;
use alloy_rlp::Encodable;
use alloy_trie::{EMPTY_ROOT_HASH, HashBuilder, Nibbles};
use op_alloy_consensus::OpReceiptEnvelope;

/// Computes the receipt root from a list of receipts.
///
/// This matches Go's `types.DeriveSha(receipts, trie.NewStackTrie(nil))`.
///
/// The algorithm:
/// 1. For each receipt at index i, the key is RLP(i)
/// 2. The value is the RLP-encoded receipt (envelope format for typed receipts)
/// 3. Keys are sorted and inserted into a Merkle Patricia Trie
/// 4. Returns the root hash
#[must_use]
pub fn compute_receipt_root(receipts: &[OpReceiptEnvelope]) -> B256 {
    if receipts.is_empty() {
        return EMPTY_ROOT_HASH;
    }

    // Build key-value pairs: (RLP(index), RLP(receipt))
    let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = receipts
        .iter()
        .enumerate()
        .map(|(i, receipt)| {
            let key = encode_index(i);
            let mut value = Vec::new();
            receipt.encode(&mut value);
            (key, value)
        })
        .collect();

    // Sort by key (lexicographic order for proper trie construction)
    pairs.sort_by(|a, b| a.0.cmp(&b.0));

    // Build the trie using HashBuilder
    let mut builder = HashBuilder::default();
    for (key, value) in pairs {
        let nibbles = Nibbles::unpack(&key);
        builder.add_leaf(nibbles, &value);
    }

    builder.root()
}

/// Computes the transaction root from a list of RLP-encoded transactions.
///
/// This matches Go's `types.DeriveSha(txs, trie.NewStackTrie(nil))`.
#[must_use]
pub fn compute_tx_root(txs_rlp: &[Vec<u8>]) -> B256 {
    if txs_rlp.is_empty() {
        return EMPTY_ROOT_HASH;
    }

    // Build key-value pairs: (RLP(index), tx_rlp)
    let mut pairs: Vec<(Vec<u8>, &[u8])> = txs_rlp
        .iter()
        .enumerate()
        .map(|(i, tx)| (encode_index(i), tx.as_slice()))
        .collect();

    // Sort by key (lexicographic order for proper trie construction)
    pairs.sort_by(|a, b| a.0.cmp(&b.0));

    // Build the trie using HashBuilder
    let mut builder = HashBuilder::default();
    for (key, value) in pairs {
        let nibbles = Nibbles::unpack(&key);
        builder.add_leaf(nibbles, value);
    }

    builder.root()
}

/// RLP-encodes an index for use as a trie key.
///
/// For index 0, returns [0x80] (RLP encoding of empty byte sequence).
/// For other indices, returns the minimal RLP encoding.
fn encode_index(index: usize) -> Vec<u8> {
    let mut buf = Vec::new();
    index.encode(&mut buf);
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_receipt_root_empty() {
        let result = compute_receipt_root(&[]);
        assert_eq!(result, EMPTY_ROOT_HASH);
    }

    #[test]
    fn test_compute_tx_root_empty() {
        let result = compute_tx_root(&[]);
        assert_eq!(result, EMPTY_ROOT_HASH);
    }

    #[test]
    fn test_encode_index_zero() {
        let encoded = encode_index(0);
        // RLP encoding of 0 is 0x80 (empty byte sequence)
        assert_eq!(encoded, vec![0x80]);
    }

    #[test]
    fn test_encode_index_small() {
        // RLP encoding of small integer (< 128) is the integer itself
        let encoded = encode_index(127);
        assert_eq!(encoded, vec![127]);
    }

    #[test]
    fn test_encode_index_larger() {
        // RLP encoding of 128 is 0x8180 (string prefix + value)
        let encoded = encode_index(128);
        assert_eq!(encoded, vec![0x81, 0x80]);
    }
}
