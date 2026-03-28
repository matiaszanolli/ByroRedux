# C++ Interop

The `cxx-bridge` crate provides type-safe FFI between Rust and C++ via
the `cxx` crate. This is for performance-critical code or legacy library
integration.

Source: `crates/cxx-bridge/`

## Structure

```
cxx-bridge/
├── Cargo.toml
├── build.rs          cxx-build compilation
├── src/
│   └── lib.rs        #[cxx::bridge] module
└── cpp/
    ├── native_utils.h
    └── native_utils.cpp
```

## Bridge Definition

```rust
#[cxx::bridge(namespace = "gamebyro")]
pub mod ffi {
    struct EngineInfo {
        name: String,
        version_major: u32,
        version_minor: u32,
        version_patch: u32,
    }

    extern "Rust" {
        fn engine_info() -> EngineInfo;
    }

    unsafe extern "C++" {
        include!("gamebyro-cxx-bridge/cpp/native_utils.h");
        fn native_hello() -> String;
    }
}
```

## Current State

The bridge is minimal — a proof-of-concept demonstrating:
- Shared structs across the FFI boundary (`EngineInfo`)
- Rust functions callable from C++ (`engine_info()`)
- C++ functions callable from Rust (`native_hello()`)

The binary crate calls `native_hello()` at startup to verify the bridge
is linked correctly.

## Build Configuration

`build.rs` compiles the C++ sources with C++17:

```rust
cxx_build::bridge("src/lib.rs")
    .file("cpp/native_utils.cpp")
    .std("c++17")
    .compile("gamebyro_cxx");
```

## Future Use

The C++ bridge will be used for:
- Integration with native libraries (physics, audio)
- Performance-critical math or data processing
- Any legacy code that's cheaper to wrap than rewrite
