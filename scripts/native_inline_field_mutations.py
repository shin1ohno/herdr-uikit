#!/usr/bin/env python3
"""Negative controls for the native inline field (docs/native-inline-field.md).

Each mutation removes one guarded decision or path; the named host test or
fixture assertion must then fail, after the fixture's own "opened" checks.

  native_inline_field_mutations.py host           # Linux or macOS: cargo test --test native_inline_field_policy
  native_inline_field_mutations.py uikit OUTDIR   # Mac: baseline + each UIKit mutation on a disposable Simulator

`uikit` runs scripts/check-native-search.py (or the script NATIVE_SEARCH_CHECK
names, for a wrapper that runs it unchanged) once per mutation;
NATIVE_INLINE_ONLY=label,label limits the runs. Every source file is
restored byte for byte after each run.
"""
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
POLICY = 'src/window/native_inline_field/policy.rs'
TESTS = 'tests/native_inline_field_policy.rs'
CONTROLLER = 'src/window/native_inline_field/controller.rs'
PRESENT = 'src/window/native_inline_field/present.rs'
MOD = 'src/window/native_inline_field/mod.rs'

# (label, [(file, before, after)], failing host test)
HOST = [
    ('unchanged-not-detected', [(POLICY, '    if trimmed == initial.trim() {', '    if false {')],
     'commit_unchanged_is_unchanged'),
    ('empty-sent', [(POLICY, '    if trimmed.is_empty() && !allow_empty {', '    if false {')],
     'empty_without_allow_is_hint'),
    ('allow-empty-ignored', [(POLICY, '    if trimmed.is_empty() && !allow_empty {', '    if trimmed.is_empty() {')],
     'empty_with_allow_commits_empty'),
    ('no-limit', [(POLICY, 'text.trim().chars().take(max_chars)', 'text.trim().chars().take(usize::MAX)')],
     'commit_trims_and_limits'),
    ('clamp-cuts-marked', [(POLICY, '    if marked || text.chars().count() <= max_chars {',
                            '    if text.chars().count() <= max_chars {')],
     'clamp_never_cuts_marked'),
    ('clamp-never-cuts', [(POLICY, '    if marked || text.chars().count() <= max_chars {', '    if true {')],
     'clamp_cuts_after_composition'),
    ('no-outset', [(POLICY, '        x: label.x - outset_h,', '        x: label.x,')],
     'rect_outset_and_min_width'),
    ('no-min-width', [(POLICY, '        w: (label.w + 2.0 * outset_h).max(min_width),', '        w: label.w + 2.0 * outset_h,')],
     'rect_outset_and_min_width'),
    ('always-visible', [(POLICY, '    if right <= x || bottom <= y {', '    if false {')],
     'visible_hidden_when_clip_empty'),
    ('gate-per-kind', [(TESTS, 'const KIND_GATES: bool = false;', 'const KIND_GATES: bool = true;')],
     'gate_shared_with_search'),
    ('unbounded-queue', [(POLICY, '        if self.events.len() >= self.cap {', '        if false {')],
     'events_bounded_8_in_order'),
    ('capacity-9', [(POLICY, 'pub const EVENT_CAPACITY: usize = 8;', 'pub const EVENT_CAPACITY: usize = 9;')],
     'events_bounded_8_in_order'),
    ('unchanged-as-committed', [(POLICY, '            _ => NativeInlineDismissReason::Unchanged,',
                                 '            _ => NativeInlineDismissReason::Committed,')],
     'seven_closers_one_dismissed'),
    ('escape-as-outside-tap', [(POLICY, '        GateReason::Escape => NativeInlineDismissReason::Escape,',
                                '        GateReason::Escape => NativeInlineDismissReason::OutsideTap,')],
     'seven_closers_one_dismissed'),
]

# (label, [(file, before, after)], fixture assertion that must fail)
UIKIT = [
    ('overlay-passes-touches',
     [(CONTROLLER, '        overlay.setUserInteractionEnabled(true);', '        overlay.setUserInteractionEnabled(false);')],
     'outside-tap-dismissed'),
    # `selectAll:` alone makes a UITextField first responder, so the select
    # goes too: what is left is `Opened` for a field never acquired. One edit:
    # check-native-search.py restores two edits of one file in the wrong order.
    ('opened-before-acquire',
     [(PRESENT, '            let focused = window.acquire_field(presented.field_view());\n'
                '            if focused && select_all {\n                presented.select_all();\n            }',
       '            let focused = true;\n            let _ = select_all;')],
     'p1-field-first-responder'),
    ('return-while-marked',
     [(CONTROLLER, '    fn should_return(&self, field: &UITextField) -> bool {\n        if marked(field) {\n            return false;\n        }\n',
       '    fn should_return(&self, field: &UITextField) -> bool {\n')],
     'return-and-escape-while-marked-keep-open'),
    ('escape-while-marked',
     [(CONTROLLER, 'gate::escape_dismisses(usages.iter().copied(), marked(self))',
       'gate::escape_dismisses(usages.iter().copied(), false)')],
     'return-and-escape-while-marked-keep-open'),
    ('clip-ignored',
     [(CONTROLLER, '        let (shown, hidden) = policy::visible(frame, clip);',
       '        let (shown, hidden) = policy::visible(frame, None);\n        let _ = clip;')],
     'hidden-outside-clip-keeps-editing'),
    ('set-frame-ignored',
     [(MOD, '    window.defer(move |w| w.place_inline(id));', '    let _ = id;')],
     'set-frame-moves-container'),
    ('select-all-skipped',
     [(PRESENT, '            if focused && select_all {\n                presented.select_all();\n            }',
       '            let _ = select_all;')],
     'p1-selected-text-replaced-on-type'),
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


def host():
    def test(name=None):
        args = ['cargo', 'test', '--locked', '--test', 'native_inline_field_policy', '--', '--test-threads=2']
        if name:
            args += ['--exact', name]
        return subprocess.run(args, cwd=ROOT, text=True, capture_output=True)
    base = test()
    print(base.stdout[-600:])
    if base.returncode:
        sys.exit('baseline host tests fail')
    failures = 0
    for label, edits, name in HOST:
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


def uikit(out):
    script = os.environ.get('NATIVE_SEARCH_CHECK', str(ROOT / 'scripts' / 'check-native-search.py'))
    common = ['--example', 'native_inline_field_probe', '--test', 'InlineField/testProbe', '--prefix', 'INLINE']
    runs = [('baseline', [], None)] + UIKIT + [('restored', [], None)]
    only = os.environ.get('NATIVE_INLINE_ONLY', '')
    verdicts = []
    for label, edits, assertion in runs:
        if only and label not in only.split(','):
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
    elif sys.argv[1:2] == ['uikit'] and len(sys.argv) == 3:
        uikit(sys.argv[2])
    else:
        sys.exit(__doc__)
