#!/usr/bin/env bash
# GCP Confidential Space Deployment
# -----------------------------------------------------------------------
# Prerequisites:
#   gcloud CLI installed and authenticated
#   docker CLI installed
#   GCP project with billing enabled
# -----------------------------------------------------------------------
set -euo pipefail

PROJECT="${PROJECT:-${1:-}}"
REGION="${REGION:-us-central1}"
SERVICE="${SERVICE:-tee-match}"

if [ -z "$PROJECT" ]; then
  echo "Usage: $0 <PROJECT_ID>" >&2
  echo "   or: PROJECT=<id> $0" >&2
  exit 1
fi

gcloud config set project "$PROJECT" 2>/dev/null || true

echo "=== 1. Enable APIs ======================================"
gcloud services enable \
  compute.googleapis.com \
  confidentialcomputing.googleapis.com \
  cloudkms.googleapis.com \
  artifactregistry.googleapis.com \
  --project="$PROJECT"

echo "=== 2. Create Artifact Registry repo ===================="
gcloud artifacts repositories create tee-match-repo \
  --repository-format=docker \
  --location="$REGION" \
  --project="$PROJECT" \
  2>/dev/null || echo "  (repo already exists)"
IMAGE="${REGION}-docker.pkg.dev/${PROJECT}/tee-match-repo/${SERVICE}:latest"

echo "=== 3. Build and push Docker image ======================"
docker build -f infra/Dockerfile -t "$IMAGE" .
docker push "$IMAGE"

echo "=== 4. Create KMS key ring + key ========================"
gcloud kms keyrings create cer-perp \
  --location=global \
  --project="$PROJECT" \
  2>/dev/null || echo "  (keyring already exists)"

gcloud kms keys create tee-dek \
  --location=global \
  --keyring=cer-perp \
  --purpose=encryption \
  --project="$PROJECT" \
  2>/dev/null || echo "  (key already exists)"

echo "=== 5. Generate and encrypt DEK ========================="
DEK_HEX=$(openssl rand -hex 32)
echo -n "$DEK_HEX" | base64 > /tmp/tek-dek.plain
gcloud kms encrypt \
  --location=global \
  --keyring=cer-perp \
  --key=tee-dek \
  --plaintext-file=/tmp/tek-dek.plain \
  --ciphertext-file=/tmp/tek-dek.enc \
  --project="$PROJECT"
rm -f /tmp/tek-dek.plain
DEK_ENC_B64=$(base64 < /tmp/tek-dek.enc | tr -d '\n')
rm -f /tmp/tek-dek.enc

echo "=== 6. Deploy Confidential Space workload ==============="
echo "  DEK_HEX: ${DEK_HEX:0:16}... (save this!)"
echo "  Image:   $IMAGE"
echo ""
echo "  Deploy via GCP Console:"
echo "    Confidential Space → Create Workload"
echo "    Image: $IMAGE"
echo "    KMS Key: projects/$PROJECT/locations/global/keyRings/cer-perp/cryptoKeys/tee-dek"
echo "    Port: 9721"
echo "    Env: CER_DEK=\${DEK_HEX}"
echo ""
echo "  Or use: gcloud beta confidential-computing workloads create ..."
echo "  See: tools/tee-match/src/crypto.rs for the encryption scheme."
echo ""
echo "=== Local test command ==================================="
echo "  docker run -p 9721:9721 -e CER_DEK=$DEK_HEX -v \$(pwd)/circuits/keys:/keys $IMAGE"
