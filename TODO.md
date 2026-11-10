# TODO

- Complete native edit origin validation on the exact PR head.
  - Reason: the Mac mini fell below the required 6 GiB free-disk guard after
    device clippy. Simulator clippy and the full simulator suite were not run;
    mutation sources are restored, but the restored positive suite still needs
    another run. No build or cache cleanup is authorized below the guard.
  - First step: verify at least 6 GiB free with `df -g /System/Volumes/Data`,
    then use `zsh -il`, `CARGO_TARGET_DIR=/Users/shin1ohno/ManagedProjects/yebisu/target`,
    and `IPHONEOS_DEPLOYMENT_TARGET=17.0` to run the README clippy/test gates,
    including `cargo test --locked --all-features --target aarch64-apple-ios-sim`
    with `CARGO_TARGET_AARCH64_APPLE_IOS_SIM_RUNNER=scripts/sim-runner.sh`.
  - Close condition: the restored seven `native_origin` tests, both iOS clippy
    gates, host checks, and the full simulator suite exit 0 on the exact PR
    head; retain the mutation failures as evidence, then remove this entry.
