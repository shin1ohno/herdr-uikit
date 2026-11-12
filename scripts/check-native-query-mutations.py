#!/usr/bin/env python3
"""Native helper negatives, only in a clean isolated checkout under a new lease."""
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'src/window/text_input/probe/query.rs'
TEST = 'native_queries_observe_captured_client_without_selection_repair'


def run(*args):
    return subprocess.run(args, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT)


def guard():
    space = os.statvfs(ROOT)
    if space.f_bavail * space.f_frsize < 6 * 1024**3:
        sys.exit('STOP: less than 6 GiB available; delete nothing')


def main():
    if platform.system() != 'Darwin' or os.environ.get('HERDR_RETIREMENT_LEASE') != 'exclusive':
        sys.exit('Requires macOS and a newly granted exclusive lease')
    status = run('git', 'status', '--porcelain')
    if status.returncode or status.stdout.strip():
        sys.exit('Requires a clean isolated task checkout')
    runner = os.environ.get('CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUNNER', '')
    if not Path(runner).is_absolute() or not Path(runner).is_file():
        sys.exit('Set the absolute simulator runner path')
    output = os.environ.get('HERDR_QUERY_EVIDENCE_DIR', '')
    if not output or not Path(output).is_absolute():
        sys.exit('Set a new absolute task-owned evidence directory')
    evidence = Path(output)
    evidence.mkdir(parents=True, exist_ok=False)
    original = SOURCE.read_bytes()
    text = original.decode()
    head = run('git', 'rev-parse', 'HEAD')
    if head.returncode:
        sys.exit('Cannot snapshot exact head')
    command = ['cargo', 'test', '--locked', '--all-features', '--target',
               'aarch64-apple-ios-sim', TEST, '--', '--nocapture']
    anchor = '    pub fn document_length_utf16(&self) -> Result<NativeQuery<Option<usize>>, NativeQueryError> {\n        self.observe(|| {\n'
    reversed_check = '            if range.start > range.end {\n                return Err(NativeQueryError::ReversedRange);\n            }\n'
    document_before = anchor + '''            let Some(begin) = self.beginning() else {
                return Ok(None);
            };
            // SAFETY: standard position getter on the same main-thread receiver.
            let end: Option<Retained<UITextPosition>> =
                unsafe { msg_send![&*self.0, endOfDocument] };
            end.map(|end| self.offset(&begin, &end)).transpose()
        })
    }
'''
    # One method replacement: both identity and real native selectors now use
    # the registered TextInputView's current endpoint instead of retained self.
    # Non-test library compilation keeps the same endpoint; the actual lib-test
    # initializer exercises the defect after installing its real second client.
    document_after = document_before.replace('        self.observe(|| {', '''        #[cfg(test)]
        let sampled = super::super::tests::native_queries::q05_current_capture(self);
        #[cfg(not(test))]
        let sampled = self.clone();
        sampled.observe(|| {''').replace('self.beginning()', 'sampled.beginning()').replace(
            '&*self.0', '&*sampled.0').replace('self.offset(', 'sampled.offset(')
    mutations = [
        ('q05-recapture-current', document_before, document_after,
         'p13-q05-retained-client-not-current'),
        ('query-repairs-selection', anchor, anchor +
         '            self.0.setSelectedTextRange(self.0.selectedTextRange().as_deref());\n',
         'p13-query-does-not-set-selection'),
        ('retired-zero-hidden', anchor, anchor +
         '            if !objc2::DefinedClass::ivars(&*self.0).target.client.live() { return Ok(None); }\n',
         'p13-retired-native-zero'),
        ('reversed-setup-normalized', reversed_check, '',
         'p13-reversed-setup-rejected-before-setter'),
    ]
    manifest = {'head': head.stdout.strip(), 'fixture': 'SDK-host only',
                'source_sha256': hashlib.sha256(original).hexdigest(),
                'command': command, 'mutations': []}
    for label, before, after, assertion in mutations:
        if text.count(before) != 1:
            sys.exit(f'{label}: unique source anchor missing')
        mutant = text.replace(before, after).encode()
        manifest['mutations'].append({'label': label, 'before': before, 'after': after,
                                     'assertion': assertion,
                                     'mutant_sha256': hashlib.sha256(mutant).hexdigest()})
    (evidence / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')

    def test(label, assertion=None):
        guard()
        result = run(*command)
        (evidence / f'{label}.log').write_text(result.stdout)
        (evidence / f'{label}.status.json').write_text(json.dumps({
            'returncode': result.returncode,
            'source_sha256': hashlib.sha256(SOURCE.read_bytes()).hexdigest(),
        }) + '\n')
        print(label, result.stdout, flush=True)
        if assertion:
            valid = (result.returncode == 101 and TEST in result.stdout
                     and 'panicked at' in result.stdout and 'test result: FAILED' in result.stdout
                     and assertion in result.stdout)
        else:
            valid = result.returncode == 0 and '1 passed' in result.stdout
        if not valid:
            raise RuntimeError(f'{label}: not the required executed assertion outcome')

    test('baseline')
    for label, before, after, assertion in mutations:
        failure = None
        try:
            SOURCE.write_bytes(text.replace(before, after).encode())
            test(label, assertion)
        except BaseException as error:
            failure = error
        finally:
            SOURCE.write_bytes(original)
            if SOURCE.read_bytes() != original:
                raise RuntimeError('exact source restore failed')
        # Restore proof is required even when the intended negative failed setup.
        test(label + '-restored')
        if failure is not None:
            raise failure


if __name__ == '__main__':
    main()
