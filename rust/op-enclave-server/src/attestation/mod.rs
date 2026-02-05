//! Attestation verification for AWS Nitro Enclaves.
//!
//! This module provides:
//! - [`AwsCaRoot`]: AWS CA root certificate handling
//! - [`verify_attestation`]: Attestation document verification
//! - [`verify_attestation_with_pcr0`]: Verification with PCR0 check

mod ca_roots;
mod verify;

pub use ca_roots::{get_default_ca_root, AwsCaRoot, DEFAULT_CA_ROOTS, DEFAULT_CA_ROOTS_SHA256};
pub use verify::{
    extract_public_key, verify_attestation, verify_attestation_with_pcr0, AttestationDocument,
    VerificationResult,
};
