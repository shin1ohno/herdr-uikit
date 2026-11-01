# Simulator resolution for scripts/sim-runner.sh; source it, do not run it.
# Adapted from herdr-gpui's scripts/ios/lib.sh, without the app bundle parts.
#
# HERDR_IOS_SIMULATOR names the simulator by UDID or by device name. A name
# resolves to an available device of that name, preferring one already booted,
# then the newest iOS runtime, so the default works on any Mac whose Xcode has
# that iPad without committing one machine's UDID.

HERDR_IOS_SIMULATOR=${HERDR_IOS_SIMULATOR:-"iPad Pro 13-inch (M5)"}

# Prints "UDID STATE" for the simulator HERDR_IOS_SIMULATOR-style argument $1
# names, or fails when no available simulator matches.
ios_sim_resolve() {
  xcrun simctl list --json devices available | python3 -I -c '
import json, re, sys

wanted = sys.argv[1]
candidates = []
for runtime, devices in json.load(sys.stdin)["devices"].items():
    match = re.search(r"\.iOS-(\d+)-(\d+)(?:-(\d+))?$", runtime)
    if match is None:
        continue
    version = tuple(int(part or 0) for part in match.groups())
    for device in devices:
        if wanted in (device["udid"], device["name"]):
            candidates.append((device["state"] == "Booted", version, device["udid"], device["state"]))
if not candidates:
    sys.exit(1)
_, _, udid, state = max(candidates)
print(udid, state)
' "$1"
}

# Boots the simulator $1 names unless it is already booted, waits until it has
# finished booting, and prints its UDID. Never shuts down or erases one.
ios_sim_boot() {
  local wanted=$1 resolved udid state log
  if ! resolved=$(ios_sim_resolve "$wanted"); then
    echo "error: no available simulator is named or has the UDID '$wanted';" \
      "set HERDR_IOS_SIMULATOR to one from 'xcrun simctl list devices available'" >&2
    return 1
  fi
  read -r udid state <<<"$resolved"
  if [[ $state != Booted ]]; then
    echo "Booting simulator $udid ($wanted), currently $state; a first boot takes about a minute" >&2
    # -b boots a shut-down device; for one already booting it only waits. It
    # reports progress every second, so its output is kept for failures only.
    if ! log=$(xcrun simctl bootstatus "$udid" -b 2>&1); then
      printf '%s\n' "$log" >&2
      echo "error: simulator $udid did not finish booting" >&2
      return 1
    fi
    echo "Simulator $udid booted" >&2
  fi
  printf '%s\n' "$udid"
}
