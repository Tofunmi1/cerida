.PHONY: build-contracts build-verifier deploy e2e-test clean circuit setup full
.PHONY: sp1-install sp1-build sp1-vkey build-contracts-sp1 e2e-sp1

TINY_DIR := $(PWD)
SP1_PROVER_DIR := $(TINY_DIR)/sp1-prover
SP1_ELF := $(SP1_PROVER_DIR)/elf/private-payment-sp1

# ── Circom path ───────────────────────────────────────────────────────────────

build-verifier: circuit-keys/verification_key.json
	VERIFIER_VK_JSON=$(TINY_DIR)/circuit-keys/verification_key.json \
	  cargo build --target wasm32v1-none --release -p tiny-verifier

build-contracts: build-verifier
	cargo build --target wasm32v1-none --release -p tiny-pool

deploy: build-contracts
	./scripts/deploy.sh

e2e-test:
	cd e2e && cargo run --release -- full --amount 1000000 --secret 12345

clean:
	cargo clean
	cd sp1-prover && cargo clean
	rm -rf circuit-keys $(SP1_PROVER_DIR)/elf

# Circom circuit build
circuit:
	cd circuits && circom src/main.circom --r1cs --wasm --sym -o ../circuit-keys

# Groth16 trusted setup
SNARKJS := npx snarkjs
setup: circuit
	cd circuit-keys && \
	  $(SNARKJS) powersoftau new bn254 14 pot12_0000.ptau && \
	  $(SNARKJS) powersoftau contribute pot12_0000.ptau pot12_0001.ptau --name="First contribution" -v && \
	  $(SNARKJS) powersoftau prepare phase2 pot12_0001.ptau pot12_final.ptau && \
	  $(SNARKJS) groth16 setup main.r1cs pot12_final.ptau main.zkey && \
	  $(SNARKJS) zkey export verificationkey main.zkey verification_key.json && \
	  echo "✓ Circuit setup complete"

full: setup build-contracts deploy

# ── SP1 path ──────────────────────────────────────────────────────────────────

# Install SP1 toolchain (run once)
sp1-install:
	curl -L https://sp1.succinct.xyz | bash && sp1up

# Build SP1 guest ELF (requires SP1 toolchain: make sp1-install)
sp1-build:
	cd $(SP1_PROVER_DIR) && cargo prove build
	mkdir -p $(SP1_PROVER_DIR)/elf
	cp $(SP1_PROVER_DIR)/target/elf-compilation/riscv32im-succinct-zkvm-elf/release/private-payment-sp1 \
	   $(SP1_ELF)
	@echo "✓ SP1 ELF built: $(SP1_ELF)"

# Extract vkey hash from the built ELF
# Outputs: 64-char hex string (the SP1 program's verifying key hash)
sp1-vkey: $(SP1_ELF)
	cd $(SP1_PROVER_DIR) && cargo run --release --bin sp1-host -- \
	  --amount 0 --secret 0 --vkey

# Build SP1 host binary
sp1-host-bin: $(SP1_ELF)
	cd $(SP1_PROVER_DIR) && cargo build --release --bin sp1-host

# Build pool contract with real SP1 VK embedded.
# Usage: make build-contracts-sp1 SP1_VK_JSON=<path> SP1_VKEY_HASH=<hex>
build-contracts-sp1:
ifndef SP1_VK_JSON
	$(error SP1_VK_JSON is required. Run: SP1_VK_JSON=... SP1_VKEY_HASH=... make build-contracts-sp1)
endif
ifndef SP1_VKEY_HASH
	$(error SP1_VKEY_HASH is required)
endif
	SP1_VK_JSON=$(SP1_VK_JSON) SP1_VKEY_HASH=$(SP1_VKEY_HASH) \
	  cargo build --target wasm32v1-none --release -p tiny-pool

# Build pool contract with placeholder VK (development mode — no real SP1 needed)
build-contracts-dev:
	cargo build --target wasm32v1-none --release -p tiny-pool

# Full SP1 e2e (placeholder VK, mock proof — instant, no SP1 toolchain needed)
e2e-sp1-mock:
	$(MAKE) build-contracts-dev
	cd e2e && cargo run --release -- sp1 --amount 1000000 --secret 12345

# Full SP1 e2e with real Groth16 proof (requires SP1 toolchain + ~90s)
# Run after: make sp1-build + set SP1_VK_JSON + SP1_VKEY_HASH
e2e-sp1:
ifndef SP1_VK_JSON
	$(error Set SP1_VK_JSON and SP1_VKEY_HASH first)
endif
	$(MAKE) build-contracts-sp1 SP1_VK_JSON=$(SP1_VK_JSON) SP1_VKEY_HASH=$(SP1_VKEY_HASH)
	cd e2e && cargo run --release -- sp1 --amount 1000000 --secret 12345 --real
