ROOT := $(PWD)
CIRCUIT_KEYS := $(ROOT)/circuits/keys
CONTRACT_TARGET := $(ROOT)/contracts/target/wasm32v1-none/release

.PHONY: all clean \
	circuit-setup \
	build-contracts build-orderbook \
	build-tools \
	deploy deploy-orderbook \
	e2e

all: circuit-setup build-contracts build-tools

# ======== Circuits ========
circuit-setup:
	$(MAKE) -C circuits setup-all

# ======== Contracts ========
build-orderbook: circuit-setup
	VK_COMMIT_JSON=$(CIRCUIT_KEYS)/order_commitment_vk.json \
	VK_CANCEL_JSON=$(CIRCUIT_KEYS)/order_cancel_vk.json \
	VK_MATCH_JSON=$(CIRCUIT_KEYS)/order_match_vk.json \
	  cargo build --target wasm32v1-none --release -p orderbook -p verifier-groth16 -p types
	ls -la $(CONTRACT_TARGET)/orderbook.wasm

build-contracts: build-orderbook

# ======== Tools ========
build-tools:
	cargo build --release -p prover
	cargo build --release -p e2e

# ======== Deploy ========
deploy-orderbook: build-orderbook
	cargo run --release -p e2e -- deploy

deploy: deploy-orderbook

# ======== E2E ========
e2e: build-contracts build-tools
	cargo run --release -p e2e -- full

# ======== Clean ========
clean:
	$(MAKE) -C circuits clean
	cargo clean
	rm -rf deployments
