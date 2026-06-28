CIRCUIT_DIR := $(PWD)
OUT_DIR := $(CIRCUIT_DIR)/keys
SNARKJS := npx snarkjs

.PHONY: all clean \
	order-commitment order-cancel order-match \
	setup-order-commitment setup-order-cancel setup-order-match \
	setup-all

all: setup-all

# --- Compile circuits ---

order-commitment:
	cd $(CIRCUIT_DIR) && circom src/order_commitment.circom --r1cs --wasm --sym -o $(OUT_DIR)

order-cancel:
	cd $(CIRCUIT_DIR) && circom src/order_cancel.circom --r1cs --wasm --sym -o $(OUT_DIR)

order-match:
	cd $(CIRCUIT_DIR) && circom src/order_match.circom --r1cs --wasm --sym -o $(OUT_DIR)

# --- Groth16 trusted setup ---

ENTROPY := $(shell date +%s | shasum -a 256 | head -c 40)

setup-order-commitment: order-commitment
	cd $(OUT_DIR) && \
	  $(SNARKJS) powersoftau new bn254 14 pot14_0000.ptau && \
	  $(SNARKJS) powersoftau contribute pot14_0000.ptau pot14_0001.ptau --name="contribution" -e=$(ENTROPY) && \
	  $(SNARKJS) powersoftau prepare phase2 pot14_0001.ptau pot14_final.ptau && \
	  $(SNARKJS) groth16 setup order_commitment.r1cs pot14_final.ptau order_commitment.zkey && \
	  $(SNARKJS) zkey export verificationkey order_commitment.zkey order_commitment_vk.json && \
	  echo "✓ order_commitment setup complete"

setup-order-cancel: order-cancel
	cd $(OUT_DIR) && \
	  $(SNARKJS) powersoftau new bn254 14 pot14_0000.ptau && \
	  $(SNARKJS) powersoftau contribute pot14_0000.ptau pot14_0001.ptau --name="contribution" -e=$(ENTROPY) && \
	  $(SNARKJS) powersoftau prepare phase2 pot14_0001.ptau pot14_final.ptau && \
	  $(SNARKJS) groth16 setup order_cancel.r1cs pot14_final.ptau order_cancel.zkey && \
	  $(SNARKJS) zkey export verificationkey order_cancel.zkey order_cancel_vk.json && \
	  echo "✓ order_cancel setup complete"

setup-order-match: order-match
	cd $(OUT_DIR) && \
	  $(SNARKJS) powersoftau new bn254 16 pot16_0000.ptau && \
	  $(SNARKJS) powersoftau contribute pot16_0000.ptau pot16_0001.ptau --name="contribution" -e=$(ENTROPY) && \
	  $(SNARKJS) powersoftau prepare phase2 pot16_0001.ptau pot16_final.ptau && \
	  $(SNARKJS) groth16 setup order_match.r1cs pot16_final.ptau order_match.zkey && \
	  $(SNARKJS) zkey export verificationkey order_match.zkey order_match_vk.json && \
	  echo "✓ order_match setup complete"

setup-all: setup-order-commitment setup-order-cancel setup-order-match

clean:
	rm -rf $(OUT_DIR)
