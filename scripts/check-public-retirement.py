#!/usr/bin/env python3
"""Bounded UIApplicationMain probe; run this WHOLE script under leased guarded.py.

Output is kept outside the checkout. No physical-device signing, timers for
input recovery, shared-source edits, target removal, or OS-routing claims.
"""
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
OUT = Path(os.environ.get('HERDR_PROBE_OUTPUT', ''))
UDID = os.environ.get('SIMULATOR_UDID', '')
BUNDLE = 'be.ohno.cos328399c7.retirementprobe'
TARGET = Path(os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target')))


def run(*args):
    result = subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT)
    print('$', ' '.join(map(str, args)), '\n', result.stdout, flush=True)
    return result


def checked(*args):
    result = run(*args)
    if result.returncode:
        sys.exit(f'command failed ({result.returncode}); no probe claim')
    return result


if platform.system() != 'Darwin' or os.environ.get('HERDR_RETIREMENT_LEASE') != 'exclusive':
    sys.exit('Requires macOS and explicit exclusive lease')
if not UDID or not OUT.is_absolute() or OUT == ROOT or ROOT in OUT.parents:
    sys.exit('Set SIMULATOR_UDID and an absolute task-owned HERDR_PROBE_OUTPUT outside checkout')
if run('git', 'status', '--porcelain').stdout.strip():
    sys.exit('Requires clean isolated checkout')
if OUT.exists():
    sys.exit('Output must be new; preserve prior evidence')
OUT.mkdir(parents=True)
APP = OUT / 'RetirementProbe.app'
APP.mkdir()
plist = {
    'CFBundleDevelopmentRegion': 'en', 'CFBundleExecutable': 'retirement_probe',
    'CFBundleIdentifier': BUNDLE, 'CFBundleInfoDictionaryVersion': '6.0',
    'CFBundleName': 'SDK Retirement Probe', 'CFBundlePackageType': 'APPL',
    'CFBundleShortVersionString': '1.0', 'CFBundleVersion': '1',
    'CFBundleSupportedPlatforms': ['iPhoneSimulator'], 'LSRequiresIPhoneOS': True,
    'MinimumOSVersion': '17.0', 'UIDeviceFamily': [2], 'UILaunchScreen': {},
    'UIApplicationSceneManifest': {'UIApplicationSupportsMultipleScenes': True},
    'UISupportedInterfaceOrientations~ipad': ['UIInterfaceOrientationPortrait',
        'UIInterfaceOrientationLandscapeLeft', 'UIInterfaceOrientationLandscapeRight'],
}
(APP / 'Info.plist').write_bytes(plistlib.dumps(plist))


def probe(label, expected=None):
    free = shutil.disk_usage(ROOT).free
    if free < 6 * 1024**3:
        sys.exit('STOP: below 6 GiB; delete nothing')
    print(f'PROBE_BEGIN {label} free_bytes={free}', flush=True)
    checked('cargo', 'build', '--locked', '--features', 'test-support', '--example',
            'retirement_probe', '--target', 'aarch64-apple-ios-sim')
    shutil.copy2(TARGET / 'aarch64-apple-ios-sim/debug/examples/retirement_probe',
                 APP / 'retirement_probe')
    checked('codesign', '--force', '--sign', '-', '--timestamp=none', str(APP))
    checked('codesign', '--verify', '--strict', str(APP))
    checked('xcrun', 'simctl', 'install', UDID, str(APP))
    stdout, stderr = OUT / (label + '.stdout'), OUT / (label + '.stderr')
    checked('xcrun', 'simctl', 'launch', '--stdout=' + str(stdout),
            '--stderr=' + str(stderr), UDID, BUNDLE)
    text = ''
    for _ in range(120):
        text = stderr.read_text(errors='replace') if stderr.exists() else ''
        if 'RETIREMENT_PROBE_PASS:' in text or 'RETIREMENT_PROBE_ASSERT:' in text:
            break
        time.sleep(0.5)
    else:
        # The exact task-owned app only; preserve its logs before stopping it.
        print(text, flush=True)
        run('xcrun', 'simctl', 'terminate', UDID, BUNDLE)
        sys.exit(f'{label}: no terminal probe marker in 60 seconds')
    print(text, flush=True)
    if expected:
        if f'RETIREMENT_PROBE_ASSERT: {expected}' not in text or 'RETIREMENT_PROBE_PASS:' in text:
            sys.exit(f'{label}: required executed assertion absent')
    elif 'RETIREMENT_PROBE_PASS:' not in text or 'RETIREMENT_PROBE_ASSERT:' in text:
        sys.exit(f'{label}: probe failed')
    # The app exits itself after writing its marker; no process is left running.
    print(f'PROBE_END {label}', flush=True)


probe('baseline')
mutations = [
    ('launch-unprotected', 'src/platform/shared.rs',
     '                    let _in_gpui = crate::gpui_call::InGpui::enter();\n                    launch();',
     '                    launch();', 'launch-weak-update-after-app-borrow'),
    ('slot-drained-before-restore', 'src/window/state.rs',
     '    let _in_gpui = crate::gpui_call::InGpui::enter();\n    let result = f(&mut callback);\n    slot.borrow_mut().get_or_insert(callback);',
     '    let in_gpui = crate::gpui_call::InGpui::enter();\n    let result = f(&mut callback);\n    drop(in_gpui);\n    slot.borrow_mut().get_or_insert(callback);',
     'public-api-callback-slot-restored'),
]
for label, relative, before, after, assertion in mutations:
    path = ROOT / relative
    original = path.read_text()
    if original.count(before) != 1:
        sys.exit(f'{label}: unique source anchor missing')
    try:
        path.write_text(original.replace(before, after))
        probe(label, assertion)
    finally:
        path.write_text(original)
    probe(label + '-restored')
print('PUBLIC_RETIREMENT_MUTATIONS_PASS; OS retarget/loss UNVERIFIED', flush=True)
