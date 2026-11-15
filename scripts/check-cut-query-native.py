#!/usr/bin/env python3
"""Native stages B1-B5 for NativeClient::cut_and_observe_selection (SH1-234).

5 build/install, 6 UIApplicationMain launches, 7 actual Cuts, in one disposable
hosted iPad Simulator. Stops at the first unexpected result; no retries.

    HERDR_EXPECTED_HEAD=<sha> python3 scripts/check-cut-query-native.py <new evidence dir>

  B1 head H, L1 default policy: main positive, nil selection, loan refusal,
     explicit-disabled refusal, retired refusal (last); 2 Cuts
  B2 fixture mode mutation M1 (NEXT_TURN), L2: intended kill
     p13-cut-first-before-next-turn (DeferredExecuted before the first getter)
  B3 exact restore, L3 --main-only: main positive
  B4 core source mutation M2 (an early selected_range_utf16), L4 --main-only:
     intended kill p13-cut-first-no-earlier-native-query (EarlyQuery, 2 entries)
  B5 exact restore, L5 --main-only, then L6 --explicit-policy on the same install

Simulator injection is not OS IME routing evidence and does not certify P13
S1-S5/B1/S/Z/B2 or physical-device behavior.
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
TARGET = Path(os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target')))
BUNDLE = 'be.ohno.herdr-uikit.cutqueryprobe'
EXAMPLE = 'cut_query_probe'
FIXTURE = 'examples/cut_query_probe.rs'
CORE = 'src/window/text_input/probe/cut_query.rs'
M1 = ('    const NEXT_TURN: bool = false;\n', '    const NEXT_TURN: bool = true;\n')
M2 = ('            obs::permit();\n            self.selected_range_utf16()\n',
      '            let _ = self.selected_range_utf16();\n'
      '            obs::permit();\n            self.selected_range_utf16()\n')
MAIN = ['live-native-client', 'cut-observation-present', 'cut-first-selection-5..5',
        'fixture-callbacks-legal-balanced', 'one-actual-cut', 'tail-draw-at-down-returned',
        'cut-native-length-11', 'cut-text-marked-clipboard']
L1 = ['table-unbalanced-violation', 'setup-caret-4', 'setup-selected-5..7', *MAIN,
      'nil-selection-mode', 'cut-nil-selection-preserved', 'nil-case-one-actual-cut',
      'loan-refused-before-send', 'disable-cut', 'disabled-refused-before-send',
      'restore-policy', 'retire-last', 'retired-refused-before-send']


class Stop(Exception):
    pass


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class Run:
    def __init__(self, evidence, udid):
        self.ev, self.udid = evidence, udid
        self.ledger = {'head': head(), 'udid': udid, 'bundle': BUNDLE, 'stages': [], 'cuts': 0}

    def cmd(self, name, *args):
        result = subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT)
        (self.ev / f'{name}.log').write_text('$ ' + ' '.join(map(str, args)) + '\n' + result.stdout)
        tail = '\n'.join(result.stdout.splitlines()[-15:])
        print(f'::group::{name} exit {result.returncode}\n$ {" ".join(map(str, args))}\n{tail}\n::endgroup::',
              flush=True)
        if result.returncode:
            raise Stop(f'{name} exit {result.returncode}')
        return result.stdout

    def guard(self, label):
        free = shutil.disk_usage(ROOT).free
        print(f'DISK {label} free_bytes={free}', flush=True)
        if free < 6 * 1024**3:
            raise Stop(f'below 6 GiB free before {label}')

    def build_install(self, stage):
        self.guard(stage)
        out = self.cmd(f'{stage}-build', 'cargo', 'build', '--locked', '--features', 'test-support',
                       '--example', EXAMPLE, '--target', 'aarch64-apple-ios-sim',
                       '--message-format=json')
        artifacts = [json.loads(line) for line in out.splitlines()
                     if line.startswith('{') and '"compiler-artifact"' in line]
        example = [a for a in artifacts if a.get('target', {}).get('name') == EXAMPLE]
        if len(example) != 1 or not example[0].get('executable'):
            raise Stop(f'{stage}: example artifact missing')
        exe = Path(example[0]['executable'])
        app = self.ev / f'apps/{stage}/CutQueryProbe.app'
        app.mkdir(parents=True)
        (app / 'Info.plist').write_bytes(plistlib.dumps({
            'CFBundleDevelopmentRegion': 'en', 'CFBundleExecutable': EXAMPLE,
            'CFBundleIdentifier': BUNDLE, 'CFBundleInfoDictionaryVersion': '6.0',
            'CFBundleName': 'SDK Cut Query Probe', 'CFBundlePackageType': 'APPL',
            'CFBundleShortVersionString': '1.0', 'CFBundleVersion': '1',
            'CFBundleSupportedPlatforms': ['iPhoneSimulator'], 'LSRequiresIPhoneOS': True,
            'MinimumOSVersion': '17.0', 'UIDeviceFamily': [2], 'UILaunchScreen': {},
            'UIApplicationSceneManifest': {'UIApplicationSupportsMultipleScenes': True},
            'UISupportedInterfaceOrientations~ipad': ['UIInterfaceOrientationPortrait',
                'UIInterfaceOrientationLandscapeLeft', 'UIInterfaceOrientationLandscapeRight'],
        }))
        shutil.copy2(exe, app / EXAMPLE)
        self.cmd(f'{stage}-codesign', 'codesign', '--force', '--sign', '-', '--timestamp=none', str(app))
        self.cmd(f'{stage}-verify', 'codesign', '--verify', '--strict', str(app))
        uuid = self.cmd(f'{stage}-uuid', 'xcrun', 'dwarfdump', '--uuid', str(app / EXAMPLE)).strip()
        cdhash = [l for l in self.cmd(f'{stage}-cdhash', 'codesign', '-dvvv', str(app)).splitlines()
                  if l.startswith('CDHash=')]
        self.cmd(f'{stage}-install', 'xcrun', 'simctl', 'install', self.udid, str(app))
        container = self.cmd(f'{stage}-container', 'xcrun', 'simctl', 'get_app_container',
                             self.udid, BUNDLE, 'app').strip()
        record = {'stage': stage, 'head': head(), 'fixture_sha256': sha(ROOT / FIXTURE),
                  'core_sha256': sha(ROOT / CORE), 'build_sha256': sha(exe),
                  'installed_sha256': sha(Path(container) / EXAMPLE), 'uuid': uuid,
                  'cdhash': cdhash, 'launches': []}
        if record['installed_sha256'] != sha(app / EXAMPLE):
            raise Stop(f'{stage}: installed binary differs from the signed app')
        self.ledger['stages'].append(record)

    def launch(self, label, args, expect, required, cuts):
        self.guard(label)
        out, err = self.ev / f'{label}.stdout', self.ev / f'{label}.stderr'
        started = self.cmd(f'{label}-launch', 'xcrun', 'simctl', 'launch', '--stdout=' + str(out),
                           '--stderr=' + str(err), self.udid, BUNDLE, *args).strip()
        text = ''
        for _ in range(600):
            text = err.read_text(errors='replace') if err.exists() else ''
            if 'CUT_QUERY_PROBE_PASS:' in text or 'CUT_QUERY_PROBE_ASSERT:' in text:
                break
            time.sleep(0.5)
        else:
            subprocess.run(['xcrun', 'simctl', 'terminate', self.udid, BUNDLE], check=False)
            print(text, flush=True)
            raise Stop(f'{label}: no terminal marker in 300 seconds')
        print(f'::group::{label} stderr\n{text}\n::endgroup::', flush=True)
        lines = text.splitlines()
        marks = lambda tag: [l.split(': ', 1)[1] for l in lines if l.startswith(tag + ': ')]
        checks, asserts = marks('CUT_QUERY_PROBE_CHECK'), marks('CUT_QUERY_PROBE_ASSERT')
        self.ledger['stages'][-1]['launches'].append({
            'launch': label, 'args': args, 'process': started, 'asserts': asserts,
            'traces': marks('CUT_QUERY_PROBE_TRACE'), 'checks': len(checks)})
        missing = [c for c in required if c not in checks]
        if missing:
            raise Stop(f'{label}: required checks missing {missing}')
        passed = 'CUT_QUERY_PROBE_PASS:' in text
        if expect == 'PASS' and (asserts or not passed):
            raise Stop(f'{label}: expected PASS, got {asserts}')
        if expect != 'PASS' and (asserts != [expect] or passed):
            raise Stop(f'{label}: expected only ASSERT {expect}, got {asserts}')
        self.ledger['cuts'] += cuts
        print(f'STAGE_OK {label} expect={expect} cuts={cuts} total={self.ledger["cuts"]}', flush=True)

    def mutated(self, path, before, after, stage, body):
        original = (ROOT / path).read_text()
        if original.count(before) != 1:
            raise Stop(f'{stage}: unique anchor missing in {path}')
        try:
            (ROOT / path).write_text(original.replace(before, after))
            body()
        finally:
            (ROOT / path).write_text(original)
        status = self.cmd(f'{stage}-restore-status', 'git', 'status', '--porcelain').strip()
        if status or sha(ROOT / path) != hashlib.sha256(original.encode()).hexdigest():
            raise Stop(f'{stage}: exact restore of {path} failed: {status}')

    def stages(self):
        self.build_install('B1')
        self.launch('L1', [], 'PASS', L1, 2)
        self.mutated(FIXTURE, *M1, 'B2', lambda: (self.build_install('B2'), self.launch(
            'L2', [], 'p13-cut-first-before-next-turn',
            ['live-native-client', 'next-turn-started', 'cut-observation-present'], 1)))
        self.build_install('B3')
        self.launch('L3', ['--main-only'], 'PASS', MAIN, 1)
        self.mutated(CORE, *M2, 'B4', lambda: (self.build_install('B4'), self.launch(
            'L4', ['--main-only'], 'p13-cut-first-no-earlier-native-query',
            ['live-native-client', 'cut-observation-present', 'p13-cut-first-before-next-turn',
             'cut-deferred-without-intended-witness'], 1)))
        self.build_install('B5')
        self.launch('L5', ['--main-only'], 'PASS', MAIN, 1)
        self.launch('L6', ['--explicit-policy'], 'PASS', ['explicit-policy', *MAIN], 1)
        stages = {s['stage']: s for s in self.ledger['stages']}
        if self.ledger['cuts'] != 7 or len(stages) != 5:
            raise Stop(f'budget mismatch: cuts {self.ledger["cuts"]}, builds {len(stages)}')
        for restored in ('B3', 'B5'):
            if (stages[restored]['fixture_sha256'], stages[restored]['core_sha256']) != (
                    stages['B1']['fixture_sha256'], stages['B1']['core_sha256']):
                raise Stop(f'{restored}: sources differ from B1')


def head():
    return subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True,
                          stdout=subprocess.PIPE).stdout.strip()


def simulator():
    devices = json.loads(subprocess.check_output(
        ['xcrun', 'simctl', 'list', '--json', 'devices', 'available'], text=True))['devices']
    candidates = []
    for runtime, entries in devices.items():
        suffix = runtime.split('.iOS-', 1)[1].split('-') if '.iOS-' in runtime else ['x']
        if all(part.isascii() and part.isdecimal() for part in suffix):
            candidates += [(tuple(map(int, suffix)), runtime, d['deviceTypeIdentifier'])
                           for d in entries
                           if d['isAvailable'] and d['name'].startswith('iPad Pro 13-inch')]
    if not candidates:
        raise Stop('no available iPad Pro 13-inch Simulator')
    _, runtime, kind = max(candidates)
    return subprocess.check_output(['xcrun', 'simctl', 'create', 'herdr-uikit cut query', kind,
                                    runtime], text=True).strip()


def main():
    if platform.system() != 'Darwin' or os.environ.get('GITHUB_ACTIONS') != 'true':
        sys.exit('Runs only in the hosted macOS CI job (owner decision: no local native runs)')
    expected = os.environ.get('HERDR_EXPECTED_HEAD', '')
    if len(sys.argv) != 2 or not expected:
        sys.exit('usage: HERDR_EXPECTED_HEAD=<sha> check-cut-query-native.py <new evidence dir>')
    evidence = Path(sys.argv[1])
    if not evidence.is_absolute() or ROOT in evidence.parents or evidence.exists():
        sys.exit('evidence must be a new absolute directory outside the checkout')
    if head() != expected:
        sys.exit(f'NATIVE_STOP: checkout {head()} is not the exact head {expected}')
    if subprocess.run(['git', 'status', '--porcelain'], cwd=ROOT, text=True,
                      stdout=subprocess.PIPE).stdout.strip():
        sys.exit('NATIVE_STOP: requires a clean checkout')
    evidence.mkdir(parents=True)
    udid = simulator()
    run = Run(evidence, udid)
    result = 1
    try:
        run.cmd('boot', 'xcrun', 'simctl', 'bootstatus', udid, '-b')
        run.stages()
        run.ledger['result'] = (f'NATIVE_PASS head={expected} builds=5 launches=6 '
                                f'cuts={run.ledger["cuts"]}')
        result = 0
    except Stop as stop:
        run.ledger['result'] = f'NATIVE_STOP: {stop}'
    finally:
        print('NATIVE_LEDGER ' + json.dumps(run.ledger, ensure_ascii=False), flush=True)
        print(run.ledger['result'], flush=True)
        subprocess.run(['xcrun', 'simctl', 'shutdown', udid], check=False)
        subprocess.run(['xcrun', 'simctl', 'delete', udid], check=True)
        print('NATIVE SIMULATOR deleted', udid, flush=True)
    sys.exit(result)


if __name__ == '__main__':
    main()
