#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
bundle_dir="${repo_root}/target/bundled"
auv2_build_dir="${repo_root}/target/auv2-build"
clap_bundle="${bundle_dir}/Oiko Wow.clap"
component_name="Oiko Wow.component"
component_destination="${bundle_dir}/${component_name}"

cd "${repo_root}"

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo run --locked -p xtask --release -- \
    bundle-universal wow-plugin --release

cmake \
    -S packaging/auv2 \
    -B "${auv2_build_dir}" \
    -DCMAKE_BUILD_TYPE=Release \
    "-DCMAKE_OSX_ARCHITECTURES=arm64;x86_64" \
    "-DWOW_CLAP_BUNDLE=${clap_bundle}"
cmake --build "${auv2_build_dir}" --config Release --target oiko_wow_auv2

component_source="$(find "${auv2_build_dir}" -type d -name "${component_name}" -print -quit)"
if [[ -z "${component_source}" ]]; then
    echo "Could not find the built ${component_name}" >&2
    exit 1
fi

rm -rf "${component_destination}"
ditto "${component_source}" "${component_destination}"
codesign --force --deep --sign - "${component_destination}"

for bundle in "Oiko Wow.clap" "Oiko Wow.vst3"; do
    binary="${bundle_dir}/${bundle}/Contents/MacOS/Oiko Wow"
    architectures="$(lipo -archs "${binary}")"
    if [[ "${architectures}" != *arm64* || "${architectures}" != *x86_64* ]]; then
        echo "${binary} is not universal: ${architectures}" >&2
        exit 1
    fi
done

component_binary="${component_destination}/Contents/MacOS/Oiko Wow"
embedded_clap_binary="${component_destination}/Contents/PlugIns/Oiko Wow.clap/Contents/MacOS/Oiko Wow"

for binary in "${component_binary}" "${embedded_clap_binary}"; do
    architectures="$(lipo -archs "${binary}")"
    if [[ "${architectures}" != *arm64* || "${architectures}" != *x86_64* ]]; then
        echo "${binary} is not universal: ${architectures}" >&2
        exit 1
    fi
done

plutil -lint "${component_destination}/Contents/Info.plist"
echo "Created universal CLAP, VST3, and AUv2 bundles in ${bundle_dir}"
