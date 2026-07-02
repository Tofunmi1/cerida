ROOT := $(PWD)
CIRCUIT_KEYS := $(ROOT)/circuits/keys
CONTRACT_TARGET := $(ROOT)/contracts/target/wasm32v1-none/release

.PHONY: all clean \
	circuit-setup \
	build-contracts build-orderbook build-perp-engine build-shielded-pool \
	build-tools \
	deploy deploy-orderbook deploy-perp-engine \
	e2e ci-check hooks

all: circuit-setup build-contracts build-tools

# ======== Circuits ========
circuit-setup:
	$(MAKE) -C circuits setup-all

# ======== Contracts ========
build-orderbook:
	VK_COMMIT_JSON=$(CIRCUIT_KEYS)/order_commitment_vk.json \
	VK_CANCEL_JSON=$(CIRCUIT_KEYS)/order_cancel_vk.json \
	  cargo build --target wasm32v1-none --release -p orderbook
	wasm-opt -Oz --strip-debug --strip-producers --strip-target-features \
	  $(ROOT)/target/wasm32v1-none/release/orderbook.wasm \
	  -o $(ROOT)/target/wasm32v1-none/release/orderbook.wasm
	ls -la $(ROOT)/target/wasm32v1-none/release/orderbook.wasm

build-perp-engine:
	VK_COMMIT_JSON=$(CIRCUIT_KEYS)/order_commitment_vk.json \
	VK_CANCEL_JSON=$(CIRCUIT_KEYS)/order_cancel_vk.json \
	VK_MATCH_JSON=$(CIRCUIT_KEYS)/order_match_vk.json \
	VK_NOTE_SPEND_JSON=$(CIRCUIT_KEYS)/note_spend_vk.json \
	  cargo build --target wasm32v1-none --release -p perp-engine
	wasm-opt -Oz --strip-debug --strip-producers \
	  $(ROOT)/target/wasm32v1-none/release/perp_engine.wasm \
	  -o $(ROOT)/target/wasm32v1-none/release/perp_engine.wasm
	ls -la $(ROOT)/target/wasm32v1-none/release/perp_engine.wasm

build-shielded-pool:
	VK_POOL_INSERT_JSON=$(CIRCUIT_KEYS)/shielded_insert_vk.json \
	VK_POOL_WITHDRAW_JSON=$(CIRCUIT_KEYS)/shielded_withdraw_vk.json \
	  cargo build --target wasm32v1-none --release -p shielded-pool
	wasm-opt -Oz --strip-debug --strip-producers \
	  $(ROOT)/target/wasm32v1-none/release/shielded_pool.wasm \
	  -o $(ROOT)/target/wasm32v1-none/release/shielded_pool.wasm
	ls -la $(ROOT)/target/wasm32v1-none/release/shielded_pool.wasm

build-contracts: build-orderbook build-perp-engine build-shielded-pool

# ======== Tools ========
build-tools:
	cargo build --release --manifest-path tools/rust-circuits/Cargo.toml
	cargo build --release --manifest-path tools/e2e/Cargo.toml

# ======== Deploy ========
deploy-orderbook: build-orderbook
	cargo run --release -p e2e -- deploy

deploy-perp-engine: build-perp-engine
	stellar contract deploy \
	  --wasm $(ROOT)/target/wasm32v1-none/release/perp_engine.wasm \
	  --source e2e \
	  --network testnet

deploy: deploy-orderbook deploy-perp-engine

# ======== E2E ========
e2e: build-contracts build-tools
	cargo run --release --manifest-path tools/e2e/Cargo.toml -- --keys-dir circuits/keys --wasm-dir target/wasm32v1-none/release full

# ======== CI ========
ci-check:
	cargo fmt --all -- --check
	VK_COMMIT_JSON=$(CIRCUIT_KEYS)/order_commitment_vk.json \
	VK_CANCEL_JSON=$(CIRCUIT_KEYS)/order_cancel_vk.json \
	VK_MATCH_JSON=$(CIRCUIT_KEYS)/order_match_vk.json \
	VK_NOTE_SPEND_JSON=$(CIRCUIT_KEYS)/note_spend_vk.json \
	VK_POOL_INSERT_JSON=$(CIRCUIT_KEYS)/shielded_insert_vk.json \
	VK_POOL_WITHDRAW_JSON=$(CIRCUIT_KEYS)/shielded_withdraw_vk.json \
	  cargo clippy --all-targets -- -D warnings
	VK_COMMIT_JSON=$(CIRCUIT_KEYS)/order_commitment_vk.json \
	VK_CANCEL_JSON=$(CIRCUIT_KEYS)/order_cancel_vk.json \
	VK_MATCH_JSON=$(CIRCUIT_KEYS)/order_match_vk.json \
	VK_NOTE_SPEND_JSON=$(CIRCUIT_KEYS)/note_spend_vk.json \
	VK_POOL_INSERT_JSON=$(CIRCUIT_KEYS)/shielded_insert_vk.json \
	VK_POOL_WITHDRAW_JSON=$(CIRCUIT_KEYS)/shielded_withdraw_vk.json \
	  cargo test -p perp-engine -p orderbook -p types -p collateral
	cargo test -p shielded-pool
	cargo test -p rust-circuits

# ======== Git hooks ========
hooks:
	cp .githooks/pre-commit .git/hooks/pre-commit
	chmod +x .git/hooks/pre-commit
	git config core.hooksPath .githooks
	@echo "Installed pre-commit hook (.githooks/pre-commit → .git/hooks/pre-commit)"

# ======== Clean ========
clean:
	$(MAKE) -C circuits clean
	cargo clean
	rm -rf deployments
