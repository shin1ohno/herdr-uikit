# Deferred validation

- SH1-156 native composition retirement remains experimental. Reason: no exact-head
  iOS build or UIKit delivery evidence yet; injected native selectors do not prove
  OS cancellation. First step: obtain the exclusive Mac lease, then run the commands
  in the task's stage-ready artifact (device/simulator clippy, all-feature SDK tests,
  fix-removal negatives), and execute the receiving matrix in
  `docs/composition-retirement.md`. Close only when old results never reach the new
  client and fresh English/software/IME-first hardware input arrives once without
  a tap, loss, replay or reorder, including focus/reconnect/Detach and observer/AX
  behavior. Resolve `NativeCallbackInFlight` call sites explicitly; do not turn a
  refusal into success. Remove this item in the commit recording those proofs.
