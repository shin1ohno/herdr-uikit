#!/usr/bin/env bash
# Cargo runner for aarch64-apple-ios-sim: runs a test binary inside the iPad
# simulator with `xcrun simctl spawn`, which exits with the binary's status,
# so `cargo test --target aarch64-apple-ios-sim` works:
#
#   CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUNNER=scripts/sim-runner.sh \
#     cargo test --target aarch64-apple-ios-sim
#
# Boots the simulator HERDR_IOS_SIMULATOR names (scripts/lib.sh) when it is
# not running, and never shuts it down. The binary runs as a plain process,
# outside any app bundle, so a test that needs UIApplicationMain cannot run here.
set -euo pipefail

if (($# == 0)); then
  echo "usage: sim-runner.sh BINARY [ARGS...] (set as Cargo's target runner)" >&2
  exit 2
fi

# shellcheck source=scripts/lib.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib.sh"

binary=$1
shift
# simctl searches the device's PATH for a bare name; give it the host path.
if [[ $binary != /* ]]; then binary=$PWD/$binary; fi

udid=$(ios_sim_boot "$HERDR_IOS_SIMULATOR")

# simctl hands the process only SIMCTL_CHILD_-prefixed variables: forward what
# the test harness and Cargo-built tests read.
while IFS= read -r variable; do
  case "$variable" in
    RUST_* | CARGO_* | HERDR_TEST_*) export "SIMCTL_CHILD_$variable=${!variable}" ;;
  esac
done < <(compgen -e)

exec xcrun simctl spawn "$udid" "$binary" "$@"
