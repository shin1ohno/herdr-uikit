#!/usr/bin/env python3
"""Negative controls for the input mode indicator (issue #139, AC12).

Each mutation reverts one guarded place; the named test or fixture assertion
must then fail at an assertion, after the case's own first-responder and
active preconditions.

  input_mode_mutations.py host            # macOS: cargo test --test selection_display_state
  input_mode_mutations.py unit            # leased Mac: Simulator unit contracts per mutation
  input_mode_mutations.py uikit OUTDIR    # leased Mac: fixture baseline + each UIKit mutation

`unit` runs scripts/test-simulator.py and `uikit` runs
scripts/check-native-search.py; INPUT_MODE_UNIT_RUNNER and INPUT_MODE_RUNNER
name a wrapper with the same arguments instead (for example one that names
the disposable Simulator differently). INPUT_MODE_ONLY=label,label limits a
run. Every source file is restored byte for byte after each run.
"""
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
STATE = 'src/window/text_input/selection_display/state.rs'
DISPLAY = 'src/window/text_input/selection_display.rs'
CURSOR = 'src/window/text_input/selection_display/cursor.rs'
SYNC = 'src/window/text_input/sync.rs'
RETIREMENT = 'src/window/text_input/retirement.rs'
LIFECYCLE = 'src/window/lifecycle.rs'
MOD = 'src/window/text_input/mod.rs'

# (label, [(file, before, after)], failing host test)
HOST = [
    ('g4-resign-ignored', [(STATE, '        if self.activated {\n            self.activated = false;',
                            '        if false {\n            self.activated = false;')],
     'resign_deactivates'),
    ('g5-caret-not-compared', [(STATE, '        if Some(key) != self.caret {', '        if true {')],
     'same_caret_does_not_update'),
    ('acquire-toggles-off', [(STATE, '        self.activated = true;\n        sink.set_activated(true);',
                              '        self.activated = true;\n        sink.set_activated(false);\n'
                              '        sink.set_activated(true);')],
     'acquire_when_active_does_not_toggle'),
]

ATTACH_SUCCESSOR = ('            // `textInput` is fixed at creation: the successor gets its own.\n'
                    '            view.attach_selection_display(&parent);\n')
DETACH_OLD = ('        // The old interaction is off and gone before the successor can\n'
              '        // acquire, so two are never active at once.\n'
              '        old.detach_selection_display();\n')
DROP_DETACH = '        current.detach_selection_display();\n'

# (label, [(file, before, after)], Simulator unit assertion message that must appear)
UNIT = [
    ('g9-successor-not-attached', [(RETIREMENT, ATTACH_SUCCESSOR, '')], 'retirement-moves-the-interaction'),
    ('g10-old-not-detached', [(RETIREMENT, DETACH_OLD, '')], 'old-display-removed'),
    ('g12-drop-keeps-interaction', [(MOD, DROP_DETACH, '')], 'drop-detaches'),
]

# (label, [(file, before, after)], fixture assertion that must fail)
UIKIT = [
    ('g1-became-not-activated',
     [(SYNC, '            ivars.target.text_input_did_acquire();\n'
             '            self.selection_display(|display| display.activate());\n',
       '            ivars.target.text_input_did_acquire();\n')],
     'im0-precondition-active'),
    ('g2-requested-not-activated',
     [(SYNC, '                    ivars.target.text_input_did_acquire();\n'
             '                    self.selection_display(|display| display.activate());\n',
       '                    ivars.target.text_input_did_acquire();\n')],
     'im5b-request-while-responder-activates'),
    ('g3-resign-not-deactivated',
     [(SYNC, '        self.selection_display(|display| display.deactivate());\n', '')],
     'im5-key-only-deactivates'),
    ('g6-caret-not-forwarded',
     [(SYNC, '        self.selection_display(|display| display.caret_moved(bounds));\n', '')],
     'im4-one-move-one-reread'),
    ('g7-caret-move-notifies-delegate',
     [(SYNC, '        self.selection_display(|display| display.caret_moved(bounds));\n',
       '        self.selection_display(|display| display.caret_moved(bounds));\n'
       '        self.notify(Change::Selection);\n')],
     'im7-no-delegate-traffic'),
    ('g8-system-cursor-view',
     [(DISPLAY, '        unsafe { interaction.setCursorView(cursor.as_view()) };\n', '')],
     'im2-transparent-cursor-class'),
    ('g8b-cursor-view-draws',
     [(CURSOR, 'view.setBackgroundColor(Some(&UIColor::clearColor()));',
       'view.setBackgroundColor(Some(&UIColor::blueColor()));')],
     'im2-no-system-caret-pixels'),
    ('g9-successor-not-attached', [(RETIREMENT, ATTACH_SUCCESSOR, '')],
     'im6-one-interaction-after-retirement'),
    ('g10-old-not-detached', [(RETIREMENT, DETACH_OLD, '')],
     'im6-one-interaction-after-retirement'),
    ('g11-window-not-attached',
     [(LIFECYCLE, '            text_input.attach_selection_display(&view);\n', '')],
     'im0-precondition-active'),
    ('g12-drop-keeps-interaction', [(MOD, DROP_DETACH, '')], 'im6c-close-leaves-none'),
]


def apply(edits):
    originals = {}
    for relative, before, after in edits:
        path = ROOT / relative
        originals.setdefault(path, path.read_bytes())
        text = path.read_text()
        if text.count(before) != 1:
            sys.exit(f'anchor not unique in {relative}: {before!r}')
        path.write_text(text.replace(before, after))
    return originals


def restore(originals):
    for path, data in originals.items():
        path.write_bytes(data)
        if path.read_bytes() != data:
            sys.exit(f'restore failed: {path}')


def selected(label):
    only = os.environ.get('INPUT_MODE_ONLY', '')
    return not only or label in only.split(',')


def host():
    def test(name=None):
        args = ['cargo', 'test', '--locked', '--test', 'selection_display_state']
        if name:
            args += ['--', '--exact', name]
        return subprocess.run(args, cwd=ROOT, text=True, capture_output=True)
    base = test()
    print(base.stdout[-600:])
    if base.returncode:
        sys.exit('baseline host tests fail')
    failures = 0
    for label, edits, name in HOST:
        if not selected(label):
            continue
        originals = apply(edits)
        try:
            result = test(name)
        finally:
            restore(originals)
        failed = result.returncode != 0 and f'{name} ... FAILED' in result.stdout
        print(f'HOST_MUTATION {label}: {name} ' + ('FAILED AS REQUIRED' if failed else 'DID NOT FAIL'))
        failures += not failed
    restored = test()
    print('HOST_RESTORED', 'PASS' if restored.returncode == 0 else 'FAIL')
    sys.exit(1 if failures or restored.returncode else 0)


def unit():
    runner = os.environ.get('INPUT_MODE_UNIT_RUNNER', str(ROOT / 'scripts' / 'test-simulator.py'))
    failures = 0
    for label, edits, message in UNIT:
        if not selected(label):
            continue
        originals = apply(edits)
        try:
            result = subprocess.run([sys.executable, runner], cwd=ROOT, text=True,
                                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        finally:
            restore(originals)
        failed = (result.returncode != 0 and message in result.stdout
                  and 'selection_display_attaches_moves_on_retirement_and_detaches ... FAILED' in result.stdout)
        lines = result.stdout.splitlines()
        for n, text in enumerate(lines):
            if 'panicked at' in text:
                print('\n'.join(lines[n:n + 2]), flush=True)
        print(f'UNIT_MUTATION {label}: {message} ' + ('FAILED AS REQUIRED' if failed else 'DID NOT FAIL'),
              flush=True)
        failures += not failed
    sys.exit(1 if failures else 0)


def uikit(out):
    script = os.environ.get('INPUT_MODE_RUNNER', str(ROOT / 'scripts' / 'check-native-search.py'))
    common = ['--example', 'input_mode_probe', '--test', 'InputMode/testInputMode', '--prefix', 'INPUTMODE']
    runs = [('baseline', [], None)] + UIKIT + [('restored', [], None)]
    verdicts = []
    for label, edits, assertion in runs:
        if not selected(label):
            continue
        args = [sys.executable, script, *common, '--output', f'{out}/{label}', '--label', label]
        for relative, before, after in edits:
            args += ['--mutate', json.dumps([relative, before, after])]
        if assertion:
            args += ['--expect-assert', assertion]
        result = subprocess.run(args, cwd=ROOT)
        verdicts.append((label, assertion, result.returncode == 0))
        print(f'UIKIT_RUN {label} expect={assertion} ok={result.returncode == 0}', flush=True)
    for line in verdicts:
        print('UIKIT_VERDICT', *line)
    sys.exit(0 if all(ok for _, _, ok in verdicts) else 1)


if __name__ == '__main__':
    if sys.argv[1:2] == ['host']:
        host()
    elif sys.argv[1:2] == ['unit']:
        unit()
    elif sys.argv[1:2] == ['uikit'] and len(sys.argv) == 3:
        uikit(sys.argv[2])
    else:
        sys.exit(__doc__)
