# Session Summary

## What We Did

### 1. Built the full stack
| Component | Status |
|-----------|--------|
| Noir circuit | ✅ Compiled, 4/4 tests pass |
| Soroban contract | ✅ Built to WASM |
| Client deps | ✅ Installed |
| Server deps | ✅ Installed |

### 2. Resolved compile issues
- **Poseidon2 API change**: Noir 1.0 uses `poseidon2_permutation`, older versions use `Poseidon2::hash`. Circuit now targets Noir 0.34.0.

### 3. Discovered proof system mismatch
- **Contract expects**: Groth16 proofs (alpha/beta/gamma/delta VK + BN254 pairing check)
- **Modern Noir + bb.js produces**: UltraHonk proofs (different system)
- **Resolution**: Deferred — VK constants are placeholder zeros for now

### 4. Deployed to Stellar testnet
- **Contract ID**: `CCKGEE2PRAT2Z3PYR4DCWQYO2YWVSYU3PDX3CH2VS3GJG5NNSQIRG5KS`
- **Open tx**: https://stellar.expert/explorer/testnet/tx/fb039d91c9f8ac613c47f441ebc72b427a9bc43e8710bb9fafa27a5302a27926
- **Close tx**: https://stellar.expert/explorer/testnet/tx/3d46eafd0f9223ce8a43d63587fee89c15529313fc7368a53b979ab55e0fa5bb
- `open()` stores commitment hash + collateral ✅
- `close()` returns collateral ✅ (but VK is zero bytes, so pairing check passes trivially)

### 5. Architecture doc written
`ARCHITECTURE.md` — full flow from frontend → TEE server → Stellar chain.

## What's Left

| Item | Notes |
|------|-------|
| **VK extraction** | Need Groth16-capable `bb` binary to extract VK and populate contract constants |
| **Prover server** | `tiny/server/` ready but needs to be started locally |
| **Frontend** | Not started yet — browser UI with wallet connect |
| **TEE deployment** | Docker image ready for GCP Confidential Space |

## Key Links
- Contract: https://stellar.expert/explorer/testnet/contract/CCKGEE2PRAT2Z3PYR4DCWQYO2YWVSYU3PDX3CH2VS3GJG5NNSQIRG5KS
- Stellar testnet: https://lab.stellar.org/r/testnet
