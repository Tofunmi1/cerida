#!/bin/bash
set -e

# Write the oracle signing key to the stellar identity file that the keeper
# reads via `stellar keys address e2e`. Pass ORACLE_SECRET=S... as env var.
if [ -n "$ORACLE_SECRET" ]; then
  mkdir -p /root/.config/stellar/identity
  printf 'secret_key = "%s"\n' "$ORACLE_SECRET" \
    > /root/.config/stellar/identity/e2e.toml
  echo "[entrypoint] oracle identity written (e2e)"
else
  echo "[entrypoint] WARNING: ORACLE_SECRET not set — oracle keeper will fail"
fi

exec keepers "$@"
