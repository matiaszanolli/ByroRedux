fn main() {
    cxx_build::bridge("src/lib.rs")
        .file("cpp/native_utils.cpp")
        .std("c++17")
        .compile("byroredux_cxx");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cpp/native_utils.h");
    println!("cargo:rerun-if-changed=cpp/native_utils.cpp");
}
