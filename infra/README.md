# TEE Match Server — GCP Confidential Space Deployment

## Quick Start (Local Docker)

```bash
# 1. Build
docker build -f infra/Dockerfile -t tee-match .

# 2. Generate a DEK (32 random bytes, hex-encoded)
export CER_DEK=$(openssl rand -hex 32)

# 3. Run with proving keys mounted
docker run -p 9721:9721 \
  -e CER_DEK \
  -v $(pwd)/circuits/keys:/keys \
  tee-match

# 4. Test
./infra/test-tee-local.sh
```

## GCP Deployment ($300 credit)

### Prerequisites
```bash
gcloud auth login
gcloud projects create cer-perp-trading --name="CER-PERP TEE"
gcloud config set project cer-perp-trading
gcloud billing projects link cer-perp-trading --billing-account=YOUR_BILLING_ID
```

### Deploy
```bash
PROJECT=cer-perp-trading ./infra/deploy.sh
```

### Manual Steps (if script fails)

#### 1. Enable APIs
```bash
gcloud services enable \
  compute.googleapis.com \
  confidentialcomputing.googleapis.com \
  cloudkms.googleapis.com \
  artifactregistry.googleapis.com
```

#### 2. Build and Push Image
```bash
REGION=us-central1
IMAGE="${REGION}-docker.pkg.dev/${PROJECT}/tee-match-repo/tee-match:latest"

gcloud artifacts repositories create tee-match-repo \
  --repository-format=docker --location="$REGION"

docker build -f infra/Dockerfile -t "$IMAGE" .
docker push "$IMAGE"
```

#### 3. Create KMS Key
```bash
gcloud kms keyrings create cer-perp --location=global
gcloud kms keys create tee-dek \
  --location=global --keyring=cer-perp --purpose=encryption

# Generate and encrypt DEK
DEK_HEX=$(openssl rand -hex 32)
echo -n "$DEK_HEX" | base64 > /tmp/dek.plain
gcloud kms encrypt \
  --location=global --keyring=cer-perp --key=tee-dek \
  --plaintext-file=/tmp/dek.plain --ciphertext-file=/tmp/dek.enc
# Save DEK_HEX — you'll need it to encrypt order submissions
```

#### 4. Deploy Confidential Space Workload
```bash
# Via gcloud (beta)
gcloud beta confidential-computing workloads create tee-match \
  --location="$REGION" \
  --container-image="$IMAGE" \
  --kms-key="projects/${PROJECT}/locations/global/keyRings/cer-perp/cryptoKeys/tee-dek"
```

Or use the GCP Console: **Confidential Space → Create Workload**

#### 5. Verify
```bash
# Get workload IP
WORKLOAD_IP=$(gcloud compute instances describe tee-match \
  --zone="$REGION-a" --format='get(networkInterfaces[0].accessConfigs[0].natIP)')

# Request attestation
curl "http://${WORKLOAD_IP}:9721/attestation"
```

## Architecture

```
Client                    Reverse Proxy          TEE Server (GCP Conf. Space)
  │                            │                         │
  │  1. GET /attestation       │                         │
  │─────────────────────────►  │─────────────────────►   │
  │                            │                         │
  │  2. verify SEV-SNP         │   { token, hwmodel }    │
  │     derive session key     │◄───────────────────────  │
  │                            │                         │
  │  3. POST /place            │                         │
  │     AES-GCM(session,      │                         │
  │       {cmd:"place",...})  │                         │
  │─────────────────────────►  │─────────────────────►   │
  │                            │   decrypt with DEK      │
  │                            │   CLOB match            │
  │                            │   generate ZK proof     │
  │   { fills: [...], ok }     │   submit on-chain       │
  │◄─────────────────────────  │◄──────────────────────  │
```

## Security Model

- **Proving keys**: Inside SEV-SNP encrypted memory. No host access.
- **DEK**: Wrapped by KMS, unwrapped by GCP launcher at VM boot. Set as `CER_DEK` env var.
- **Order secrets**: AES-256-GCM encrypted in-flight. Decrypted only in enclave.
- **On-chain verification**: ZK proofs independently verifiable — no trust in TEE required.

## Local TEE Simulation

For development without GCP hardware:

```bash
# Terminal 1: start server with mock DEK
CER_DEK=$(openssl rand -hex 32) \
  cargo run --manifest-path tools/tee-match/Cargo.toml --features secure \
  -- ServeSecure --addr 127.0.0.1:9721 --db /tmp/tee-local-db

# Terminal 2: run the test harness
./infra/test-tee-local.sh
```

What works locally:
- All secure endpoints (place, cancel, match, market, init)
- Encrypted order flow (AES-GCM with DEK)
- CLOB matching + ZK proof generation
- On-chain submission (if testnet RPC accessible)

What only works on GCP:
- Real SEV-SNP attestation (locally returns stub)
- KMS unwrap (locally uses env var)
