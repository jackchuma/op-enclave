//! L1/L2 data providers for kona-derive integration.
//!
//! This module provides implementations of data providers used by the
//! derivation pipeline to access L1 and L2 block data.

mod block_info;
mod l1_receipts;
mod l2_system_config;
mod trie;

pub use block_info::BlockInfoWrapper;
pub use l1_receipts::L1ReceiptsFetcher;
pub use l2_system_config::L2SystemConfigFetcher;
pub use trie::{compute_receipt_root, compute_tx_root};
