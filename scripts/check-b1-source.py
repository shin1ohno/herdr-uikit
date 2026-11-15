#!/usr/bin/env python3
"""ONE bounded extracted-source batch. No Cargo/product/native compilation.

Copies selected production modules into a new external evidence directory.
External SDK/ObjC imports are replaced with explicit fixture types. This tests
Rust scheduling/ownership source and raw adapter bodies, not UIKit/GPUI behavior.
Mutants affect ONLY the extracted copies; every run logs input hashes/output.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / 'scripts/b1-source-fixture'
FILES = [
    'src/native_operation.rs', 'src/native_operation/state.rs',
    'src/native_operation/tests.rs', 'src/native_operation/state/tests.rs',
    'src/native_retirement_request.rs', 'src/text_input_retirement.rs',
    'src/gpui_call.rs', 'src/gpui_call/tests.rs', 'src/platform/slot.rs',
    'src/platform/slot/tests.rs', 'src/platform/slot/tests/native_retirement.rs',
    'src/window/client_lifetime.rs', 'src/window/native_retirement_request.rs',
    'src/window/native_retirement_request/coordinator.rs',
    'src/window/native_retirement_request/transaction.rs', 'src/window/retirement.rs',
    'src/native_operation/entry_guard_tests.rs', 'src/window/state/press_ledger.rs',
]
INCLUDE_STR = re.compile(r'include_str!\("([^"]+)"\)')


def digest(data):
    return hashlib.sha256(data).hexdigest()


def body(source, anchor):
    """Brace-balanced extraction of the exact three known raw closure bodies."""
    if source.count(anchor) != 1:
        raise ValueError(f'non-unique raw anchor: {anchor}')
    start = source.index(anchor) + len(anchor)
    level = 1
    for i in range(start, len(source)):
        level += (source[i] == '{') - (source[i] == '}')
        if level == 0:
            return source[start:i]
    raise ValueError('unclosed raw closure')


def include_targets():
    """Files the compiled modules read with include_str!, relative to ROOT.

    They are read, not compiled, so the source guards that read them must see
    the production bytes: they are copied verbatim, never rewritten.
    """
    targets = []
    for relative in FILES:
        for target in INCLUDE_STR.findall((ROOT / relative).read_text()):
            resolved = str((ROOT / relative).parent.joinpath(target).resolve().relative_to(ROOT))
            if resolved in FILES:
                raise ValueError(f'{resolved} is both compiled and read by include_str!')
            if resolved not in targets:
                targets.append(resolved)
    return targets


def extract(destination):
    hashes = {}
    for relative in FILES:
        data = (ROOT / relative).read_bytes()
        hashes[relative] = digest(data)
        text = data.decode().replace('use gpui::AnyWindowHandle;', 'use crate::AnyWindowHandle;')
        text = text.replace('use objc2::MainThreadMarker;', 'use crate::MainThreadMarker;')
        text = text.replace('use objc2::rc::Retained;', 'use std::rc::Rc as Retained;')
        text = text.replace(', thiserror::Error', '')
        text = re.sub(r'^\s*#\[error\([^\n]*\)\]\n', '\n', text, flags=re.M)
        if relative.endswith('client_lifetime.rs'):
            # Construct an exhausted token without changing production visibility.
            text += '\npub(crate) fn fixture(identity: u64) -> Rc<ClientLifetime> { Rc::new(ClientLifetime { identity, ..ClientLifetime::default() }) }\n'
        target = destination / relative.removeprefix('src/')
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text)
    for relative in include_targets():
        hashes[relative] = digest((ROOT / relative).read_bytes())
        target = destination / relative.removeprefix('src/')
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / relative, target)
    for name in ['main.rs', 'window.rs', 'cases.rs']:
        hashes[f'scripts/b1-source-fixture/{name}'] = digest((FIXTURE / name).read_bytes())
        shutil.copyfile(FIXTURE / name, destination / name)
    raw_sources = {name: (ROOT / name).read_text() for name in ['src/platform/shared.rs', 'src/window/scene.rs']}
    hashes.update({name: digest(text.encode()) for name, text in raw_sources.items()})
    shared = body(raw_sources['src/platform/shared.rs'], 'DispatchQueue::main().exec_async(move || {')
    shared = shared.replace('tracing::warn!("a platform callback was queued before the platform ran")', 'missing_shared()')
    scene = raw_sources['src/window/scene.rs']
    destroy = body(scene[scene.index('pub(super) fn destroy_unwanted('):scene.index('fn scene_request_refused(')], 'DispatchQueue::main().exec_async(move || {')
    refusal = body(scene[scene.index('pub(super) fn new_scene_refused('):], 'DispatchQueue::main().exec_async(move || {')
    raw = (FIXTURE / 'raw-template.rs').read_text()
    for key, value in [('SHARED_BODY', shared), ('DESTROY_BODY', destroy), ('REFUSAL_BODY', refusal)]:
        if raw.count(key) != 1:
            raise ValueError('fixture raw placeholder not unique')
        raw = raw.replace(key, value)
    (destination / 'raw_bodies.rs').write_text(raw)
    hashes['scripts/b1-source-fixture/raw-template.rs'] = digest((FIXTURE / 'raw-template.rs').read_bytes())
    return hashes


MUTATIONS = [
    ('prior-fence-lost', 'window/native_retirement_request/coordinator.rs',
     'old_fenced_at_submission: self.old_fenced_at_submission,',
     'old_fenced_at_submission: false,', 'queued-retry-preserves-existing-fence'),
    ('legacy-interrupt-lost', 'window/retirement.rs',
     '    super::legacy_interrupt_native_retirement();', '', 'new-terminal-precedes-legacy-drain'),
    ('legacy-hold-lost', 'gpui_call.rs',
     'is_running() || LEGACY_HOLD.get() || DRAINING.replace(true)',
     'is_running() || DRAINING.replace(true)', 'slot-release-exactly-once'),
    ('capacity-lost', 'window/native_retirement_request.rs',
     'if coordinator::busy() {', 'if false {', 'assertion failed: !s.completing && s.pending.is_none()'),
    ('wrong-window-accepted', 'native_retirement_request.rs',
     'self.client == other.client && self.window.ptr_eq(&other.window)',
     'self.client == other.client', 'window-identity-not-numeric-client'),
    ('slot-loan-lost', 'platform/slot.rs',
     '        let _callback_loan = crate::native_operation::loan();', '', 'callback-loan-through-restore'),
    ('raw-shared-lost', 'raw_bodies.rs',
     '        let _raw_later_entry = crate::native_operation::raw("shared-later");', '', 'raw-ingress-does-not-inherit-task'),
    ('raw-destroy-lost', 'raw_bodies.rs',
     '        let _raw_destroy_entry = crate::native_operation::raw("scene-destroy");', '', 'raw-ingress-does-not-inherit-task'),
    ('raw-refusal-lost', 'raw_bodies.rs',
     '            let _raw_refusal_entry = crate::native_operation::raw("scene-refusal");', '', 'raw-ingress-does-not-inherit-task'),
    ('snapshot-admitted', 'native_operation/state.rs',
     'Phase::EditDispatch | Phase::PasteFallback)',
     'Phase::EditDispatch | Phase::PasteFallback | Phase::ObserveSnapshot)', 'query-loan-is-not-admission'),
    ('quiescence-loan-lost', 'native_operation/state.rs',
     '            && s.loans == 0', '', 'native-prepare-no-borrow'),
] + [
    # Held-press ledger (plan-2 section 4.1). S1-S7: the std-only ledger and its
    # own tests, compiled as is.
    ('s1-began-ignored', 'window/state/press_ledger.rs',
     '            (PressPhase::Began | PressPhase::Changed, None) => {',
     '            (PressPhase::Began, None) => {}\n            (PressPhase::Changed, None) => {', 'keyed-began-holds'),
    ('s2-ended-ignored', 'window/state/press_ledger.rs',
     '            (PressPhase::Ended | PressPhase::Cancelled, Some(index)) => {',
     '            (PressPhase::Ended, Some(_)) => {}\n            (PressPhase::Cancelled, Some(index)) => {',
     'release-clears-usage'),
    ('s2-cancelled-ignored', 'window/state/press_ledger.rs',
     '            (PressPhase::Ended | PressPhase::Cancelled, Some(index)) => {',
     '            (PressPhase::Cancelled, Some(_)) => {}\n            (PressPhase::Ended, Some(index)) => {',
     'cancel-clears-usage'),
    ('s2-release-clears-all', 'window/state/press_ledger.rs',
     '                ledger.entries.remove(index);', '                let _ = index;\n                ledger.entries.clear();',
     'release-keeps-other-usage'),
    ('s3-duplicate-pushed', 'window/state/press_ledger.rs',
     '            (PressPhase::Began | PressPhase::Changed, Some(_)) => {}',
     '            (PressPhase::Began | PressPhase::Changed, Some(_)) => ledger.entries.push(Entry { id, since: now, _pin: fact.pin }),',
     'same-usage-dedup'),
    ('s4-unkeyed-began-ignored', 'window/state/press_ledger.rs',
     '        let id = fact.id();',
     '        let id = fact.id();\n        if matches!(id, Id::Identity(_)) && phase == PressPhase::Began { continue; }',
     'unkeyed-held'),
    ('s4-any-unkeyed-release', 'window/state/press_ledger.rs',
     '        let known = ledger.entries.iter().position(|entry| entry.id == id);',
     '        let known = ledger.entries.iter().position(|entry| entry.id == id || matches!((entry.id, id), (Id::Identity(_), Id::Identity(_))));',
     'unkeyed-identity'),
    ('s5-overflow-lost', 'window/state/press_ledger.rs',
     '                    ledger.overflow = true;', '', 'overflow-fail-closed'),
    ('s6-changed-ignored', 'window/state/press_ledger.rs',
     '            (PressPhase::Began | PressPhase::Changed, None) => {',
     '            (PressPhase::Changed, None) => {}\n            (PressPhase::Began, None) => {', 'changed-unseen-holds'),
] + [
    (f's7-witness-{name}-lost', 'window/state/press_ledger.rs',
     'self.presses != 0 || self.repeat || self.menu_press || self.modifiers', after, f'key-state-{name}-witness')
    for name, after in [
        ('presses', 'self.repeat || self.menu_press || self.modifiers'),
        ('repeat', 'self.presses != 0 || self.menu_press || self.modifiers'),
        ('menu', 'self.presses != 0 || self.repeat || self.modifiers'),
        ('modifiers', 'self.presses != 0 || self.repeat || self.menu_press'),
    ]
] + [
    # G2: the real hardware_held passes the text view's key state.
    ('g2-witness-call-dropped', 'window/native_retirement_request.rs',
     'press_ledger::held(&window.state.borrow().press_ledger, witness)',
     'press_ledger::held(&window.state.borrow().press_ledger, press_ledger::KeyWitness::default())',
     'request-key-state-presses'),
] + [
    # S8 / G2: the key state's mapping (read by a source guard).
    (f's8-witness-{name}-dropped', 'window/text_input/keys.rs', before, after, 'witness-maps-every-field')
    for name, before, after in [
        ('presses', 'presses: self.presses.len(),', 'presses: 0,'),
        ('repeat', 'repeat: self.repeat.is_some(),', 'repeat: false,'),
        ('menu', 'menu_press: self.menu_press.is_some(),', 'menu_press: false,'),
        ('modifiers', 'modifiers: self.held != HeldModifiers::default(),', 'modifiers: false,'),
    ]
] + [
    ('s8-text-view-witness-default', 'window/text_input/keys.rs',
     '        self.ivars().keys.borrow().witness()', '        KeyWitness::default()',
     'text-view-witness-is-its-key-state'),
] + [
    # S9: each selector's record line (read by a source guard): four removed,
    # four with the wrong phase.
    ('s9-text-pressesBegan-removed', 'window/text_input/view_class.rs',
     '            self.ivars().target.record_presses(PressPhase::Began, presses);\n', '',
     'pressesBegan must feed the ledger with PressPhase::Began'),
    ('s9-text-pressesChanged-phase', 'window/text_input/view_class.rs',
     '            self.ivars().target.record_presses(PressPhase::Changed, presses);\n', '            self.ivars().target.record_presses(PressPhase::Began, presses);\n',
     'pressesChanged must feed the ledger with PressPhase::Changed'),
    ('s9-text-pressesEnded-removed', 'window/text_input/view_class.rs',
     '            self.ivars().target.record_presses(PressPhase::Ended, presses);\n', '',
     'pressesEnded must feed the ledger with PressPhase::Ended'),
    ('s9-text-pressesCancelled-phase', 'window/text_input/view_class.rs',
     '            self.ivars().target.record_presses(PressPhase::Cancelled, presses);\n', '            self.ivars().target.record_presses(PressPhase::Ended, presses);\n',
     'pressesCancelled must feed the ledger with PressPhase::Cancelled'),
    ('s9-drawing-pressesBegan-phase', 'window/view.rs',
     '            if let Some(window) = self.owner() { window.record_presses(PressPhase::Began, presses); }\n', '            if let Some(window) = self.owner() { window.record_presses(PressPhase::Changed, presses); }\n',
     'pressesBegan must feed the ledger with PressPhase::Began'),
    ('s9-drawing-pressesChanged-removed', 'window/view.rs',
     '            if let Some(window) = self.owner() { window.record_presses(PressPhase::Changed, presses); }\n', '',
     'pressesChanged must feed the ledger with PressPhase::Changed'),
    ('s9-drawing-pressesEnded-phase', 'window/view.rs',
     '            if let Some(window) = self.owner() { window.record_presses(PressPhase::Ended, presses); }\n', '            if let Some(window) = self.owner() { window.record_presses(PressPhase::Cancelled, presses); }\n',
     'pressesEnded must feed the ledger with PressPhase::Ended'),
    ('s9-drawing-pressesCancelled-removed', 'window/view.rs',
     '            if let Some(window) = self.owner() { window.record_presses(PressPhase::Cancelled, presses); }\n', '',
     'pressesCancelled must feed the ledger with PressPhase::Cancelled'),
] + [
    # G1: the UIPress adapter bodies (read by a source guard).
    ('g1-adapter-usage-dropped', 'window/state.rs',
     '                    press.key(self.mtm).map(|key| key.keyCode().0),', '                    None,',
     'window-adapter-maps-usage-identity-phase'),
    ('g1-adapter-phase-ignored', 'window/state.rs',
     '            phase,\n            facts,', '            press_ledger::PressPhase::Began,\n            facts,',
     'window-adapter-maps-usage-identity-phase'),
    ('g1-text-adapter-phase-ignored', 'window/input_target.rs',
     '            window.record_presses(phase, presses);',
     '            window.record_presses(super::state::press_ledger::PressPhase::Began, presses);',
     'text-adapter-forwards-phase'),
    # S10 / S11 / G7: the request reads the ledger, not the history flag.
    ('s10-held-press-admitted', 'window/native_retirement_request.rs',
     '        if hardware_held(&window).is_some() {', '        if false {', 'held-press-refused'),
    ('s11-history-refuses', 'window/native_retirement_request.rs',
     '        if hardware_held(&window).is_some() {',
     '        if hardware_held(&window).is_some() || state.native_hardware_observed {',
     'released-hardware-admitted'),
    # S12: the press frame itself still refuses.
    ('s12-press-frame-admitted', 'native_operation/state.rs',
     '        if s.presses != 0 {', '        if false {', 'hardware-entry-never-admitted'),
]


def run_batch(out, only=None, test_filter=None):
    if not out.is_absolute() or out == ROOT or ROOT in out.parents:
        raise SystemExit('Use a new absolute task-owned evidence directory OUTSIDE source')
    out.mkdir(parents=True, exist_ok=False)
    baseline = out / 'extracted'
    baseline.mkdir()
    hashes = extract(baseline)
    manifest = {'scope': 'extracted Rust source with native/GPUI test doubles; NOT shipping/native/OS proof',
                'head': subprocess.check_output(['git', '-C', str(ROOT), 'rev-parse', 'HEAD'], text=True).strip(),
                'sources': hashes, 'mutations': [], 'runs': []}
    mutations = [m for m in MUTATIONS if not only or m[0] == only]
    if not mutations:
        raise ValueError('unknown mutation label')
    manifest['test_filter'] = test_filter
    for label, relative, before, after, assertion in mutations:
        source = (baseline / relative).read_text()
        if source.count(before) != 1:
            raise ValueError(f'{label}: source anchor count {source.count(before)}')
        manifest['mutations'].append({'label': label, 'file': relative, 'before': before, 'after': after,
                                     'assertion': assertion, 'mutant_sha256': digest(source.replace(before, after).encode())})
    manifest_path = out / 'manifest.json'
    manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')

    def execute(label, expected=None):
        binary = out / f'{label}.bin'
        command = ['rustc', '--edition=2024', '--test', str(baseline / 'main.rs'), '-o', str(binary)]
        compiled = subprocess.run(command, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=60)
        (out / f'{label}.compile.log').write_text(compiled.stdout)
        if compiled.returncode:
            manifest['runs'].append({'label': label, 'compile_returncode': compiled.returncode, 'valid': False})
            manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
            raise RuntimeError(f'{label}: compilation/setup failure; not an intended negative')
        test_command = [str(binary), '--test-threads=1', '--nocapture']
        if test_filter:
            test_command.extend([test_filter, '--exact'])
        result = subprocess.run(test_command, text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=30)
        (out / f'{label}.log').write_text(result.stdout)
        valid = result.returncode == 101 and expected in result.stdout if expected else result.returncode == 0 and '0 passed' not in result.stdout
        manifest['runs'].append({'label': label, 'command': command, 'returncode': result.returncode,
                                 'intended_assertion': expected, 'valid': valid})
        manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
        print(f'{label}: exit={result.returncode} intended={expected} valid={valid}', flush=True)
        if not valid:
            raise RuntimeError(f'{label}: required result absent; inspect preserved log')
    execute('baseline')
    for label, relative, before, after, assertion in mutations:
        path = baseline / relative
        original = path.read_bytes()
        try:
            path.write_text(original.decode().replace(before, after))
            execute(label, assertion)
        finally:
            path.write_bytes(original)
        assert digest(path.read_bytes()) == digest(original)
    execute('restored')
    manifest['source_unchanged'] = all(digest((ROOT / name).read_bytes()) == value for name, value in hashes.items())
    manifest_path.write_text(json.dumps(manifest, indent=2) + '\n')
    assert manifest['source_unchanged']


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--only', choices=[m[0] for m in MUTATIONS])
    parser.add_argument('--test-filter')
    args = parser.parse_args()
    run_batch(args.output, args.only, args.test_filter)
