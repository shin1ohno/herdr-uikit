#!/usr/bin/env bash
# Every internal native acquisition/resignation must pass the production guard.
set -euo pipefail
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
base=e8eb490701c0521194eeea8fd62c4073316470ee
pattern='\.(becomeFirstResponder|resignFirstResponder)\(\)'
baseline=$(git -C "$root" grep -nE "$pattern" "$base" -- src/window)
if [[ $(printf '%s\n' "$baseline" | wc -l | tr -d ' ') != 5 ]]; then
  echo 'FAIL: known baseline must contain five native responder calls' >&2
  exit 1
fi
current=$(git -C "$root" grep -nE "$pattern" -- src/window ':!src/window/edit_menu/responder_switch.rs' || true)
if [[ $(printf '%s\n' "$current" | wc -l | tr -d ' ') != 1 ]] ||
   [[ ! $current =~ ^src/window/lifecycle.rs:[0-9]+:.*text_input_view.resignFirstResponder\(\) ]]; then
  printf 'FAIL: only close may bypass ResponderSwitch\n%s\n' "$current" >&2
  exit 1
fi
printf 'PASS: baseline five calls; current close-only call\n%s\n' "$current"
