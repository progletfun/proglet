#!/usr/bin/env bash
# Reproducible build for solana-verify. Deploy ONLY the .so this produces:
# a plain `anchor build` output will not match the verified hash.
#
# The base image is pinned by digest. solana-verify's auto-picked image (from
# the solana crates in Cargo.lock) ships a Cargo too old for the edition-2024
# crates the test dependencies pull in. Verifiers must pass the same
# --base-image to `solana-verify verify-from-repo`.
set -euo pipefail
cd "$(dirname "$0")/.."

BASE_IMAGE="solanafoundation/solana-verifiable-build@sha256:12fd4c0a0790f0fc41ef74b0cdb6bccc167ba6137eb1adb749bac649481c86bd" # tag 4.3.0

solana-verify build --library-name proglet --base-image "$BASE_IMAGE"
solana-verify get-executable-hash target/deploy/proglet.so
