//! Build script — generates `shaders/include/shader_constants.glsl` from
//! `src/shader_constants_data.rs` (the single source of truth).
//!
//! Every GLSL shader that needs these constants should add:
//!   `#include "include/shader_constants.glsl"`
//!
//! and be compiled with (run from `crates/renderer/shaders/`; `-I<dir>` must
//! have no space before the path, and glslang writes its own default output
//! name unless `-o` is given — #4051, both defects reproduced against
//! glslang 11:16.2.0):
//!   glslangValidator -V -I. <shader.glsl> -o <shader.glsl>.spv

use std::path::Path;

// `shader_constants_data.rs` is included in both this build script and the
// renderer crate. The library derives vertex word offsets from its real
// `crate::Vertex`; this layout mirror gives the build script the same
// `offset_of!` expressions without pulling the renderer (or ash) into itself.
#[repr(C)]
#[allow(dead_code)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 4],
    normal: [f32; 3],
    uv: [f32; 2],
    bone_indices: [u32; 4],
    bone_weights: [f32; 4],
    splat_weights_0: [u8; 4],
    splat_weights_1: [u8; 4],
    tangent: [f32; 4],
}

// Pull the same constants that shader_constants.rs uses. Because build.rs
// runs in a separate compilation context it cannot import from the crate, so
// we share the raw data file via include!.
include!("src/shader_constants_data.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/shader_constants_data.rs");
    // #4490 — the .spv files are include_bytes!-tracked, but editing a .glsl
    // source alone recompiles nothing (cargo only sees the generated header
    // and the .spv bytes), which is the stale-SPIR-V trap documented in
    // docs/engine/skyal.md §4. Pin the whole shader tree so an edited source
    // or a recompiled .spv reliably dirties this crate. Smoke scripts were
    // separately instructed to `touch crates/renderer/src/lib.rs` after
    // shader edits; this is the structural half of the same fix.
    println!("cargo:rerun-if-changed=shaders");

    // #5097 — the header is declared once, as the SHADER_DEFINES table in
    // the data file; this used to be a 1415-line writeln! body that
    // hand-retyped every constant's name and format a second (and via the
    // pin test, a third) time.
    let out = render_shader_constants_header();

    let out_path = Path::new("shaders/include/shader_constants.glsl");
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).expect("failed to create shaders/include/");
    }

    let current = std::fs::read_to_string(out_path).unwrap_or_default();
    if current != out {
        std::fs::write(out_path, &out).expect("failed to write shader_constants.glsl");
        println!("cargo:warning=Regenerated shaders/include/shader_constants.glsl — recompile affected GLSL shaders.");
    }
}
