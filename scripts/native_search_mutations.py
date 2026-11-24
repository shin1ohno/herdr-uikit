#!/usr/bin/env python3
"""Negative controls for the native modal gate (review F8).

Each mutation removes one guarded transition or reader; the named test or
fixture assertion must then fail, after the fixture's own "opened" checks.

  native_search_mutations.py host            # Linux or macOS: cargo test --test native_modal
  native_search_mutations.py uikit OUTDIR    # leased Mac: baseline + each UIKit mutation

Every source file is restored byte for byte after each run.
"""
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
GATE = 'src/window/native_modal/gate.rs'
FENCE = 'src/window/native_modal/fence.rs'
INTENT = 'src/window/native_modal/intent.rs'
QUEUE = 'src/window/native_modal/queue.rs'

# (label, [(file, before, after)], failing host test)
HOST = [
    ('restore-before-release', [(GATE, '        if released {\n            self.set(id, Phase::Restoring);',
                                 '        if true {\n            self.set(id, Phase::Restoring);')],
     'h1_restore_waits_for_release_and_for_an_actual_responder'),
    ('second-request-accepted', [(GATE, '        if self.current.is_some() {\n            return Err(Busy);',
                                  '        if false {\n            return Err(Busy);')],
     'h1_a_second_presentation_is_refused_until_the_first_is_restored'),
    ('failure-as-dismissal', [(GATE, 'vec![Effect::Emit(Note::Failed(failure)), Effect::Restore]',
                               'vec![Effect::Emit(Note::Dismissed(DismissReason::System)), Effect::Restore]')],
     'f2_presentation_failure_is_reported_once_without_a_dismissal'),
    ('opened-without-field', [(GATE, '                if focused {\n                    vec![Effect::Emit(Note::Opened)]',
                               '                if true {\n                    vec![Effect::Emit(Note::Opened)]')],
     'f2_opened_only_after_the_field_is_first_responder'),
    ('close-dismisses-twice', [(GATE, '            Phase::AwaitingConsumer | Phase::Restoring => {}',
                                '            Phase::AwaitingConsumer | Phase::Restoring => {\n'
                                '                effects.push(Effect::Emit(Note::Dismissed(DismissReason::WindowClosed)));\n'
                                '            }')],
     'f2_window_close_ends_every_phase_once'),
    ('restored-in-any-phase', [(GATE, '            Some(Phase::Restoring) => {\n                self.current = None;',
                                '            Some(_) => {\n                self.current = None;')],
     'h1_restore_waits_for_release_and_for_an_actual_responder'),
    ('restore-never-gives-up', [(GATE, '            Some(Phase::Restoring) if attempt + 1 < RESTORE_ATTEMPTS => {',
                                 '            Some(Phase::Restoring) if true => {')],
     'h1_an_unsettled_restore_retries_then_opens_with_nothing_restored'),
    ('restore-timeout-in-any-phase', [(GATE, '            Some(Phase::Restoring) if attempt + 1 < RESTORE_ATTEMPTS => {',
                                       '            Some(_) if attempt + 1 < RESTORE_ATTEMPTS => {')],
     'h1_an_unsettled_restore_retries_then_opens_with_nothing_restored'),
    ('late-input-admitted', [(GATE, '        if !self.blocks() {\n            return false;\n        }\n        self.late_drops',
                              '        if true {\n            return false;\n        }\n        self.late_drops')],
     'f5_late_input_is_dropped_from_acceptance_to_the_restore'),
    ('escape-while-composing', [(GATE, '    !composing && usages.into_iter()', '    usages.into_iter()')],
     'escape_closes_only_when_not_composing'),
    ('escape-ignored', [(GATE, '.any(|usage| usage == ESCAPE_USAGE)', '.any(|_| false)')],
     'escape_closes_only_when_not_composing'),
    ('release-clears-all-keys', [(FENCE, '        self.usages.remove(&usage);', '        self.usages.clear();')],
     'h3_two_held_keys_release_independently'),
    ('no-overflow', [(FENCE, '            self.overflow = true;\n            return;', '            return;')],
     'h3_overflow_fences_everything_until_nothing_is_held'),
    ('lost-release-kept', [(FENCE, '        self.usages.retain(|usage| held.contains(usage));\n', '')],
     'h3_a_lost_release_ends_when_uikit_stops_reporting_the_key'),
    ('first-keyboard-request-wins', [(INTENT, '    pub fn hide(&mut self) {\n        self.request = Request::Hide;',
                                      '    pub fn hide(&mut self) {\n        if self.request == Request::None {\n'
                                      '            self.request = Request::Hide;\n        }')],
     'h2_keyboard_intent_keeps_the_last_request_and_counts_notifications'),
    ('menu-runs-while-shut', [(GATE, '    !fenced_key && !shut_when_performed && !shut_when_delivered', '    true')],
     'h4_menu_commands_never_run_or_show_enabled_while_shut'),
    ('queries-not-coalesced', [(QUEUE, '        if event.is_query()\n            && let Some(last)',
                                '        if false\n            && let Some(last)')],
     'f11_queries_coalesce_in_place_and_ordering_events_keep_order'),
]

# (label, [(file, before, after)], fixture assertion that must fail)
UIKIT = [
    ('update-first-responder-ungated',
     [('src/window/lifecycle.rs', 'if state.closed || state.modal_holds_responder() {', 'if state.closed {')],
     't-u2-drawing-stays-off'),
    ('late-input-delivered',
     [('src/window/input_target.rs', '        if window.native_modal_drops_input() {\n            return None;\n        }\n', '')],
     'f5-late-text-dropped'),
    ('held-key-not-released',
     [('src/window/text_input/keys.rs', '            if press.route == Route::Gpui {\n                let keystroke = Keystroke {',
       '            if press.route == Route::Gpui && press.usage < 0 {\n                let keystroke = Keystroke {')],
     'f3-held-keyup-before-gate'),
    ('release-not-fenced',
     [('src/window/input_target.rs', 'let blocked = state.modal_blocks_input() || state.press_fence.blocks(usage);',
       'let blocked = state.modal_blocks_input();')],
     'f3-fenced-release-swallowed'),
    ('busy-presenter-unchecked',
     [('src/window/native_modal.rs', '} else if self.controller.presentedViewController().is_some() {',
       '} else if self.controller.presentedViewController().is_some() && false {')],
     'f2-presenter-busy-failed'),
    ('restore-before-release', HOST[0][1], 'f6-no-restore-before-release'),
    ('menu-enabled-and-runs-while-shut',
     [(GATE, '    !fenced_key && !shut_when_performed && !shut_when_delivered', '    true'),
      (GATE, '    !key_window_shut\n', '    true\n')],
     'f4-menu-shortcut-disabled-while-open'),
    ('menu-call-sites-ungated',
     [('src/platform/menus.rs', '        !crate::window::native_modal::command_runs(self.fenced, shut, false)\n', '        false\n'),
      ('src/platform/menus.rs', '            if !crate::window::native_modal::command_runs(false, false, shut) {\n', '            if false {\n'),
      ('src/platform/menus.rs', '    if !crate::window::native_modal::command_enabled(modal) {\n', '    if false {\n')],
     'f4-menu-shortcut-disabled-while-open'),
    ('os-dismissal-ignored',
     [('src/window/native_modal/panel.rs',
       'self.with_window(|window, session| window.native_modal_dismissed(session, reason));',
       'let _ = reason;')],
     'f2-outside-tap-dismissed'),
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
        args = ['cargo', 'test', '--locked', '--test', 'native_modal', '--', '--test-threads=2']
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
    script = str(ROOT / 'scripts' / 'check-native-search.py')
    common = ['--example', 'native_search_probe', '--test', 'NativeSearch/testProbe', '--prefix', 'PROBE']
    runs = [('baseline', [], None)] + UIKIT + [('restored', [], None)]
    only = os.environ.get('NATIVE_SEARCH_ONLY', '')
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
