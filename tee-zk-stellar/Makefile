.PHONY: all circuit contract docker demo deploy clean

all: circuit contract docker

# Compile the Noir circuit
circuit:
	cd circuit && nargo compile

# Build the Soroban contract
contract:
	cd contract && cargo build --target wasm32-unknown-unknown --release

# Build and start the TEE prover Docker container
docker:
	docker compose build
	docker compose up -d

# Run the demo client (builds circuit + contract first)
demo: circuit docker
	cd client && npm install && npx tsx src/demo.ts

# Deploy contract to testnet
deploy:
	./scripts/deploy.sh

# Integration test
test: demo

clean:
	docker compose down -v
	rm -rf circuit/target
	rm -rf contract/target
	rm -rf tee-prover/node_modules
	rm -rf client/node_modules
