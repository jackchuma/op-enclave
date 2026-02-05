//! L2 system config fetcher implementation.
//!
//! This module provides a system config fetcher for L2 blocks, used by the
//! derivation pipeline to access the system configuration from L2 blocks.

use alloy_consensus::Header;
use alloy_primitives::{B256, Bytes, U256};
use kona_genesis::{RollupConfig, SystemConfig};
use kona_protocol::L1BlockInfoTx;

use crate::error::ProviderError;

/// A fetcher for L2 system configuration from L2 blocks.
///
/// This struct holds a single L2 block's header and first transaction data,
/// and provides methods to extract the system configuration.
/// It matches Go's `l2SystemConfigFetcher`.
#[derive(Debug, Clone)]
pub struct L2SystemConfigFetcher {
    /// The rollup configuration.
    config: RollupConfig,
    /// The block hash.
    hash: B256,
    /// The block header.
    header: Header,
    /// The first transaction's calldata (deposit tx data).
    first_tx_data: Option<Bytes>,
}

impl L2SystemConfigFetcher {
    /// Creates a new `L2SystemConfigFetcher`.
    ///
    /// # Arguments
    ///
    /// * `config` - The rollup configuration
    /// * `hash` - The block hash
    /// * `header` - The block header
    /// * `first_tx_data` - The first transaction's calldata (should be L1 info deposit)
    #[must_use]
    pub const fn new(
        config: RollupConfig,
        hash: B256,
        header: Header,
        first_tx_data: Option<Bytes>,
    ) -> Self {
        Self {
            config,
            hash,
            header,
            first_tx_data,
        }
    }

    /// Returns the system configuration for the given L2 block hash.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The hash doesn't match
    /// - The block is missing the L1 info deposit transaction
    /// - The L1 block info cannot be parsed
    /// - Genesis hash mismatch
    pub fn system_config_by_l2_hash(&self, hash: B256) -> Result<SystemConfig, ProviderError> {
        if hash != self.hash {
            return Err(ProviderError::BlockNotFound(hash));
        }
        self.block_to_system_config()
    }

    /// Extracts the system configuration from the block.
    fn block_to_system_config(&self) -> Result<SystemConfig, ProviderError> {
        let block_hash = self.hash;
        let block_number = self.header.number;
        let l2_time = self.header.timestamp;

        // Check if this is the genesis block
        if block_number == self.config.genesis.l2.number {
            // Verify genesis hash matches
            if block_hash != self.config.genesis.l2.hash {
                return Err(ProviderError::GenesisHashMismatch {
                    number: block_number,
                    expected: self.config.genesis.l2.hash,
                    actual: block_hash,
                });
            }
            // Return genesis system config
            return self
                .config
                .genesis
                .system_config
                .ok_or(ProviderError::L1InfoParseError(
                    "genesis system config not set".to_string(),
                ));
        }

        // Non-genesis block: parse L1 info from deposit tx
        let tx_data = self
            .first_tx_data
            .as_ref()
            .ok_or(ProviderError::MissingL1InfoDeposit(block_hash))?;

        // Parse L1BlockInfo from the deposit transaction data
        let l1_info = L1BlockInfoTx::decode_calldata(tx_data)
            .map_err(|e| ProviderError::L1InfoParseError(e.to_string()))?;

        // Build the system config
        let mut sys_cfg = SystemConfig {
            batcher_address: l1_info.batcher_address(),
            overhead: l1_info.l1_fee_overhead(),
            scalar: U256::from_be_bytes(self.encode_fee_scalar(&l1_info, l2_time).0),
            gas_limit: self.header.gas_limit,
            base_fee_scalar: None,
            blob_base_fee_scalar: None,
            eip1559_denominator: None,
            eip1559_elasticity: None,
            operator_fee_scalar: None,
            operator_fee_constant: None,
        };

        // Add Isthmus operator fee params
        if is_isthmus_but_not_first_block(&self.config, l2_time) {
            if let L1BlockInfoTx::Isthmus(info) = &l1_info {
                sys_cfg.operator_fee_scalar = Some(info.operator_fee_scalar);
                sys_cfg.operator_fee_constant = Some(info.operator_fee_constant);
            }
        }

        Ok(sys_cfg)
    }

    /// Encodes the fee scalar for the system config.
    ///
    /// For Ecotone+ blocks (not activation block), translates the scalar fields
    /// back into the encoded scalar format.
    fn encode_fee_scalar(&self, l1_info: &L1BlockInfoTx, l2_time: u64) -> B256 {
        if is_ecotone_but_not_first_block(&self.config, l2_time) {
            // Encode v1 scalar format:
            // byte 0: version (1)
            // bytes 24-28: blob_base_fee_scalar (big-endian u32)
            // bytes 28-32: base_fee_scalar (big-endian u32)
            let mut encoded = [0u8; 32];
            encoded[0] = 1; // version 1

            // Convert U256 to u32 - these are originally u32 values
            let blob_scalar: u32 = l1_info
                .blob_base_fee_scalar()
                .try_into()
                .unwrap_or(u32::MAX);
            let base_scalar: u32 = l1_info.l1_fee_scalar().try_into().unwrap_or(u32::MAX);

            encoded[24..28].copy_from_slice(&blob_scalar.to_be_bytes());
            encoded[28..32].copy_from_slice(&base_scalar.to_be_bytes());
            B256::from(encoded)
        } else {
            // Pre-Ecotone or Ecotone activation block: use raw scalar from L1FeeScalar
            B256::from(l1_info.l1_fee_scalar().to_be_bytes::<32>())
        }
    }

    /// Returns the block hash.
    #[must_use]
    pub const fn hash(&self) -> B256 {
        self.hash
    }

    /// Returns a reference to the header.
    #[must_use]
    pub const fn header(&self) -> &Header {
        &self.header
    }

    /// Returns a reference to the rollup config.
    #[must_use]
    pub const fn config(&self) -> &RollupConfig {
        &self.config
    }
}

/// Checks if Ecotone is active but this is not the activation block.
const fn is_ecotone_but_not_first_block(config: &RollupConfig, l2_time: u64) -> bool {
    is_fork_active_but_not_activation(config.hardforks.ecotone_time, l2_time)
}

/// Checks if Isthmus is active but this is not the activation block.
const fn is_isthmus_but_not_first_block(config: &RollupConfig, l2_time: u64) -> bool {
    is_fork_active_but_not_activation(config.hardforks.isthmus_time, l2_time)
}

/// Helper to check if a fork is active but not the activation block.
const fn is_fork_active_but_not_activation(fork_time: Option<u64>, l2_time: u64) -> bool {
    match fork_time {
        Some(activation_time) => {
            // Fork is active if l2_time >= activation_time
            // Not activation block if l2_time > activation_time
            l2_time >= activation_time && l2_time > activation_time
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_rollup_config;
    use crate::providers::test_utils::test_header;

    #[test]
    fn test_genesis_block_returns_genesis_config() {
        let mut config = default_rollup_config();
        let genesis_hash = B256::repeat_byte(0xAA);
        config.genesis.l2.number = 0;
        config.genesis.l2.hash = genesis_hash;

        let header = test_header(0, 0);
        let fetcher = L2SystemConfigFetcher::new(config.clone(), genesis_hash, header, None);

        let result = fetcher.system_config_by_l2_hash(genesis_hash);
        assert!(result.is_ok());

        let sys_cfg = result.unwrap();
        assert_eq!(
            sys_cfg.gas_limit,
            config.genesis.system_config.unwrap().gas_limit
        );
    }

    #[test]
    fn test_genesis_hash_mismatch() {
        let mut config = default_rollup_config();
        config.genesis.l2.number = 0;
        config.genesis.l2.hash = B256::repeat_byte(0xAA);

        let wrong_hash = B256::repeat_byte(0xBB);
        let header = test_header(0, 0);
        let fetcher = L2SystemConfigFetcher::new(config, wrong_hash, header, None);

        let result = fetcher.system_config_by_l2_hash(wrong_hash);
        assert!(matches!(
            result,
            Err(ProviderError::GenesisHashMismatch { .. })
        ));
    }

    #[test]
    fn test_block_not_found() {
        let config = default_rollup_config();
        let hash = B256::repeat_byte(0xAA);
        let wrong_hash = B256::repeat_byte(0xBB);
        let header = test_header(100, 1000);
        let fetcher = L2SystemConfigFetcher::new(config, hash, header, None);

        let result = fetcher.system_config_by_l2_hash(wrong_hash);
        assert!(matches!(result, Err(ProviderError::BlockNotFound(h)) if h == wrong_hash));
    }

    #[test]
    fn test_missing_l1_info_deposit() {
        let mut config = default_rollup_config();
        config.genesis.l2.number = 0;
        config.genesis.l2.hash = B256::repeat_byte(0x00);

        let hash = B256::repeat_byte(0xAA);
        let header = test_header(100, 1000); // Not genesis block
        let fetcher = L2SystemConfigFetcher::new(config, hash, header, None);

        let result = fetcher.system_config_by_l2_hash(hash);
        assert!(matches!(
            result,
            Err(ProviderError::MissingL1InfoDeposit(_))
        ));
    }

    #[test]
    fn test_fork_detection_not_active() {
        assert!(!is_fork_active_but_not_activation(None, 100));
    }

    #[test]
    fn test_fork_detection_at_activation() {
        // At exactly activation time, should return false (is activation block)
        assert!(!is_fork_active_but_not_activation(Some(100), 100));
    }

    #[test]
    fn test_fork_detection_after_activation() {
        // After activation time, should return true
        assert!(is_fork_active_but_not_activation(Some(100), 102));
    }

    #[test]
    fn test_fork_detection_before_activation() {
        assert!(!is_fork_active_but_not_activation(Some(100), 50));
    }
}
