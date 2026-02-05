//! Operator fee parameter encoding.
//!
//! This module provides utilities for encoding operator fee parameters
//! into a format compatible with the Optimism protocol.

use alloy_primitives::B256;

/// Encodes operator fee parameters into a 32-byte value.
///
/// The encoding format is:
/// - Bytes 0-19: zeros (padding)
/// - Bytes 20-24: scalar (big-endian u32)
/// - Bytes 24-32: constant (big-endian u64)
///
/// This matches the Go implementation in `l2_system_config_fetcher.go`.
#[must_use]
pub fn encode_operator_fee_params(scalar: u32, constant: u64) -> B256 {
    let mut encoded = [0u8; 32];
    encoded[20..24].copy_from_slice(&scalar.to_be_bytes());
    encoded[24..32].copy_from_slice(&constant.to_be_bytes());
    B256::from(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_operator_fee_params_zeros() {
        let result = encode_operator_fee_params(0, 0);
        assert_eq!(result, B256::ZERO);
    }

    #[test]
    fn test_encode_operator_fee_params_scalar_only() {
        let result = encode_operator_fee_params(0x12345678, 0);
        let mut expected = [0u8; 32];
        expected[20..24].copy_from_slice(&[0x12, 0x34, 0x56, 0x78]);
        assert_eq!(result, B256::from(expected));
    }

    #[test]
    fn test_encode_operator_fee_params_constant_only() {
        let result = encode_operator_fee_params(0, 0x123456789ABCDEF0);
        let mut expected = [0u8; 32];
        expected[24..32].copy_from_slice(&[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0]);
        assert_eq!(result, B256::from(expected));
    }

    #[test]
    fn test_encode_operator_fee_params_both() {
        let result = encode_operator_fee_params(0xDEADBEEF, 0xCAFEBABE12345678);
        let mut expected = [0u8; 32];
        expected[20..24].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        expected[24..32].copy_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE, 0x12, 0x34, 0x56, 0x78]);
        assert_eq!(result, B256::from(expected));
    }
}
