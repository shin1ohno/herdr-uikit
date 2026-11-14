#!/usr/bin/env python3
"""Run SDK tests in one disposable hosted Simulator and remove it afterwards."""
import json
import os
import subprocess


def main():
    devices = json.loads(subprocess.check_output(['xcrun', 'simctl', 'list', '--json', 'devices', 'available'], text=True))['devices']
    candidates = []
    for runtime, entries in devices.items():
        if '.iOS-' not in runtime:
            continue
        suffix = runtime.split('.iOS-', 1)[1].split('-')
        if not all(part.isascii() and part.isdecimal() for part in suffix):
            continue
        for device in entries:
            if device['isAvailable'] and device['name'].startswith('iPad Pro 13-inch'):
                candidates.append((tuple(map(int, suffix)), runtime, device['deviceTypeIdentifier']))
    if not candidates:
        raise RuntimeError('No available iPad Pro 13-inch Simulator')
    _, runtime, kind = max(candidates)
    device = subprocess.check_output(['xcrun', 'simctl', 'create', 'herdr-uikit SDK contracts', kind, runtime], text=True).strip()
    try:
        subprocess.run(['cargo', 'test', '--locked', '--all-features', '--target', 'aarch64-apple-ios-sim'], check=True,
                       env={**os.environ, 'HERDR_IOS_SIMULATOR': device,
                            'CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUNNER': str(os.path.abspath('scripts/sim-runner.sh'))})
    finally:
        subprocess.run(['xcrun', 'simctl', 'shutdown', device], check=False)
        subprocess.run(['xcrun', 'simctl', 'delete', device], check=True)
        print('SDK SIMULATOR deleted', device, flush=True)


if __name__ == '__main__':
    main()
