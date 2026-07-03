#!/usr/bin/env bash
# GCP Confidential Space Deployment
# -----------------------------------------------------------------------
# Prerequisites:
#   gcloud CLI installed and authenticated
#   GCP project with billing enabled
# -----------------------------------------------------------------------
set -euo pipefail

PROJECT="${PROJECT:-${1:-}}"
REGION="${REGION:-us-central1}"
ZONE="${ZONE:-us-central1-a}"
SERVICE="${SERVICE:-tee-match}"

# Stellar signing key used by tee-match to submit on-chain transactions.
# Must be set in the environment before running this script.
# Example: export STELLAR_SOURCE_SECRET=SXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
if [ -z "${STELLAR_SOURCE_SECRET:-}" ]; then
  echo "WARNING: STELLAR_SOURCE_SECRET not set — tee-match will use named identity 'e2e'" >&2
  echo "  Set it with: export STELLAR_SOURCE_SECRET=<stellar-secret-key>" >&2
fi

if [ -z "$PROJECT" ]; then
  echo "Usage: $0 <PROJECT_ID>" >&2
  echo "   or: PROJECT=<id> $0" >&2
  exit 1
fi

gcloud config set project "$PROJECT" 2>/dev/null || true

IMAGE="${REGION}-docker.pkg.dev/${PROJECT}/tee-match-repo/${SERVICE}:latest"

echo "=== 1. Enable APIs ======================================"
gcloud services enable \
  compute.googleapis.com \
  confidentialcomputing.googleapis.com \
  cloudkms.googleapis.com \
  artifactregistry.googleapis.com \
  cloudbuild.googleapis.com \
  --project="$PROJECT"

echo "=== 2. Create Artifact Registry repo ===================="
gcloud artifacts repositories create tee-match-repo \
  --repository-format=docker \
  --location="$REGION" \
  --project="$PROJECT" \
  2>/dev/null || echo "  (repo already exists)"

echo "=== 3. Build and push via Cloud Build ==================="
gcloud builds submit \
  --config=cloudbuild-tee.yaml \
  --project="$PROJECT" \
  .

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

echo "=== 5. Create service account for TEE VM ================"
SA_NAME="tee-match-sa"
SA_EMAIL="${SA_NAME}@${PROJECT}.iam.gserviceaccount.com"

gcloud iam service-accounts create "$SA_NAME" \
  --display-name="tee-match TEE service account" \
  --project="$PROJECT" \
  2>/dev/null || echo "  (service account already exists)"

# Allow it to pull from Artifact Registry
gcloud projects add-iam-policy-binding "$PROJECT" \
  --member="serviceAccount:${SA_EMAIL}" \
  --role="roles/artifactregistry.reader" \
  --condition=None 2>/dev/null || true

# Allow the Confidential Space launcher to call the attestation API
gcloud projects add-iam-policy-binding "$PROJECT" \
  --member="serviceAccount:${SA_EMAIL}" \
  --role="roles/confidentialcomputing.workloadUser" \
  --condition=None 2>/dev/null || true

# Allow launcher logs to be written
gcloud projects add-iam-policy-binding "$PROJECT" \
  --member="serviceAccount:${SA_EMAIL}" \
  --role="roles/logging.logWriter" \
  --condition=None 2>/dev/null || true

# Allow it to decrypt with the KMS key
gcloud kms keys add-iam-policy-binding tee-dek \
  --location=global \
  --keyring=cer-perp \
  --member="serviceAccount:${SA_EMAIL}" \
  --role="roles/cloudkms.cryptoKeyDecrypter" \
  --project="$PROJECT" 2>/dev/null || true

echo "=== 6. Deploy Confidential Space VM ====================="
gcloud compute instances create tee-match-vm \
  --project="$PROJECT" \
  --zone="$ZONE" \
  --machine-type=n2d-standard-2 \
  --confidential-compute-type=SEV \
  --maintenance-policy=TERMINATE \
  --shielded-secure-boot \
  --image-project=confidential-space-images \
  --image-family=confidential-space \
  --service-account="${SA_EMAIL}" \
  --scopes=cloud-platform \
  --tags=tee-match \
  --metadata=\
"tee-image-reference=${IMAGE},\
tee-restart-policy=Always,\
tee-env-KEYS_DIR=/keys,\
tee-env-STELLAR_SOURCE_SECRET=${STELLAR_SOURCE_SECRET:-}" \
  2>/dev/null || echo "  (instance already exists — use 'gcloud compute instances update-container' to update)"

echo "=== 7. Deploy keepers VM ================================"
gcloud builds submit \
  --config=cloudbuild-keepers.yaml \
  --project="$PROJECT" \
  .

KEEPERS_IMAGE="${REGION}-docker.pkg.dev/${PROJECT}/tee-match-repo/keepers:latest"
TEE_INTERNAL_IP=$(gcloud compute instances describe tee-match-vm \
  --zone="$ZONE" --project="$PROJECT" \
  --format="value(networkInterfaces[0].networkIP)" 2>/dev/null || echo "UNKNOWN")

gcloud compute instances create keepers-vm \
  --project="$PROJECT" \
  --zone="$ZONE" \
  --machine-type=e2-standard-2 \
  --image-project=cos-cloud \
  --image-family=cos-stable \
  --tags=keepers \
  --metadata=\
"gce-container-declaration=spec:
  containers:
  - image: ${KEEPERS_IMAGE}
    args:
    - --tee-addr
    - ${TEE_INTERNAL_IP}:9720
    - --perp-id
    - CD6IY25X36TIDYU7TKX3Y6NMZY2TTCDKCYYHER5EAHATKAZNXN6J4JBA
    - --no-oracle
    env:
    - name: SOROBAN_RPC_URL
      value: https://soroban-testnet.stellar.org
    restartPolicy: Always" \
  2>/dev/null || echo "  (keepers instance already exists)"

echo ""
echo "=== Done ================================================"
echo "  tee-match VM internal IP: ${TEE_INTERNAL_IP}"
echo "  Image: ${IMAGE}"
echo ""
echo "  Firewall rule (allow keepers → tee-match on port 9720):"
echo "    gcloud compute firewall-rules create allow-keepers-to-tee \\"
echo "      --network=default --allow=tcp:9720 \\"
echo "      --source-tags=keepers --target-tags=tee-match \\"
echo "      --project=$PROJECT"
