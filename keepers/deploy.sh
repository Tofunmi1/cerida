#!/usr/bin/env bash
# Deploy Keepers to GCP (separate instance from TEE server)
set -euo pipefail
PROJECT="${PROJECT:-${1:-}}"
REGION="${REGION:-us-central1}"
IMAGE="${REGION}-docker.pkg.dev/${PROJECT}/tee-match-repo/keepers:latest"

if [ -z "$PROJECT" ]; then
  echo "Usage: $0 <PROJECT_ID>" >&2
  exit 1
fi

gcloud config set project "$PROJECT" 2>/dev/null || true
gcloud services enable compute.googleapis.com artifactregistry.googleapis.com --project="$PROJECT"

docker build -f keepers/Dockerfile -t "$IMAGE" .
docker push "$IMAGE"

echo "Image pushed: $IMAGE"
echo ""
echo "Run on GCP:"
echo ""
echo "  gcloud compute instances create-with-container keepers \\"
echo "    --container-image=$IMAGE \\"
echo "    --container-arg='--perp-id=<CONTRACT_ID>' \\"
echo "    --container-arg='--tee-addr=<TEE_SERVER_IP>:9720' \\"
echo "    --container-arg='--oracle-interval-secs=300' \\"
echo "    --container-env=SOROBAN_RPC_URL=<RPC_URL>"
echo ""
echo "Or run locally:"
echo "  docker run $IMAGE --perp-id <PERP_ID> --tee-addr <TEE_IP>:9720"
