package main

import (
	"encoding/hex"
	"fmt"
	"math/big"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
)

func main() {
	// Hardhat account #0 - same key used in Rust tests
	privateKeyHex := "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
	privateKey, err := crypto.HexToECDSA(privateKeyHex)
	if err != nil {
		panic(err)
	}

	// Test values matching Rust's signing_test_vectors
	configHash := common.HexToHash("0x1111111111111111111111111111111111111111111111111111111111111111")
	l1OriginHash := common.HexToHash("0x2222222222222222222222222222222222222222222222222222222222222222")
	l2BlockNumber := big.NewInt(12345)
	prevOutputRoot := common.HexToHash("0x3333333333333333333333333333333333333333333333333333333333333333")
	outputRoot := common.HexToHash("0x4444444444444444444444444444444444444444444444444444444444444444")

	// Build signing data (matching server.go format)
	var signingData []byte
	signingData = append(signingData, configHash.Bytes()...)
	signingData = append(signingData, l1OriginHash.Bytes()...)
	signingData = append(signingData, common.BytesToHash(l2BlockNumber.Bytes()).Bytes()...)
	signingData = append(signingData, prevOutputRoot.Bytes()...)
	signingData = append(signingData, outputRoot.Bytes()...)

	hash := crypto.Keccak256(signingData)
	sig, err := crypto.Sign(hash, privateKey)
	if err != nil {
		panic(err)
	}

	pubKeyBytes := crypto.FromECDSAPub(&privateKey.PublicKey)

	fmt.Println("=== Go Test Vectors ===")
	fmt.Printf("signing_data: %s\n", hex.EncodeToString(signingData))
	fmt.Printf("hash:         %s\n", hex.EncodeToString(hash))
	fmt.Printf("signature:    %s\n", hex.EncodeToString(sig))
	fmt.Printf("public_key:   %s\n", hex.EncodeToString(pubKeyBytes))
	fmt.Println()
	fmt.Println("=== Copy to Rust test ===")
	fmt.Printf("let go_signature_hex = \"%s\";\n", hex.EncodeToString(sig))
	fmt.Printf("let go_public_key_hex = \"%s\";\n", hex.EncodeToString(pubKeyBytes))
}
