package enclave

import (
	"context"
	"encoding/json"
	"math/big"
	"os"
	"testing"

	"github.com/ethereum-optimism/optimism/op-node/rollup"
	"github.com/ethereum-optimism/optimism/op-service/eth"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/common/hexutil"
	"github.com/ethereum/go-ethereum/core/stateless"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/params"
)

// StatelessTestFixture represents the test fixture format from Rust.
// This matches the structure in rust/op-enclave-core/tests/fixtures/base_sepolia_block.json
type StatelessTestFixture struct {
	RollupConfig     json.RawMessage    `json:"rollupConfig"`
	L1Config         json.RawMessage    `json:"l1Config"`
	L1Origin         *types.Header      `json:"l1Origin"`
	L1Receipts       []*FixtureReceipt  `json:"l1Receipts"`
	PreviousBlockTxs []hexutil.Bytes    `json:"previousBlockTxs"`
	BlockHeader      *types.Header      `json:"blockHeader"`
	SequencedTxs     []hexutil.Bytes    `json:"sequencedTxs"`
	Witness          *WitnessFixture    `json:"witness"`
	MessageAccount   *eth.AccountResult `json:"messageAccount"`
	ExpectedStateRoot    string `json:"expectedStateRoot"`
	ExpectedReceiptsRoot string `json:"expectedReceiptsRoot"`
}

// WitnessFixture represents the execution witness in the fixture format.
type WitnessFixture struct {
	Headers []*types.Header   `json:"headers"`
	Codes   map[string]string `json:"codes"`
	State   map[string]string `json:"state"`
}

// FixtureReceipt represents a receipt in the fixture format (minimal log format).
type FixtureReceipt struct {
	Type              hexutil.Uint64 `json:"type"`
	Status            hexutil.Uint64 `json:"status"`
	CumulativeGasUsed hexutil.Uint64 `json:"cumulativeGasUsed"`
	Logs              []*FixtureLog  `json:"logs"`
}

// FixtureLog represents a log in the fixture format (without transactionHash).
type FixtureLog struct {
	Address common.Address `json:"address"`
	Topics  []common.Hash  `json:"topics"`
	Data    hexutil.Bytes  `json:"data"`
}

// ToReceipt converts a FixtureReceipt to types.Receipt.
func (r *FixtureReceipt) ToReceipt() *types.Receipt {
	logs := make([]*types.Log, len(r.Logs))
	for i, l := range r.Logs {
		logs[i] = &types.Log{
			Address: l.Address,
			Topics:  l.Topics,
			Data:    l.Data,
		}
	}
	receipt := &types.Receipt{
		Type:              uint8(r.Type),
		Status:            uint64(r.Status),
		CumulativeGasUsed: uint64(r.CumulativeGasUsed),
		Logs:              logs,
	}
	// Derive bloom filter from logs
	receipt.Bloom = types.CreateBloom(receipt)
	return receipt
}

// ToReceipts converts fixture receipts to types.Receipts.
func toReceipts(fr []*FixtureReceipt) types.Receipts {
	receipts := make(types.Receipts, len(fr))
	for i, r := range fr {
		receipts[i] = r.ToReceipt()
	}
	return receipts
}

// TestExecuteStatelessWithRustFixture loads the Rust fixture and runs Go's ExecuteStateless.
// This test validates whether Go execution succeeds with the same witness data that Rust uses.
func TestExecuteStatelessWithRustFixture(t *testing.T) {
	// Path to the Rust fixture
	fixturePath := "../../rust/op-enclave-core/tests/fixtures/base_sepolia_block.json"

	// Read the fixture
	data, err := os.ReadFile(fixturePath)
	if err != nil {
		t.Skipf("Fixture not found at %s: %v", fixturePath, err)
	}

	var fixture StatelessTestFixture
	if err := json.Unmarshal(data, &fixture); err != nil {
		t.Fatalf("Failed to parse fixture: %v", err)
	}

	t.Logf("Loaded fixture:")
	t.Logf("  Block number: %d", fixture.BlockHeader.Number.Uint64())
	t.Logf("  Block timestamp: %d", fixture.BlockHeader.Time)
	t.Logf("  Headers: %d", len(fixture.Witness.Headers))
	t.Logf("  Codes: %d", len(fixture.Witness.Codes))
	t.Logf("  State nodes: %d", len(fixture.Witness.State))
	t.Logf("  L1 receipts: %d", len(fixture.L1Receipts))
	t.Logf("  Previous block txs: %d", len(fixture.PreviousBlockTxs))
	t.Logf("  Sequenced txs: %d", len(fixture.SequencedTxs))

	// Transform the witness to go-ethereum's format
	// Go's transformMap only uses the values (raw bytes), not the keys (hashes)
	codes, err := transformMap(fixture.Witness.Codes)
	if err != nil {
		t.Fatalf("Failed to transform codes: %v", err)
	}
	state, err := transformMap(fixture.Witness.State)
	if err != nil {
		t.Fatalf("Failed to transform state: %v", err)
	}

	witness := &stateless.Witness{
		Headers: fixture.Witness.Headers,
		Codes:   codes,
		State:   state,
	}

	// Use Base Sepolia chain config
	config := params.SepoliaChainConfig

	// Get the rollup config for Base Sepolia
	rollupConfig := baseSepoliaRollupConfig()

	// Convert fixture receipts to Go's types.Receipts
	l1Receipts := toReceipts(fixture.L1Receipts)

	// Execute stateless
	err = ExecuteStateless(
		context.Background(),
		config,
		rollupConfig,
		fixture.L1Origin,
		l1Receipts,
		fixture.PreviousBlockTxs,
		fixture.BlockHeader,
		fixture.SequencedTxs,
		witness,
		fixture.MessageAccount,
	)

	if err != nil {
		t.Fatalf("ExecuteStateless failed: %v", err)
	}

	t.Log("Go ExecuteStateless succeeded!")
}

// baseSepoliaRollupConfig returns the rollup config for Base Sepolia.
// This matches the config used in the Rust fixture.
func baseSepoliaRollupConfig() *rollup.Config {
	regolithTime := uint64(0)
	canyonTime := uint64(1699981200)
	deltaTime := uint64(1703203200)
	ecotoneTime := uint64(1708534800)
	fjordTime := uint64(1716998400)
	graniteTime := uint64(1723478400)
	holoceneTime := uint64(1732633200)
	isthmusTime := uint64(1744905600)

	return &rollup.Config{
		Genesis: rollup.Genesis{
			L1: eth.BlockID{
				Hash:   common.HexToHash("0xcac9a83291d4dec146d6f7f69ab2304f23f5be87b1789119a0c5b1e4482444ed"),
				Number: 4370868,
			},
			L2: eth.BlockID{
				Hash:   common.HexToHash("0x0dcc9e089e30b90ddfc55be9a37dd15bc551aeee999d2e2b51414c54eaf934e4"),
				Number: 0,
			},
			L2Time: 1695768288,
			SystemConfig: eth.SystemConfig{
				BatcherAddr: common.HexToAddress("0x6CDEbe940BC0F26850285cacA097C11c33103E47"),
				GasLimit:    25_000_000,
			},
		},
		BlockTime:              2,
		MaxSequencerDrift:      600,
		SeqWindowSize:          3600,
		ChannelTimeoutBedrock:  300,
		L1ChainID:              big.NewInt(11155111),
		L2ChainID:              big.NewInt(84532),
		BatchInboxAddress:      common.HexToAddress("0xfF00000000000000000000000000000000084532"),
		DepositContractAddress: common.HexToAddress("0x49f53e41452C74589E85cA1677426Ba426459e85"),
		L1SystemConfigAddress:  common.HexToAddress("0xf272670eb55e895584501d564AfEB048bEd26194"),
		RegolithTime:           &regolithTime,
		CanyonTime:             &canyonTime,
		DeltaTime:              &deltaTime,
		EcotoneTime:            &ecotoneTime,
		FjordTime:              &fjordTime,
		GraniteTime:            &graniteTime,
		HoloceneTime:           &holoceneTime,
		IsthmusTime:            &isthmusTime,
	}
}
