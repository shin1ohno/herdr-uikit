#!/usr/bin/env python3
"""Held-press UIApplicationMain fixture and its negative controls (leased Mac).

Builds examples/held_press_probe.rs for aarch64-apple-ios-sim with
test-support, runs it in the given Simulator, then applies each mutation
below, requires the named assertion to fail, and restores the exact bytes.
Output goes outside the checkout. No signing identity, physical device or
OS-routing claim: injected presses do not show what iPadOS delivers.

Environment: HERDR_RETIREMENT_LEASE=exclusive, SIMULATOR_UDID (a booted
Simulator this run owns), HERDR_PROBE_OUTPUT (new absolute directory outside
the checkout) and HERDR_HELD_PRESS_EXPECTED_HEAD. In a git checkout HEAD
must equal it and the tree must be clean; in an exported tree (no .git) the
manifest records it with the source tree hash for comparison.
"""
import hashlib
import json
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
HEAD = os.environ.get('HERDR_HELD_PRESS_EXPECTED_HEAD', '')
BUNDLE = 'be.ohno.herdruikit.heldpressprobe'
TARGET = Path(os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target')))
PASS, ASSERT = 'HELD_PRESS_PROBE_PASS:', 'HELD_PRESS_PROBE_ASSERT:'
SCRIPT = 'src/window/retirement_probe/held_press/script.rs'

# (label, file, unique source text, replacement, assertion that must fail)
MUTATIONS = [
    ('began-as-changed', SCRIPT, 'Self::Began => c"pressesBegan:withEvent:"',
     'Self::Began => c"pressesChanged:withEvent:"', 'began-keydown'),
    ('repeat-delayed', 'src/window/text_input/keys.rs', 'const REPEAT_DELAY: f64 = 0.5;',
     'const REPEAT_DELAY: f64 = 5.0;', 'began-repeats'),
    ('changed-as-ended', SCRIPT, 'Self::Changed => c"pressesChanged:withEvent:"',
     'Self::Changed => c"pressesEnded:withEvent:"', 'changed-keeps-ledger'),
    ('cancelled-as-changed', SCRIPT, 'Self::Cancelled => c"pressesCancelled:withEvent:"',
     'Self::Cancelled => c"pressesChanged:withEvent:"', 'cancelled-keyup'),
    ('ended-as-changed', SCRIPT, 'Self::Ended => c"pressesEnded:withEvent:"',
     'Self::Ended => c"pressesChanged:withEvent:"', 'ended-keyup'),
    ('retired-sent-to-current', 'src/window/retirement_probe/held_press.rs',
     '(old.clone(), HeldProbeDestination::Retired)',
     '(current, HeldProbeDestination::Retired)', 'retired-release-no-keyup'),
]


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


def tree_sha256():
    """Hash of every source file the build and this script read."""
    digest = hashlib.sha256()
    names = ['Cargo.toml', 'Cargo.lock', 'build.rs', 'rust-toolchain.toml']
    for top in ('src', 'examples', 'scripts', 'support', 'tests'):
        names += sorted(str(p.relative_to(ROOT)) for p in (ROOT / top).rglob('*') if p.is_file())
    for name in names:
        digest.update(name.encode() + b'\0' + (ROOT / name).read_bytes() + b'\0')
    return digest.hexdigest()


def probe(app, label, expected=None):
    free = shutil.disk_usage(ROOT).free
    if free < 6 * 1024**3:
        sys.exit('STOP: below 6 GiB; delete nothing')
    print(f'PROBE_BEGIN {label} free_bytes={free}', flush=True)
    checked('cargo', 'build', '--locked', '--features', 'test-support', '--example',
            'held_press_probe', '--target', 'aarch64-apple-ios-sim')
    shutil.copy2(TARGET / 'aarch64-apple-ios-sim/debug/examples/held_press_probe',
                 app / 'held_press_probe')
    checked('codesign', '--force', '--sign', '-', '--timestamp=none', str(app))
    checked('xcrun', 'simctl', 'install', UDID, str(app))
    stdout, stderr = OUT / (label + '.stdout'), OUT / (label + '.stderr')
    checked('xcrun', 'simctl', 'launch', '--terminate-running-process',
            '--stdout=' + str(stdout), '--stderr=' + str(stderr), UDID, BUNDLE)
    text = ''
    for _ in range(120):
        text = stderr.read_text(errors='replace') if stderr.exists() else ''
        if PASS in text or ASSERT in text:
            break
        time.sleep(0.5)
    else:
        print(text, flush=True)
        run('xcrun', 'simctl', 'terminate', UDID, BUNDLE)
        sys.exit(f'{label}: no terminal probe marker in 60 seconds')
    print(text, flush=True)
    if expected:
        if f'{ASSERT} {expected}' not in text or PASS in text:
            sys.exit(f'{label}: required assertion {expected} absent')
    elif PASS not in text or ASSERT in text:
        sys.exit(f'{label}: probe failed')
    print(f'PROBE_END {label}', flush=True)


def main():
    if platform.system() != 'Darwin' or os.environ.get('HERDR_RETIREMENT_LEASE') != 'exclusive':
        sys.exit('Requires macOS and an explicit exclusive lease')
    if not UDID or not HEAD or not OUT.is_absolute() or OUT == ROOT or ROOT in OUT.parents:
        sys.exit('Set SIMULATOR_UDID, HERDR_HELD_PRESS_EXPECTED_HEAD and an absolute '
                 'HERDR_PROBE_OUTPUT outside the checkout')
    if (ROOT / '.git').exists():
        if checked('git', 'rev-parse', 'HEAD').stdout.strip() != HEAD:
            sys.exit('HEAD is not HERDR_HELD_PRESS_EXPECTED_HEAD')
        if run('git', 'status', '--porcelain').stdout.strip():
            sys.exit('Requires a clean checkout')
    if OUT.exists():
        sys.exit('Output must be new; preserve prior evidence')
    OUT.mkdir(parents=True)
    app = OUT / 'HeldPressProbe.app'
    app.mkdir()
    (app / 'Info.plist').write_bytes(plistlib.dumps({
        'CFBundleDevelopmentRegion': 'en', 'CFBundleExecutable': 'held_press_probe',
        'CFBundleIdentifier': BUNDLE, 'CFBundleInfoDictionaryVersion': '6.0',
        'CFBundleName': 'SDK Held Press Probe', 'CFBundlePackageType': 'APPL',
        'CFBundleShortVersionString': '1.0', 'CFBundleVersion': '1',
        'CFBundleSupportedPlatforms': ['iPhoneSimulator'], 'LSRequiresIPhoneOS': True,
        'MinimumOSVersion': '17.0', 'UIDeviceFamily': [2], 'UILaunchScreen': {},
        'UIApplicationSceneManifest': {'UIApplicationSupportsMultipleScenes': True},
    }))
    manifest = {'head': HEAD, 'tree_sha256': tree_sha256(), 'simulator': UDID,
                'scope': 'injected presses through actual selectors; not iPadOS delivery',
                'mutations': []}
    for label, relative, before, after, assertion in MUTATIONS:
        original = (ROOT / relative).read_text()
        if original.count(before) != 1:
            sys.exit(f'{label}: unique source anchor missing')
        manifest['mutations'].append({
            'label': label, 'file': relative, 'assertion': assertion,
            'source_sha256': hashlib.sha256(original.encode()).hexdigest(),
            'mutant_sha256': hashlib.sha256(original.replace(before, after).encode()).hexdigest()})
    (OUT / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    probe(app, 'baseline')
    for label, relative, before, after, assertion in MUTATIONS:
        path = ROOT / relative
        original = path.read_bytes()
        try:
            path.write_text(original.decode().replace(before, after))
            probe(app, label, assertion)
        finally:
            path.write_bytes(original)
        if path.read_bytes() != original:
            sys.exit(f'{label}: exact restore failed')
    if tree_sha256() != manifest['tree_sha256']:
        sys.exit('source tree differs from the baseline after restores')
    probe(app, 'restored')
    print('HELD_PRESS_MUTATIONS_PASS; iPadOS delivery order UNVERIFIED', flush=True)


if __name__ == '__main__':
    main()
