//! C++ interop layer via the `cxx` crate.
//!
//! This crate houses all `#[cxx::bridge]` modules that define the Rust ↔ C++
//! FFI boundary. Each bridge module maps to a corresponding C++ source file
//! under `cpp/`.

#[cxx::bridge(namespace = "byroredux")]
pub mod ffi {
    unsafe extern "C++" {
        include!("byroredux-cxx-bridge/cpp/native_utils.h");

        /// Placeholder: returns a greeting from the C++ side.
        fn native_hello() -> String;
    }
}

#[cfg(test)]
mod tests {
    use super::ffi;

    #[test]
    fn native_hello_returns_greeting() {
        let msg = ffi::native_hello();
        assert!(!msg.is_empty(), "native_hello() returned an empty string");
    }
}
