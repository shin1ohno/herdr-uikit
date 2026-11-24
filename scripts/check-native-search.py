#!/usr/bin/env python3
"""Native search UIApplicationMain fixtures on a disposable Simulator (leased Mac).

Builds one example for aarch64-apple-ios-sim with test-support, bundles and
installs it, launches it with its stderr captured, then runs one XCUITest
method from scripts/native-search-qa that types into the running fixture.
The fixture prints `<PREFIX>_PASS` or `<PREFIX>_ASSERT: <name>`; the verdict is
read from that stderr only.

  check-native-search.py --example native_search_premise --test Premise/testPremise \
      --prefix PREMISE --output /abs/new/dir [--expect-assert NAME] [--label L]

`--mutate '["FILE", "BEFORE", "AFTER"]'` (JSON) rewrites one unique source text for this run
and restores the exact bytes afterwards (negative controls). The Simulator is
created here and deleted in a finally block. Output must be a new absolute
directory outside the checkout. No signing identity or physical device.
"""
import argparse
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
TARGET = Path(os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target')))
QA = ROOT / 'scripts' / 'native-search-qa'


def run(log, *args, check=True, env=None, timeout=None):
    print('$', ' '.join(map(str, args)), flush=True)
    result = subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, env=env, timeout=timeout)
    log.write(f'$ {" ".join(map(str, args))}\n{result.stdout}\n')
    log.flush()
    if check and result.returncode:
        print(result.stdout[-4000:], flush=True)
        sys.exit(f'command failed ({result.returncode}); no fixture claim')
    return result


def simulator(log):
    devices = json.loads(subprocess.check_output(
        ['xcrun', 'simctl', 'list', '--json', 'devices', 'available'], text=True))['devices']
    candidates = []
    for runtime, entries in devices.items():
        if '.iOS-' not in runtime:
            continue
        suffix = runtime.split('.iOS-', 1)[1].split('-')
        if not all(part.isdecimal() for part in suffix):
            continue
        for device in entries:
            if device['name'].startswith('iPad Pro 13-inch'):
                candidates.append((tuple(map(int, suffix)), runtime, device['deviceTypeIdentifier']))
    if not candidates:
        sys.exit('No available iPad Pro 13-inch Simulator')
    _, runtime, kind = max(candidates)
    udid = run(log, 'xcrun', 'simctl', 'create', 'herdr-uikit native search', kind, runtime).stdout.strip()
    log.write(f'SIMULATOR_CREATED {udid} {runtime}\n')
    return udid


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--example', required=True)
    parser.add_argument('--test', required=True)
    parser.add_argument('--prefix', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--label', default='baseline')
    parser.add_argument('--expect-assert')
    parser.add_argument('--mutate', action='append', default=[])
    parser.add_argument('--keyboards', default='en_US@sw=QWERTY;hw=Automatic,ja_JP-Romaji@sw=QWERTY-Japanese;hw=Automatic')
    args = parser.parse_args()
    if platform.system() != 'Darwin':
        sys.exit('Requires macOS')
    out = Path(args.output)
    if not out.is_absolute() or out == ROOT or ROOT in out.parents or out.exists():
        sys.exit('--output must be a new absolute directory outside the checkout')
    if shutil.disk_usage(ROOT).free < 6 * 1024**3:
        sys.exit('STOP: below 6 GiB free; delete nothing')
    out.mkdir(parents=True)
    log = (out / 'run.log').open('w')
    bundle = f'be.ohno.herdruikit.{args.example.replace("_", "")}'
    restores = []
    for spec in args.mutate:
        relative, before, after = json.loads(spec)
        path = ROOT / relative
        original = path.read_bytes()
        text = original.decode()
        if text.count(before) != 1:
            sys.exit(f'mutation anchor not unique in {relative}: {before!r}')
        restores.append((path, original))
        path.write_text(text.replace(before, after))
        log.write(f'MUTATION {relative} sha256={hashlib.sha256(original).hexdigest()} {before!r} -> {after!r}\n')
    udid = None
    try:
        run(log, 'cargo', 'build', '--locked', '--features', 'test-support', '--example', args.example,
            '--target', 'aarch64-apple-ios-sim')
        app = out / f'{args.example}.app'
        app.mkdir()
        shutil.copy2(TARGET / 'aarch64-apple-ios-sim/debug/examples' / args.example, app / args.example)
        (app / 'Info.plist').write_bytes(plistlib.dumps({
            'CFBundleDevelopmentRegion': 'en', 'CFBundleExecutable': args.example,
            'CFBundleIdentifier': bundle, 'CFBundleInfoDictionaryVersion': '6.0',
            'CFBundleName': args.example, 'CFBundlePackageType': 'APPL',
            'CFBundleShortVersionString': '1.0', 'CFBundleVersion': '1',
            'CFBundleSupportedPlatforms': ['iPhoneSimulator'], 'LSRequiresIPhoneOS': True,
            'MinimumOSVersion': '17.0', 'UIDeviceFamily': [2], 'UILaunchScreen': {},
            'UIApplicationSceneManifest': {'UIApplicationSupportsMultipleScenes': True},
        }))
        run(log, 'codesign', '--force', '--sign', '-', '--timestamp=none', str(app))
        udid = simulator(log)
        run(log, 'xcrun', 'simctl', 'boot', udid)
        run(log, 'xcrun', 'simctl', 'bootstatus', udid, '-b')
        if args.keyboards:
            run(log, 'xcrun', 'simctl', 'spawn', udid, 'defaults', 'write', '-g', 'AppleKeyboards',
                '-array', *args.keyboards.split(','), check=False)
            run(log, 'xcrun', 'simctl', 'spawn', udid, 'defaults', 'write', '-g',
                'AppleKeyboardsExpanded', '-int', '1', check=False)
        run(log, 'xcrun', 'simctl', 'install', udid, str(app))
        project = out / 'qa'
        shutil.copytree(QA, project)
        run(log, 'xcodegen', 'generate', '--spec', str(project / 'project.yml'), '--project', str(project))
        common = ['-project', str(project / 'NativeSearchQA.xcodeproj'), '-scheme', 'NativeSearchQA',
                  '-destination', f'id={udid}', '-derivedDataPath', str(out / 'derived'),
                  'CODE_SIGNING_ALLOWED=NO']
        # The UI test is built before the fixture starts, so the fixture's
        # handshake (a tap) arrives within its wait.
        run(log, 'xcodebuild', 'build-for-testing', *common, timeout=900)
        stderr = out / 'fixture.stderr'
        run(log, 'xcrun', 'simctl', 'launch', '--terminate-running-process', f'--stdout={out / "fixture.stdout"}',
            f'--stderr={stderr}', udid, bundle)
        env = {**os.environ, 'TEST_RUNNER_NATIVE_SEARCH_BUNDLE': bundle}
        tested = run(log, 'xcodebuild', 'test-without-building', *common,
                     '-resultBundlePath', str(out / 'result.xcresult'),
                     f'-only-testing:NativeSearchQA/{args.test}', check=False, env=env, timeout=900)
        log.write(f'XCODEBUILD_EXIT {tested.returncode}\n')
        text = ''
        for _ in range(60):
            text = stderr.read_text(errors='replace') if stderr.exists() else ''
            if f'{args.prefix}_PASS' in text or f'{args.prefix}_ASSERT' in text:
                break
            time.sleep(0.5)
        run(log, 'xcrun', 'simctl', 'terminate', udid, bundle, check=False)
        print(text, flush=True)
        passed = f'{args.prefix}_PASS' in text and f'{args.prefix}_ASSERT' not in text
        if args.expect_assert:
            ok = f'{args.prefix}_ASSERT: {args.expect_assert}' in text and not passed
            verdict = f'NEGATIVE {args.label}: expected assertion {args.expect_assert} ' + ('FAILED AS REQUIRED' if ok else 'ABSENT')
        else:
            ok = passed
            verdict = f'BASELINE {args.label}: ' + ('PASS' if ok else 'FAIL')
        (out / 'verdict.txt').write_text(verdict + '\n')
        print(verdict, flush=True)
        if not ok:
            sys.exit(1)
    finally:
        for path, original in restores:
            path.write_bytes(original)
            if path.read_bytes() != original:
                print(f'RESTORE FAILED {path}', flush=True)
        if udid:
            subprocess.run(['xcrun', 'simctl', 'shutdown', udid], check=False)
            subprocess.run(['xcrun', 'simctl', 'delete', udid], check=False)
            log.write(f'SIMULATOR_DELETED {udid}\n')
            print('SIMULATOR deleted', udid, flush=True)
        shutil.rmtree(out / 'derived', ignore_errors=True)
        log.close()


if __name__ == '__main__':
    main()
