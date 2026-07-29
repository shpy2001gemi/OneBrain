#!/usr/bin/env bash
set -euo pipefail

mobile_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
rust_workspace="$(cd "${mobile_root}/.." && pwd)"
crate_name="onebrain-mobile-bridge"
library_name="libonebrain_mobile_bridge.a"
output_root="${mobile_root}/ios/RustBridge"

rustup target add \
  aarch64-apple-ios \
  aarch64-apple-ios-sim \
  x86_64-apple-ios

cargo build \
  --manifest-path "${rust_workspace}/Cargo.toml" \
  -p "${crate_name}" \
  --target aarch64-apple-ios
cargo build \
  --manifest-path "${rust_workspace}/Cargo.toml" \
  -p "${crate_name}" \
  --target aarch64-apple-ios-sim
cargo build \
  --manifest-path "${rust_workspace}/Cargo.toml" \
  -p "${crate_name}" \
  --target x86_64-apple-ios

mkdir -p "${output_root}/device" "${output_root}/simulator"
cp -f \
  "${rust_workspace}/target/aarch64-apple-ios/debug/${library_name}" \
  "${output_root}/device/${library_name}"
lipo -create \
  "${rust_workspace}/target/aarch64-apple-ios-sim/debug/${library_name}" \
  "${rust_workspace}/target/x86_64-apple-ios/debug/${library_name}" \
  -output "${output_root}/simulator/${library_name}"

echo "Rust iOS bridge artifacts:"
lipo -info "${output_root}/device/${library_name}"
lipo -info "${output_root}/simulator/${library_name}"
