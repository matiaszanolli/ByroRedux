//! Helpers for the crate's source-scan tests — tests that `include_str!` a
//! source file and assert on its text because the property they pin (an
//! ordering, a gate, a teardown sequence) needs a device to exercise.
//!
//! The helpers themselves live in `byroredux-core::source_scan` (#5100):
//! this module re-exports them so the crate's scans keep one import path
//! and the cut stays defined exactly once.

pub(crate) use byroredux_core::source_scan::production_text;
pub(crate) use byroredux_core::source_scan::rust_files;
pub(crate) use byroredux_core::source_scan::strip_test_modules;

#[cfg(test)]
mod tests {
    use super::rust_files;
    use std::collections::BTreeMap;
    use std::path::Path;

    /// Per file, the number of `include_str!` calls that read the file they sit
    /// in without going through [`production_text`]. Each is a scan over text
    /// that includes the file's own test modules, so each must be safe for a
    /// reason of its own: the needle is composed at run time, the scan is cut or
    /// bounded to a function body, it is a negative or doc-claim scan, or the
    /// file's test modules are interleaved with production code so the cut does
    /// not apply (`context/draw.rs`, `scene_buffer/upload.rs`). The sites here
    /// were reviewed one by one in the sweep that introduced `production_text`.
    const UNWRAPPED_SELF_INCLUDES: &[(&str, usize)] = &[
        ("texture_registry/mod.rs", 1),
        ("vulkan/bloom.rs", 3),
        ("vulkan/buffer.rs", 3),
        ("vulkan/caustic.rs", 4),
        ("vulkan/context/assemble_camera_and_lights.rs", 1),
        // +1 (#4722): `ui_instance_idx_is_reclamped_to_the_post_grow_slot_
        // capacity` cuts at its own module start (`mod ui_instance_idx_
        // overflow_tests`), same as the sibling #3601 pin in that module —
        // the file's test modules are interleaved with production code, so
        // production_text's first-cut does not apply.
        ("vulkan/context/build_and_upload_instances.rs", 7),
        ("vulkan/context/dispatch_skin_and_cluster.rs", 4),
        // +1 (#5087): the file-level LOC budget strips every test module
        // itself (`production_lines` in draw_frame_size_budget_tests), so
        // production_text's first-cut does not apply to this interleaved
        // file.
        ("vulkan/context/draw.rs", 3),
        ("vulkan/context/geometry_pass.rs", 1),
        ("vulkan/context/mod.rs", 1),
        // +1 (#4958): `every_frame_recorder_is_documented` strips every test
        // module itself (`strip_test_modules`), so its self-read is cut.
        ("vulkan/context/post_passes.rs", 7),
        ("vulkan/context/resize.rs", 1),
        ("vulkan/context/resources.rs", 5),
        ("vulkan/context/skinned_blas_refit.rs", 2),
        ("vulkan/context/teardown.rs", 1),
        ("vulkan/device.rs", 2),
        ("vulkan/egui_pass.rs", 1),
        // frame_upscaler.rs dropped off this list entirely (#5100): all four
        // of its self-scans now go through `production_text`.
        ("vulkan/gpu_timers.rs", 2),
        ("vulkan/image.rs", 1),
        ("vulkan/material_tests.rs", 1),
        ("vulkan/pipeline.rs", 1),
        ("vulkan/presentation.rs", 2),
        ("vulkan/scene_buffer/upload.rs", 1),
        ("vulkan/skin_compute.rs", 2),
        ("vulkan/sky_cube.rs", 2),
        ("vulkan/svgf.rs", 4),
        ("vulkan/sync.rs", 1),
        ("vulkan/taa.rs", 1),
        ("vulkan/texture.rs", 2),
        // water.rs 2→1 (#5100): the pass-bind-hoist scan now goes through
        // `production_text`; the remaining self-read is the blend-table /
        // module-doc pin, a whole-file doc audit by design.
        ("vulkan/water.rs", 1),
    ];

    /// The count of unwrapped self-including `include_str!` calls in `text`,
    /// which is the source of `file`.
    fn unwrapped_self_includes(file: &Path, text: &str) -> usize {
        // Composed so this file does not contain the needle it searches for.
        let needle = ["include_str!(", "\""].concat();
        let canonical = file.canonicalize().expect("source file resolves");
        let dir = file.parent().expect("source file has a directory");
        let mut count = 0;
        for (at, _) in text.match_indices(needle.as_str()) {
            let arg = &text[at + needle.len()..];
            let Some(end) = arg.find('"') else { continue };
            let literal = &arg[..end];
            if literal.contains('\\') || !arg[end + 1..].starts_with(')') {
                continue;
            }
            // Not a path that exists (a shader, a doc): not a self-scan.
            let Ok(target) = dir.join(literal).canonicalize() else {
                continue;
            };
            if target != canonical {
                continue;
            }
            if !text[..at].trim_end().ends_with("production_text(") {
                count += 1;
            }
        }
        count
    }

    /// A test that `include_str!`s the file it guards also sees that file's test
    /// modules, which spell out every needle the tests search for — so a scan
    /// over the whole text is satisfied by the test's own literals whether or
    /// not the production code it guards survives (#3442, #4604, #4842, and the
    /// ~30 sites the `production_text` sweep found).
    ///
    /// This pins the count of such scans that do NOT go through
    /// [`production_text`], per file, so a new one is a deliberate decision
    /// rather than an accident. Read `production_text(include_str!(..))`
    /// instead; if the file's test modules are interleaved with production code,
    /// or the scan really is a whole-file doc audit, bump the count in
    /// [`UNWRAPPED_SELF_INCLUDES`] and say why in the commit. Composing the
    /// needle at run time, or cutting/bounding the text inline, are also sound
    /// but only when the mutation check shows the test fails with the guarded
    /// code removed.
    #[test]
    fn a_new_self_scan_goes_through_production_text_or_is_reviewed_into_the_baseline() {
        let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src_root, &mut files);

        let mut found: BTreeMap<String, usize> = BTreeMap::new();
        for file in &files {
            let text = std::fs::read_to_string(file).expect("readable source file");
            let count = unwrapped_self_includes(file, &text);
            if count > 0 {
                let relative = file
                    .strip_prefix(&src_root)
                    .expect("file is under src")
                    .to_string_lossy()
                    .replace('\\', "/");
                found.insert(relative, count);
            }
        }

        let expected: BTreeMap<String, usize> = UNWRAPPED_SELF_INCLUDES
            .iter()
            .map(|&(file, count)| (file.to_string(), count))
            .collect();

        let mut problems = Vec::new();
        for file in found.keys().chain(expected.keys()).collect::<std::collections::BTreeSet<_>>() {
            let now = found.get(file).copied().unwrap_or(0);
            let baseline = expected.get(file).copied().unwrap_or(0);
            if now > baseline {
                problems.push(format!(
                    "{file}: {now} self-including include_str! not wrapped in production_text, \
                     baseline {baseline} — scan `crate::source_scan::production_text(include_str!(..))` \
                     instead, or review the new site and raise the baseline"
                ));
            } else if now < baseline {
                problems.push(format!(
                    "{file}: {now} unwrapped self-include(s), baseline {baseline} — lower the \
                     baseline so the ratchet keeps tightening"
                ));
            }
        }
        assert!(problems.is_empty(), "\n{}", problems.join("\n"));
    }
}
