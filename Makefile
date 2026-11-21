.PHONY: all build test test-verification clean fmt lint setup deploy-local deploy-testnet deploy-mainnet deploy-secure wasm-sizes wasm-opt-all

# Default target
all: build test

# Build all contracts
build:
	cargo build --all-targets
	cargo build --target wasm32-unknown-unknown --release

# Build optimized for production
build-release:
	cargo build --target wasm32-unknown-unknown --release
	@echo "Optimized WASM files are in target/wasm32-unknown-unknown/release/"

# Report WASM binary sizes for all contracts and flag any near the Soroban limit.
# Soroban enforces a hard limit of 65,536 bytes per WASM binary (64 KB).
# Budget guidelines (after wasm-opt -Oz):
#   access_control   : target ≤ 20 KB
#   ai_integration   : target ≤ 30 KB
#   analytics        : target ≤ 25 KB
#   audit            : target ≤ 20 KB
#   clinical_records : target ≤ 40 KB
#   common           : library – not deployed directly
#   compliance       : target ≤ 25 KB
#   cross_chain      : target ≤ 30 KB
#   delegation       : target ≤ 20 KB
#   emr_bridge       : target ≤ 25 KB
#   events           : target ≤ 15 KB
#   fhir             : target ≤ 30 KB
#   governor         : target ≤ 25 KB
#   identity         : target ≤ 25 KB
#   key_manager      : target ≤ 20 KB
#   metering         : target ≤ 20 KB
#   orchestrator     : target ≤ 30 KB
#   staking          : target ≤ 25 KB
#   state_channel    : target ≤ 25 KB
#   timelock         : target ≤ 20 KB
#   treasury         : target ≤ 25 KB
#   vault            : target ≤ 25 KB
#   vision_records   : target ≤ 55 KB  (largest – monitor closely)
#   zk_verifier      : target ≤ 30 KB
#   zk_voting        : target ≤ 30 KB
# Run `make wasm-sizes` after every release build to verify budgets.
WASM_DIR := target/wasm32-unknown-unknown/release
WASM_LIMIT_BYTES := 65536
wasm-sizes: build-release
	@echo "=== WASM binary sizes (Soroban limit: $(WASM_LIMIT_BYTES) bytes) ==="
	@for f in $(WASM_DIR)/*.wasm; do \
		size=$$(wc -c < "$$f"); \
		name=$$(basename "$$f"); \
		if [ "$$size" -gt "$(WASM_LIMIT_BYTES)" ]; then \
			echo "  OVER LIMIT  $$name: $$size bytes"; \
		elif [ "$$size" -gt "58000" ]; then \
			echo "  NEAR LIMIT  $$name: $$size bytes"; \
		else \
			echo "  OK          $$name: $$size bytes"; \
		fi; \
	done

# Run wasm-opt (from binaryen) over every contract binary and report size reduction.
# Install binaryen: https://github.com/WebAssembly/binaryen#building
# Usage: make wasm-opt-all   (requires wasm-opt on PATH)
wasm-opt-all: build-release
	@command -v wasm-opt >/dev/null 2>&1 || { echo "wasm-opt not found – install binaryen"; exit 1; }
	@echo "=== Running wasm-opt -Oz on all contracts ==="
	@for f in $(WASM_DIR)/*.wasm; do \
		before=$$(wc -c < "$$f"); \
		wasm-opt -Oz "$$f" -o "$$f"; \
		after=$$(wc -c < "$$f"); \
		echo "  $$(basename $$f): $$before → $$after bytes"; \
	done

# Run all tests
test:
	cargo test --all
	node --test website/*.test.js website/components/*.test.js

# Run JS micro-tests
test-js:
	node --test website/*.test.js website/components/*.test.js

# Run unit tests only
test-unit:
	cargo test --lib

# Run integration tests only
test-integration:
	cargo test --test '*'

# Consolidated package verification previously provided by verify_all_tests scripts.
test-verification:
	cargo check --all-targets
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test --all
	node --test website/*.test.js website/components/*.test.js

# Clean build artifacts
clean:
	cargo clean

# Format code
fmt:
	cargo fmt --all

# Check formatting
fmt-check:
	cargo fmt --all -- --check

# Run clippy linter
lint:
	cargo clippy --all-targets --all-features -- -D warnings

# Run clippy with fixes
lint-fix:
	cargo clippy --fix --allow-dirty --allow-staged

# Setup development environment
setup:
	rustup target add wasm32-unknown-unknown
	rustup component add rustfmt clippy rust-src
	cargo install --locked soroban-cli || true
	@echo "Setup complete!"

# Start local Soroban network
start-local:
	soroban network start local

# Stop local Soroban network
stop-local:
	soroban network stop local

# Deploy to local network
deploy-local:
	./scripts/deploy.sh local

# Deploy to testnet
deploy-testnet:
	./scripts/deploy.sh testnet

# Deploy to mainnet (requires confirmation)
deploy-mainnet:
	@echo "WARNING: Deploying to mainnet!"
	@read -p "Are you sure? (yes/no): " confirm && [ "$$confirm" = "yes" ]
	./scripts/deploy.sh mainnet

# Secure deploy with admin transfer (least-privilege)
# Usage: ADMIN_ADDRESS=GXXX... NETWORK=testnet make deploy-secure
deploy-secure:
ifndef ADMIN_ADDRESS
	$(error ADMIN_ADDRESS is required. Usage: ADMIN_ADDRESS=GXXX... make deploy-secure)
endif
	./scripts/deploy.sh $(or $(NETWORK),testnet) vision_records --admin $(ADMIN_ADDRESS)

# Generate documentation
docs:
	cargo doc --no-deps --open

# Check everything before commit
check: fmt-check lint test
	@echo "All checks passed!"

# Watch for changes and run tests
watch:
	cargo watch -x test

# Display help
help:
	@echo "Available targets:"
	@echo "  all           - Build and test"
	@echo "  build         - Build all contracts"
	@echo "  build-release - Build optimized WASM"
	@echo "  wasm-sizes    - Report WASM sizes and flag contracts near Soroban limit"
	@echo "  wasm-opt-all  - Run wasm-opt -Oz on all contracts (requires binaryen)"
	@echo "  test          - Run all tests"
	@echo "  test-unit     - Run unit tests"
	@echo "  test-integration - Run integration tests"
	@echo "  test-verification - Run consolidated verification suite"
	@echo "  clean         - Clean build artifacts"
	@echo "  fmt           - Format code"
	@echo "  lint          - Run clippy"
	@echo "  setup         - Setup dev environment"
	@echo "  deploy-local  - Deploy to local network"
	@echo "  deploy-testnet - Deploy to testnet"
	@echo "  deploy-secure - Deploy with admin transfer (least-privilege)"
	@echo "  check         - Run all checks"
