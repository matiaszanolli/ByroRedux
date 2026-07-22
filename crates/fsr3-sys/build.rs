use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let crate_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let workspace = crate_dir
        .parent()
        .and_then(Path::parent)
        .expect("fsr3-sys must remain under <workspace>/crates");
    let vendor = workspace.join("third_party/fidelityfx-sdk-v1.1.4");
    let native = crate_dir.join("native");

    let sources = [
        "ffx-api/src/ffx_api.cpp",
        "ffx-api/src/ffx_provider_fsr3upscale.cpp",
        "ffx-api/src/backends.cpp",
        "ffx-api/src/validation.cpp",
        "sdk/src/shared/ffx_assert.cpp",
        "sdk/src/shared/ffx_message.cpp",
        "sdk/src/shared/ffx_object_management.cpp",
        "sdk/src/shared/ffx_breadcrumbs_list.cpp",
        "sdk/src/components/fsr3upscaler/ffx_fsr3upscaler.cpp",
        "sdk/src/backends/shared/ffx_shader_blobs.cpp",
        "sdk/src/backends/shared/blob_accessors/ffx_fsr3upscaler_shaderblobs.cpp",
        "sdk/src/backends/vk/ffx_vk.cpp",
    ];

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .warnings(false)
        .define("FFX_BACKEND_VK", None)
        .define("FFX_FSR3UPSCALER", None)
        .include(vendor.join("ffx-api/include"))
        .include(vendor.join("ffx-api/src"))
        .include(vendor.join("sdk/include"))
        .include(vendor.join("sdk/src/shared"))
        .include(vendor.join("sdk/src/components"))
        .include(vendor.join("sdk/src/backends/shared"))
        .include(vendor.join("generated-vk"))
        .include(&native)
        .file(native.join("byro_fsr3.cpp"))
        .file(native.join("upscaler_provider_registry.cpp"))
        .file(native.join("upscaler_only_stubs.cpp"));

    for source in sources {
        build.file(vendor.join(source));
    }

    if target_env != "msvc" {
        build.define("FFX_GCC", None);
        build.flag("-include").flag(
            native
                .join("byro_ffx_portability.h")
                .to_string_lossy()
                .as_ref(),
        );
    }

    if target_os != "windows" {
        // v1.1.4's source uses a small number of MSVC-compatible constructs
        // even in its Vulkan path. Clang supports those without changing AMD
        // source; the preincluded header supplies only secure-CRT shims absent
        // from libc.
        if env::var_os("CXX").is_none() {
            build.compiler("clang++");
        }
        build
            .flag("-fms-extensions")
            .flag("-fdeclspec")
            // FidelityFX's ABI is authored around the Windows 16-bit wchar_t
            // layout, including fixed-size opaque context storage.
            .flag("-fshort-wchar");
    }

    build.compile("byroredux_fsr3");

    if target_os == "windows" {
        println!("cargo:rustc-link-lib=vulkan-1");
        if target_env == "gnu" {
            println!("cargo:rustc-link-lib=m");
        }
    } else {
        println!("cargo:rustc-link-lib=vulkan");
    }

    println!("cargo:rerun-if-changed={}", native.display());
    println!("cargo:rerun-if-changed={}", vendor.display());
}
