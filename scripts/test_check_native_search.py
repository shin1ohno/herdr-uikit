#!/usr/bin/env python3
"""Host test for scripts/check-native-search.py's --mutate restore.

Copies the script into a throwaway root with a stub `cargo` that fails at
once, so the script goes from its mutations straight to its finally block.
Every mutated file must then hold its original bytes (herdr-uikit#27). No
Simulator, build or signing; macOS only, like the script.

  python3 scripts/test_check_native_search.py
"""
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parent / 'check-native-search.py'

ONE = 'fn one() -> bool {\n    first_guard()\n}\n\nfn two() -> bool {\n    second_guard()\n}\n'
OTHER = 'fn other() -> bool {\n    third_guard()\n}\n'


@unittest.skipUnless(platform.system() == 'Darwin', 'check-native-search.py requires macOS')
class MutateRestore(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.root = self.tmp / 'root'
        (self.root / 'scripts').mkdir(parents=True)
        (self.root / 'src').mkdir()
        shutil.copy2(SCRIPT, self.root / 'scripts' / SCRIPT.name)
        bin_dir = self.tmp / 'bin'
        bin_dir.mkdir()
        cargo = bin_dir / 'cargo'
        cargo.write_text('#!/bin/sh\nexit 101\n')
        cargo.chmod(0o755)
        self.env = {**os.environ, 'PATH': f'{bin_dir}:{os.environ["PATH"]}'}
        (self.root / 'src' / 'one.rs').write_text(ONE)
        (self.root / 'src' / 'other.rs').write_text(OTHER)
        self.runs = 0

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_script(self, edits):
        self.runs += 1
        out = self.tmp / f'out{self.runs}'
        args = [sys.executable, str(self.root / 'scripts' / SCRIPT.name), '--example', 'probe',
                '--test', 'Probe/testProbe', '--prefix', 'PROBE', '--output', str(out)]
        for edit in edits:
            args += ['--mutate', json.dumps(edit)]
        result = subprocess.run(args, cwd=self.root, env=self.env, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        log = (out / 'run.log').read_text() if (out / 'run.log').exists() else ''
        return result, log

    def assert_restored(self, result):
        self.assertNotIn('RESTORE FAILED', result.stdout)
        self.assertEqual((self.root / 'src' / 'one.rs').read_text(), ONE, result.stdout)
        self.assertEqual((self.root / 'src' / 'other.rs').read_text(), OTHER, result.stdout)

    def test_two_edits_of_one_file_restore_its_original_bytes(self):
        result, log = self.run_script([
            ['src/one.rs', '    first_guard()\n', '    true\n'],
            ['src/one.rs', '    second_guard()\n', '    false\n'],
            ['src/other.rs', '    third_guard()\n', '    true\n'],
        ])
        # The stub build failed after every edit was applied.
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(log.count('MUTATION '), 3, log)
        self.assert_restored(result)

    def test_a_later_missing_anchor_restores_the_earlier_edits(self):
        result, log = self.run_script([
            ['src/one.rs', '    first_guard()\n', '    true\n'],
            ['src/other.rs', '    no_such_line()\n', '    true\n'],
        ])
        self.assertIn('mutation anchor not unique in src/other.rs', result.stdout)
        self.assertEqual(log.count('MUTATION '), 1, log)
        self.assert_restored(result)


if __name__ == '__main__':
    unittest.main()
