# Deferred validation

- Captured native query helpers need exact-head iOS validation. Reason: the
  source-only P13 stage has no native lease and its fixtures/mutations are UNRUN.
  First step: review the helper commit, then under a fresh exclusive mini lease
  execute `docs/native-query-probe.md` and the real consumer Cut/Escape/D6 cases.
  Close when actual selectors, nonmutation, nil/UTF-16/old-client observations,
  named negatives and exact restores are recorded; remove in the proof commit.

- SH1-156 native composition retirement remains experimental. Reason: the combined SDK8/SDK9 correction needs exact-head
  iOS validation and UIKit delivery evidence; injected native selectors do not prove
  OS cancellation. First step: obtain the exclusive Mac lease, then run the commands
  in the task's stage-ready artifact (device/simulator clippy, all-feature SDK tests,
  fix-removal negatives and the UIApplicationMain public-API probe), and execute the receiving matrix in
  `docs/composition-retirement.md`. Close only when old results never reach the new
  client and fresh English/software/IME-first hardware input arrives once without
  a tap, loss, replay or reorder, including focus/reconnect/Detach and observer/AX
  behavior. Resolve `NativeCallbackInFlight` call sites explicitly; do not turn a
  refusal into success. Remove this item in the commit recording those proofs.
