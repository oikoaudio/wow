#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
bundle_dir="${repo_root}/target/bundled"
component_name="Oiko Wow.component"
component_destination="${bundle_dir}/${component_name}"

cd "${repo_root}"

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo run --locked -p xtask --release -- \
    bundle-universal wow-plugin --release

# AUv2 distribution is disabled while the editor crashes Logic's AU host.
# Remove an output left by an earlier AU-enabled build so release archives
# cannot accidentally include it. The packaging project remains available for
# explicit upstream compatibility testing.
rm -rf "${component_destination}"

for bundle in "Oiko Wow.clap" "Oiko Wow.vst3"; do
    binary="${bundle_dir}/${bundle}/Contents/MacOS/Oiko Wow"
    architectures="$(lipo -archs "${binary}")"
    if [[ "${architectures}" != *arm64* || "${architectures}" != *x86_64* ]]; then
        echo "${binary} is not universal: ${architectures}" >&2
        exit 1
    fi
done

echo "Created universal CLAP and VST3 bundles in ${bundle_dir}"
