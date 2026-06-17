.PHONY: build-contracts build-verifier deploy e2e-test clean circuit setup full

TINY_DIR := $(PWD)

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
	rm -rf circuit-keys

# Build Circom circuit
circuit:
	cd circuits && circom src/main.circom --r1cs --wasm --sym -o ../circuit-keys

# Groth16 setup (uses npx snarkjs since not installed globally)
SNARKJS := npx snarkjs
setup: circuit
	cd circuit-keys && \
	  $(SNARKJS) powersoftau new bn254 14 pot12_0000.ptau && \
	  $(SNARKJS) powersoftau contribute pot12_0000.ptau pot12_0001.ptau --name="First contribution" -v && \
	  $(SNARKJS) powersoftau prepare phase2 pot12_0001.ptau pot12_final.ptau && \
	  $(SNARKJS) groth16 setup main.r1cs pot12_final.ptau main.zkey && \
	  $(SNARKJS) zkey export verificationkey main.zkey verification_key.json && \
	  echo "✓ Circuit setup complete"

# Full end-to-end
full: setup build-contracts deploy
