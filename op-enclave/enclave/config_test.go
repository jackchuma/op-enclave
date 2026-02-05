package enclave

import (
	"encoding/hex"
	"fmt"
	"math/big"
	"testing"

	"github.com/ethereum-optimism/optimism/op-node/rollup"
	"github.com/ethereum-optimism/optimism/op-service/eth"
	"github.com/ethereum/go-ethereum/common"
)

// TestGoldenVectors generates golden test vectors for Rust compatibility testing.
// Run with: go test -v -run TestGoldenVectors ./enclave/
func TestGoldenVectors(t *testing.T) {
	// Create config matching Rust's sample_config()
	config := &PerChainConfig{
		ChainID: big.NewInt(8453), // Base
		Genesis: rollup.Genesis{
			L1: eth.BlockID{
				Hash:   common.BytesToHash(repeatByte(0x11, 32)),
				Number: 1,
			},
			L2: eth.BlockID{
				Hash:   common.BytesToHash(repeatByte(0x22, 32)),
				Number: 0,
			},
			L2Time: 1686789600,
			SystemConfig: eth.SystemConfig{
				BatcherAddr: common.HexToAddress("0x5050f69a9786f081509234f1a7f4684b5e5b76c9"),
				Overhead:    eth.Bytes32{}, // zero
				Scalar:      eth.Bytes32{}, // zero (32 bytes of 0x00)
				GasLimit:    30_000_000,
			},
		},
		BlockTime:              1,
		DepositContractAddress: common.HexToAddress("0x49048044d57e1c92a77f79988d21fa8faf74e97e"),
		L1SystemConfigAddress:  common.HexToAddress("0x73a79fab69143498ed3712e519a88a918e1f4072"),
	}

	// Generate binary output
	binary := config.MarshalBinary()
	hash := config.Hash()

	fmt.Println("=== Golden Test Vectors for Rust ===")
	fmt.Println()
	fmt.Printf("Binary length: %d bytes\n", len(binary))
	fmt.Printf("Binary (hex): %s\n", hex.EncodeToString(binary))
	fmt.Println()
	fmt.Printf("Hash: %s\n", hash.Hex())
	fmt.Println()

	// Verify length
	if len(binary) != 212 {
		t.Errorf("Expected binary length 212, got %d", len(binary))
	}
}

func repeatByte(b byte, n int) []byte {
	result := make([]byte, n)
	for i := range result {
		result[i] = b
	}
	return result
}
