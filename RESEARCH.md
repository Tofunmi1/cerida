# Research: ZK + TEE Architecture for Private RWA Perpetual DEX

## Key Resources Found

### 1. UltraHonk Soroban Verifier
- **Repo**: `github.com/NethermindEth/rs-soroban-ultrahonk`
- **Fork**: `github.com/yugocabrio/rs-soroban-ultrahonk`
- **Stack**: Noir 1.0.0-beta.9 + Barretenberg 0.87.0 + bb.js
- **Flow**: Circuit → `bb.js` (UltraHonk proof) → Soroban `verify_proof()`
- **VK policy**: Set at deploy time, immutable
- **Works with**: Protocol 26 BN254 host functions (g1_msm, pairing_check)
- **Tutorial**: `jamesbachini.com/noir-on-stellar/`
- **E2E test**: `just e2e` spins local Stellar network + deploys + verifies

### 2. RISC Zero Stellar Verifier
- **Repo**: `github.com/NethermindEth/stellar-risc0-verifier`
- **Stack**: RISC Zero zkVM → Groth16 (BN254) → Soroban
- **Architecture**: VerifierRouter → EmergencyStop → Groth16Verifier
- **Governance**: TimelockController for verifier upgrades
- **Tutorial**: `jamesbachini.com/stellar-risc-zero-games/`
- **Note**: RISC Zero outputs Groth16 proofs, not UltraHonk. Verified via BN254 pairings.

### 3. TEE Options

| Option | Hardware | Attestation | Local Dev | Deployment |
|--------|----------|-------------|-----------|------------|
| **GCP Confidential Space** | AMD SEV-SNP | OIDC token | Docker only | gcloud CLI |
| **Marlin Oyster** | AMD SEV-SNP | On-chain | Local SDK | One command |
| **AWS Nitro Enclaves** | Intel/AMD | KMS signed | Docker only | Longer setup |
| **Local (no TEE)** | None | None | Just run it | Not for prod |

## Architecture Decision

### The DEX needs two ZK layers:

```
┌─────────────────────────────────────────────────────────────┐
│                    User (Browser)                            │
│  Signs Stellar txs, encrypts inputs to TEE                   │
└──────────────────────┬──────────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────────┐
│              TEE Prover Server (GCP SEV-SNP)                 │
│                                                              │
│  ┌─────────────────┐  ┌──────────────────┐                  │
│  │ Noir Circuit     │  │ RISC Zero Guest  │                  │
│  │ (position proof) │  │ (DEX execution)  │                  │
│  │ UltraHonk proof  │  │ Groth16 proof    │                  │
│  └────────┬────────┘  └────────┬─────────┘                  │
└───────────┼────────────────────┼────────────────────────────┘
            │                    │
            ▼                    ▼
┌──────────────────────┐  ┌──────────────────────┐
│ UltraHonk Verifier   │  │ RISC Zero Groth16    │
│ (Soroban contract)   │  │ Verifier (Soroban)   │
│ BN254 + Poseidon2    │  │ BN254 pairings       │
│ Protocol 26          │  │ Protocol 26          │
└──────────────────────┘  └──────────────────────┘
```

### Why both?
- **Noir/UltraHonk**: Simple & fast for the privacy layer (commitments, nullifiers, range proofs). What we already have working.
- **RISC Zero**: Complex DEX logic (order matching, liquidation checks, P&L computation). Write in Rust, no circuit DSL.

### Where TEE fits
The TEE wraps both provers. Without it, the server operator sees private inputs. With it:
1. User encrypts trade details with TEE's public key (NaCl Box)
2. TEE decrypts inside SEV-SNP encrypted memory
3. TEE runs both provers (bb.js + RISC Zero)
4. TEE returns proofs — operator never sees plaintext inputs

## Recommended Path for the Hackathon

Given 5 days to deadline:

### Day 1-2: Swap to UltraHonk verifier
- Replace our Groth16 contract with the Nethermind UltraHonk verifier
- Get `just e2e` running locally with our circuit
- Deploy to testnet with real proofs

### Day 3-4: TEE prover server
- Run the prover server locally (Docker)
- Encrypt inputs with NaCl Box (already written)
- Generate proofs inside the container
- Wire up basic frontend to call server + Stellar

### Day 5: Polish + demo video
- Record 2-3 min demo showing: open → TEE proves → close verifies on-chain
- Push to GitHub

## Projects to Fork/Reference

| Project | Use |
|---------|-----|
| `NethermindEth/rs-soroban-ultrahonk` | UltraHonk verifier contract |
| `jamesbachini/Noirlang-Experiments` | Frontend proof generator (React + bb.js WASM) |
| `NethermindEth/stellar-risc0-verifier` | RISC Zero verifier (for later) |
| `jamesbachini/typezero` | Full-stack RISC Zero + Stellar example |
| `NethermindEth/stellar-private-payments` | Privacy Pools PoC (Circom/Groth16) |
