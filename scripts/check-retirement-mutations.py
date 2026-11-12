#!/usr/bin/env python3
"""Run fix-removal negatives only in a leased, isolated Mac clone."""
import os
from pathlib import Path
import platform
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
TEST = 'native_retirement_selectors_isolate_old_callbacks_and_deliver_new_text_once'

def run(*args):
    return subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT)

def guard():
    available = os.statvfs(ROOT)
    if available.f_bavail * available.f_frsize < 6 * 1024**3:
        sys.exit('STOP: less than 6 GiB available; delete nothing')

if platform.system() != 'Darwin' or os.environ.get('HERDR_RETIREMENT_LEASE') != 'exclusive':
    sys.exit('Requires macOS and an explicit exclusive lease (HERDR_RETIREMENT_LEASE=exclusive)')
if run('git', 'status', '--porcelain').stdout.strip():
    sys.exit('Requires a clean isolated task checkout')
if not os.environ.get('CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUNNER'):
    sys.exit('Set CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUNNER to the absolute scripts/sim-runner.sh')

command = ['cargo', 'test', '--locked', '--all-features', '--target',
           'aarch64-apple-ios-sim', TEST, '--', '--nocapture']

def test(label, expect_failure=False):
    guard()
    result = run(*command)
    print(label, result.stdout, flush=True)
    if expect_failure:
        if result.returncode != 101 or TEST not in result.stdout or 'panicked at' not in result.stdout or 'test result: FAILED' not in result.stdout:
            sys.exit(f'{label}: not the required executed assertion failure')
    elif result.returncode != 0 or '1 passed' not in result.stdout:
        sys.exit(f'{label}: native adapter regression did not pass')

test('baseline')
path = ROOT / 'src/window/text_input/retirement.rs'
original = path.read_text()
mutations = [
    ('remove-old-fence', 'target.client.retire();', '// retirement removed by negative control'),
    ('reuse-old-native-origin', 'GPUITextInputView::new(old.mtm(), next)',
     'GPUITextInputView::new(old.mtm(), { let _ = next; old.ivars().target.clone() })'),
]
for label, before, after in mutations:
    if original.count(before) != 1:
        sys.exit(f'{label}: unique mutation anchor missing')
    try:
        path.write_text(original.replace(before, after))
        test(label, expect_failure=True)
    finally:
        path.write_text(original)
    test(label + '-restored')
