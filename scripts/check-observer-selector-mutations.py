#!/usr/bin/env python3
"""Native edit observer selector negatives, only in a clean isolated checkout
under a newly granted exclusive lease.

Each mutant must make native_edit_selectors fail on its named assertion. The
boundary mutant must survive: the window-less host never reaches dispatch, so
a mutant confined to the Dispatched arm is outside this test's reach.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OBSERVER = ROOT / 'src/native_edit_observer/mod.rs'
ACTIONS = ROOT / 'src/window/text_input/actions.rs'
TEST = 'native_edit_selectors_record_actual_entry_and_terminal_pairs'
CFG = '        #[cfg(all(target_os = "ios", target_abi = "sim", feature = "test-support"))]\n'
COMPLETE_ALL = CFG + '''        match checked {
            CheckedDispatch::RejectedDelivery => {
                invocation.complete(
                    observer::Decision::RefusedMenuDelivery,
                    observer::Outcome::Refused,
                );
            }
            CheckedDispatch::Disabled => {
                invocation.complete(decision.observed(), observer::Outcome::Refused);
            }
'''
DISPATCHED = '''            CheckedDispatch::Dispatched(summary) => invocation.complete(
                decision.observed(),
                observer::Outcome::Dispatched {
                    key_down_handled: summary.handled,
                    fallback: if summary.fallback_invoked {
                        observer::Fallback::Invoked
                    } else {
                        observer::Fallback::NotEligible
                    },
                },
            ),
        }
'''
# (label, file, before, after, expected assertion or None for the survivor)
MUTATIONS = [
    ('disabled', OBSERVER,
     '                registration.0.set(arm.view_generation);\n'
     '                ENABLED.store(true, SeqCst);\n',
     '                registration.0.set(arm.view_generation);\n'
     '                ENABLED.store(false, SeqCst);\n',
     'native-selector-armed-entry-recorded'),
    ('entry-registration', ACTIONS,
     '        let invocation = observer::InvocationGuard::enter(\n'
     '            &self.ivars().observation,\n',
     '        let invocation = observer::InvocationGuard::enter(\n'
     '            &observer::Registration::default(),\n',
     'native-selector-entry-uses-view-registration'),
    ('terminal', ACTIONS, COMPLETE_ALL + DISPATCHED,
     CFG + '        {\n            let _ = checked;\n            drop(invocation);\n        }\n',
     'native-selector-terminal-refusal'),
    ('boundary-dispatched-arm-only', ACTIONS, DISPATCHED,
     '            CheckedDispatch::Dispatched(summary) => {\n'
     '                let _ = (summary.handled, summary.fallback_invoked);\n'
     '                drop(invocation);\n            }\n        }\n',
     None),
]


def run(*args):
    return subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT)


def guard():
    space = os.statvfs(ROOT)
    if space.f_bavail * space.f_frsize < 6 * 1024**3:
        sys.exit('STOP: less than 6 GiB available; delete nothing')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    if platform.system() != 'Darwin' or os.environ.get('HERDR_OBSERVER_LEASE') != 'exclusive':
        sys.exit('Requires macOS and a newly granted exclusive lease')
    status = run('git', 'status', '--porcelain')
    if status.returncode or status.stdout.strip():
        sys.exit('Requires a clean isolated task checkout')
    runner = os.environ.get('CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUNNER', '')
    if not Path(runner).is_absolute() or not Path(runner).is_file():
        sys.exit('Set the absolute simulator runner path')
    output = os.environ.get('HERDR_OBSERVER_EVIDENCE_DIR', '')
    if not output or not Path(output).is_absolute():
        sys.exit('Set a new absolute task-owned evidence directory')
    evidence = Path(output)
    evidence.mkdir(parents=True, exist_ok=False)
    head = run('git', 'rev-parse', 'HEAD')
    if head.returncode:
        sys.exit('Cannot snapshot exact head')
    originals = {path: path.read_bytes() for path in (OBSERVER, ACTIONS)}
    command = ['cargo', 'test', '--locked', '--all-features', '--target',
               'aarch64-apple-ios-sim', '--lib', TEST, '--', '--nocapture']
    manifest = {'head': head.stdout.strip(), 'fixture': 'window-less SDK text host',
                'command': command,
                'sources': {str(p.relative_to(ROOT)): sha(b) for p, b in originals.items()},
                'mutations': []}
    for label, path, before, after, assertion in MUTATIONS:
        text = originals[path].decode()
        if text.count(before) != 1:
            sys.exit(f'{label}: unique source anchor missing')
        manifest['mutations'].append({
            'label': label, 'file': str(path.relative_to(ROOT)), 'before': before,
            'after': after, 'assertion': assertion,
            'expected': 'survive' if assertion is None else 'fail',
            'mutant_sha256': sha(text.replace(before, after).encode())})
    (evidence / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')

    def test(label, assertion=None, survive=False):
        guard()
        result = run(*command)
        (evidence / f'{label}.log').write_text(result.stdout)
        (evidence / f'{label}.status.json').write_text(json.dumps({
            'returncode': result.returncode,
            'sources': {str(p.relative_to(ROOT)): sha(p.read_bytes()) for p in originals},
        }) + '\n')
        print(label, 'returncode', result.returncode, flush=True)
        if assertion:
            valid = (result.returncode == 101 and TEST in result.stdout
                     and 'panicked at' in result.stdout
                     and 'test result: FAILED' in result.stdout
                     and assertion in result.stdout)
        else:
            valid = result.returncode == 0 and '1 passed' in result.stdout
        if not valid:
            what = 'survivor did not pass' if survive else 'not the required outcome'
            raise RuntimeError(f'{label}: {what}')

    test('baseline')
    failures = []
    for label, path, before, after, assertion in MUTATIONS:
        try:
            path.write_bytes(originals[path].decode().replace(before, after).encode())
            test(label, assertion, survive=assertion is None)
        except RuntimeError as error:
            failures.append(str(error))
        finally:
            path.write_bytes(originals[path])
            if path.read_bytes() != originals[path]:
                raise RuntimeError('exact source restore failed')
    # Restore proof is required even when an intended negative misbehaved.
    test('restored')
    clean = run('git', 'status', '--porcelain')
    if clean.returncode or clean.stdout.strip():
        raise RuntimeError('checkout not clean after restore')
    if failures:
        sys.exit('MUTATION-OUTCOME-MISMATCH: ' + '; '.join(failures))
    print('ALL-MUTATIONS-AS-EXPECTED', flush=True)


if __name__ == '__main__':
    main()
