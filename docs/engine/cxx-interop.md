# C++ Interop

The `cxx-bridge` crate provides type-safe FFI between Rust and C++ via
the `cxx` crate. This is for performance-critical code or legacy library
integration.

Source: `crates/cxx-bridge/`

> **Status (verified 2026-08-25):** Still a proof-of-concept. No production C++ has
> been bridged yet. The current renderer/physics/audio stack is pure Rust
> (Vulkan via `ash`, audio via `kira`); no native library is wrapped through
> this crate today. The unused Rust→C++ `EngineInfo` export was removed by
> #2426; a smoke test now pins the remaining C++→Rust greeting call.

## Structure

```
cxx-bridge/
├── Cargo.toml         depends on cxx (lib) + cxx-build (build)
├── build.rs           cxx-build compilation
├── src/
│   └── lib.rs         #[cxx::bridge] module
└── cpp/
    ├── native_utils.h
    └── native_utils.cpp
```

The crate is named `byroredux-cxx-bridge` (package name) and is referenced
as `byroredux_cxx_bridge` (crate path) from the binary. It is registered as a
workspace member in the root `Cargo.toml` and exposed as a workspace
dependency (`byroredux-cxx-bridge = { path = "crates/cxx-bridge" }`).

## Bridge Definition

The `#[cxx::bridge]` module lives in `crates/cxx-bridge/src/lib.rs` under the
`byroredux` namespace:

```rust
#[cxx::bridge(namespace = "byroredux")]
pub mod ffi {
    unsafe extern "C++" {
        include!("byroredux-cxx-bridge/cpp/native_utils.h");

        /// Placeholder: returns a greeting from the C++ side.
        fn native_hello() -> String;
    }
}
```

The C++ side (`crates/cxx-bridge/cpp/`) is equally small:

```cpp
// native_utils.h
#pragma once
#include "rust/cxx.h"

namespace byroredux {
rust::String native_hello();
} // namespace byroredux

// native_utils.cpp
#include "byroredux-cxx-bridge/cpp/native_utils.h"

namespace byroredux {
rust::String native_hello() {
    return rust::String("Hello from C++ side of ByroRedux!");
}
} // namespace byroredux
```

## Current State

The bridge is now deliberately minimal: it demonstrates only **C++ functions
callable from Rust** — `native_hello()`, declared in
  `unsafe extern "C++"` and implemented in `native_utils.cpp`.

The binary crate calls `native_hello()` at startup to verify the bridge is
linked correctly. In `byroredux/src/main.rs`, immediately after the
`"ByroRedux starting"` log line, it logs the C++ greeting:

```rust
log::info!("ByroRedux starting");
log::info!("{}", byroredux_cxx_bridge::ffi::native_hello());
```

This is the only production call into the bridge. The crate-local
`native_hello_returns_greeting` test invokes the same function and rejects an
empty result, so linkage is covered by `cargo test --workspace`.

## Build Configuration

`crates/cxx-bridge/Cargo.toml` pulls `cxx` as a normal dependency and
`cxx-build` as a build-dependency (both via the workspace), and `build.rs`
compiles the C++ sources with C++17:

```rust
cxx_build::bridge("src/lib.rs")
    .file("cpp/native_utils.cpp")
    .std("c++17")
    .compile("byroredux_cxx");

println!("cargo:rerun-if-changed=src/lib.rs");
println!("cargo:rerun-if-changed=cpp/native_utils.h");
println!("cargo:rerun-if-changed=cpp/native_utils.cpp");
```

The `rerun-if-changed` lines re-trigger the C++ build only when the bridge
module or the C++ sources change.

## Future Use

The C++ bridge will be used for:
- Integration with native libraries (e.g. physics, audio) that are cheaper
  to wrap than to reimplement.
- Performance-critical math or data processing.
- Any legacy code that's cheaper to wrap than rewrite.

As of 2026-08-25 none of this has materialised — the audio (`kira`) and
physics work to date stayed in Rust, so the bridge remains scaffolding. The
proprietary-dependency policy (parse data, never link the vendor libs —
Havok, SpeedTree, Scaleform, FaceGen) also limits what would ever be bridged
here: this crate is for our own native code, not for linking shipped game
binaries.
