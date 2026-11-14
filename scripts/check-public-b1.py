#!/usr/bin/env python3
"""Bounded UIApplicationMain probe; run this WHOLE script under leased guarded.py.

Output is kept outside the checkout. No physical-device signing, timers for
input recovery, shared-source edits, target removal, or OS-routing claims.
"""
import os
import hashlib
import json
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
BUNDLE = 'be.ohno.cos328399c7.b1probe'
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
head = checked('git', 'rev-parse', 'HEAD').stdout.strip()
if head != os.environ.get('HERDR_B1_EXPECTED_HEAD'):
    sys.exit('Set HERDR_B1_EXPECTED_HEAD to the separately frozen exact source head')
if run('git', 'status', '--porcelain').stdout.strip():
    sys.exit('Requires clean isolated checkout')
if OUT.exists():
    sys.exit('Output must be new; preserve prior evidence')
OUT.mkdir(parents=True)
APP = OUT / 'B1Probe.app'
APP.mkdir()
plist = {
    'CFBundleDevelopmentRegion': 'en', 'CFBundleExecutable': 'b1_probe',
    'CFBundleIdentifier': BUNDLE, 'CFBundleInfoDictionaryVersion': '6.0',
    'CFBundleName': 'SDK B1 Source Probe', 'CFBundlePackageType': 'APPL',
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
            'b1_probe', '--target', 'aarch64-apple-ios-sim')
    shutil.copy2(TARGET / 'aarch64-apple-ios-sim/debug/examples/b1_probe',
                 APP / 'b1_probe')
    checked('codesign', '--force', '--sign', '-', '--timestamp=none', str(APP))
    checked('codesign', '--verify', '--strict', str(APP))
    checked('xcrun', 'simctl', 'install', UDID, str(APP))
    stdout, stderr = OUT / (label + '.stdout'), OUT / (label + '.stderr')
    checked('xcrun', 'simctl', 'launch', '--stdout=' + str(stdout),
            '--stderr=' + str(stderr), UDID, BUNDLE)
    text = ''
    for _ in range(120):
        text = stderr.read_text(errors='replace') if stderr.exists() else ''
        if 'B1_PROBE_PASS:' in text or 'B1_PROBE_ASSERT:' in text:
            break
        time.sleep(0.5)
    else:
        # The exact task-owned app only; preserve its logs before stopping it.
        print(text, flush=True)
        run('xcrun', 'simctl', 'terminate', UDID, BUNDLE)
        sys.exit(f'{label}: no terminal probe marker in 60 seconds')
    print(text, flush=True)
    if expected:
        if f'B1_PROBE_ASSERT: {expected}' not in text or 'B1_PROBE_PASS:' in text:
            sys.exit(f'{label}: required executed assertion absent')
    elif 'B1_PROBE_PASS:' not in text or 'B1_PROBE_ASSERT:' in text:
        sys.exit(f'{label}: probe failed')
    # The app exits itself after writing its marker; no process is left running.
    print(f'PROBE_END {label}', flush=True)


mutations = [
    ('early-down-completion', 'src/window/input_target.rs',
     '            || self.dispatch_inner(input),',
     '            || { let result = self.dispatch_inner(input); crate::window::complete_native_retirement(); result },',
     'edit-whole-tail-before-prepare'),
    ('new-fence-removed', 'src/window/native_retirement_request/transaction.rs',
     '    record.old.fence_reserved();', '', 'old-native-origin-no-leak'),
    ('retry-fence-forgotten', 'src/window/native_retirement_request/coordinator.rs',
     'old_fenced_at_submission: self.old_fenced_at_submission,',
     'old_fenced_at_submission: false,', 'new-terminal-precedes-legacy-drain'),
    ('legacy-hold-removed', 'src/gpui_call.rs',
     'is_running() || LEGACY_HOLD.get() || DRAINING.replace(true)',
     'is_running() || DRAINING.replace(true)', 'preparing-slot-released-before-legacy'),
    ('raw-shared-removed', 'src/platform/shared.rs',
     '        let _raw_later_entry = crate::native_operation::raw("shared-later");',
     'crate::native_operation::raw_observe("shared-later");', 'actual-raw-ingress-refuses'),
    ('raw-destroy-removed', 'src/window/scene.rs',
     '        let _raw_destroy_entry = crate::native_operation::raw("scene-destroy");',
     'crate::native_operation::raw_observe("scene-destroy");', 'actual-raw-ingress-refuses'),
    ('raw-refusal-removed', 'src/window/scene.rs',
     '            let _raw_refusal_entry = crate::native_operation::raw("scene-refusal");',
     'crate::native_operation::raw_observe("scene-refusal");', 'actual-raw-ingress-refuses'),
    ('replacement-acquisition-disabled', 'src/window/text_input/retirement.rs',
     '        view.flush();\n        old.removeFromSuperview();',
     '        view.ivars().acquire.set(Acquire::Settled);\n        view.flush();\n        old.removeFromSuperview();',
     'replacement-responder-acquired-before-input'),
]
manifest = {'head': head, 'scope': 'SDK native adapter subset, not OS/B2/S/Z acceptance', 'mutations': []}
for label, relative, before, after, assertion in mutations:
    original = (ROOT / relative).read_text()
    if original.count(before) != 1:
        sys.exit(f'{label}: unique source anchor missing')
    manifest['mutations'].append({'label': label, 'file': relative, 'assertion': assertion,
        'source_sha256': hashlib.sha256(original.encode()).hexdigest(),
        'mutant_sha256': hashlib.sha256(original.replace(before, after).encode()).hexdigest()})
(OUT / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
probe('baseline')
for label, relative, before, after, assertion in mutations:
    path = ROOT / relative
    original = path.read_bytes()
    try:
        path.write_text(original.decode().replace(before, after))
        probe(label, assertion)
    finally:
        path.write_bytes(original)
    if path.read_bytes() != original:
        sys.exit(f'{label}: exact restore failed')
    probe(label + '-restored')
print('B1_NATIVE_SUBSET_MUTATIONS_PASS; original S/Z/B2 and physical/OS routing UNVERIFIED', flush=True)
