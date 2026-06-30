ROOT := $(PWD)
CIRCUIT_KEYS := $(ROOT)/circuits/keys
CONTRACT_TARGET := $(ROOT)/contracts/target/wasm32v1-none/release

.PHONY: all clean \
	circuit-setup \
	build-contracts build-orderbook build-perp-engine \
	build-tools \
	deploy deploy-orderbook deploy-perp-engine \
	e2e

all: circuit-setup build-contracts build-tools

# ======== Circuits ========
circuit-setup:
	$(MAKE) -C circuits setup-all

# ======== Contracts ========
build-orderbook:
	VK_COMMIT_JSON=$(CIRCUIT_KEYS)/order_commitment_vk.json \
	VK_CANCEL_JSON=$(CIRCUIT_KEYS)/order_cancel_vk.json \
	  cargo build --target wasm32v1-none --release -p orderbook -p verifier-groth16 -p types
	ls -la $(ROOT)/target/wasm32v1-none/release/orderbook.wasm

build-perp-engine:
	VK_COMMIT_JSON=$(CIRCUIT_KEYS)/order_commitment_vk.json \
	VK_CANCEL_JSON=$(CIRCUIT_KEYS)/order_cancel_vk.json \
	VK_MATCH_JSON=$(CIRCUIT_KEYS)/order_match_vk.json \
	VK_NOTE_SPEND_JSON=$(CIRCUIT_KEYS)/note_spend_vk.json \
	  cargo build --target wasm32v1-none --release -p perp-engine
	ls -la $(ROOT)/target/wasm32v1-none/release/perp_engine.wasm

build-contracts: build-orderbook build-perp-engine

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

# ======== Clean ========
clean:
	$(MAKE) -C circuits clean
	cargo clean
	rm -rf deployments
