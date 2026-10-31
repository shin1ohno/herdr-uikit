# Third-party code in herdr-uikit

The Metal renderer (`src/renderer`, `build.rs`, and the shader source), the
Core Text text system (`src/text_system`), the GCD dispatcher
(`src/dispatcher.rs`), and parts of the window and platform code are adapted
from GPUI's macOS platform crates, published as `gpui-pre-apple` and
`gpui-pre-macos` 0.3.6, which are a snapshot of Zed at commit `bcf6582`
(<https://github.com/zed-industries/zed>).

GPUI is Copyright 2022 - 2025 Zed Industries, Inc. and licensed under the
Apache License, Version 2.0. Changes from upstream replace AppKit with UIKit,
`CVDisplayLink` with `CADisplayLink`, and adapt the code to this repository's
error-handling, file-size, and `unsafe` rules.

The iOS renderer omits Core Video surfaces: the pinned GPUI exposes the
`PaintSurface` image buffer only on macOS. Unsupported surfaces are skipped
with a one-time warning.
