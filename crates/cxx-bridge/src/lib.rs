//! C++ interop layer via the `cxx` crate.
//!
//! This crate houses all `#[cxx::bridge]` modules that define the Rust ↔ C++
//! FFI boundary. Each bridge module maps to a corresponding C++ source file
//! under `cpp/`.

#[cxx::bridge(namespace = "byroredux")]
pub mod ffi {
    /// Example struct shared across the FFI boundary.
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
        include!("byroredux-cxx-bridge/cpp/native_utils.h");

        /// Placeholder: returns a greeting from the C++ side.
        fn native_hello() -> String;
    }
}

fn engine_info() -> ffi::EngineInfo {
    ffi::EngineInfo {
        name: "ByroRedux".into(),
        version_major: 0,
        version_minor: 1,
        version_patch: 0,
    }
}
