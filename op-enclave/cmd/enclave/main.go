package main

import (
	"net/http"
	"os"
	"strconv"

	oplog "github.com/ethereum-optimism/optimism/op-service/log"
	"github.com/ethereum/go-ethereum/log"
	"github.com/ethereum/go-ethereum/rpc"
	enclave2 "github.com/jackchuma/op-enclave/op-enclave/enclave"
	"github.com/mdlayher/vsock"
)

const (
	// defaultHTTPBodyLimit is set to 50MB (increased from go-ethereum's default of 5MB)
	// because witness data for some blocks can exceed 5MB, causing 413 errors.
	defaultHTTPBodyLimit = 50 * 1024 * 1024
)

func main() {
	oplog.SetupDefaults()

	s := rpc.NewServer()
	// Set HTTP body limit (configurable via OP_ENCLAVE_HTTP_BODY_LIMIT env var, in bytes)
	httpBodyLimit := defaultHTTPBodyLimit
	if envLimit := os.Getenv("OP_ENCLAVE_HTTP_BODY_LIMIT"); envLimit != "" {
		if parsed, err := strconv.Atoi(envLimit); err == nil && parsed > 0 {
			httpBodyLimit = parsed
			log.Info("Using custom HTTP body limit", "bytes", httpBodyLimit)
		} else {
			log.Warn("Invalid OP_ENCLAVE_HTTP_BODY_LIMIT, using default", "value", envLimit, "default", defaultHTTPBodyLimit)
		}
	}
	s.SetHTTPBodyLimit(httpBodyLimit)

	serv, err := enclave2.NewServer()
	if err != nil {
		log.Crit("Error creating API server", "error", err)
	}
	err = s.RegisterName(enclave2.Namespace, serv)
	if err != nil {
		log.Crit("Error registering API", "error", err)
	}

	listener, err := vsock.Listen(1234, &vsock.Config{})
	if err != nil {
		log.Warn("Error opening vsock listener, running in HTTP mode", "error", err)
		err = http.ListenAndServe(":1234", s)
	} else {
		err = s.ServeListener(listener)
	}
	if err != nil {
		log.Crit("Error starting server", "error", err)
	}
}
