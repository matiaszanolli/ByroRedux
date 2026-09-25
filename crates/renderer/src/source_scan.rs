//! Helpers for the crate's source-scan tests — tests that `include_str!` a
//! source file and assert on its text because the property they pin (an
//! ordering, a gate, a teardown sequence) needs a device to exercise.

/// A source file's production text: everything before its first
/// `#[cfg(test)] mod`.
///
/// `include_str!` brings in the file's test modules too, and those spell out
/// every needle the tests search for. A scan over the whole text is therefore
/// satisfied by the test's own literals whether or not the production code it
/// guards still exists — the defect class of #3442, #4604 and #4842. Scan this
/// instead.
///
/// The cut matches `#[cfg(test)]` followed by `mod`, so a `#[cfg(test)] use`
/// import earlier in the file does not truncate production code. A file whose
/// test modules are interleaved with production code (`context/draw.rs`) is not
/// served by this — a needle that lives after the first test module fails the
/// scan loudly rather than passing — and slices the function it guards
/// instead.
pub(crate) fn production_text(src: &str) -> &str {
    src.split_once("\n#[cfg(test)]\nmod ")
        .expect("source has no `#[cfg(test)] mod` — nothing to cut, so it is not a self-scan")
        .0
}

#[cfg(test)]
mod tests {
    use super::production_text;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    #[test]
    fn cuts_at_the_first_test_module_and_not_at_a_test_only_import() {
        let src = "fn a() {}\n#[cfg(test)]\nuse x;\nfn b() {}\n#[cfg(test)]\nmod tests {\n    needle\n}\n";
        let production = production_text(src);
        assert_eq!(production, "fn a() {}\n#[cfg(test)]\nuse x;\nfn b() {}");
        assert!(!production.contains("needle"));
    }

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
        ("vulkan/buffer.rs", 4),
        ("vulkan/caustic.rs", 4),
        ("vulkan/context/assemble_camera_and_lights.rs", 1),
        ("vulkan/context/build_and_upload_instances.rs", 6),
        ("vulkan/context/dispatch_skin_and_cluster.rs", 4),
        ("vulkan/context/draw.rs", 2),
        ("vulkan/context/geometry_pass.rs", 1),
        ("vulkan/context/mod.rs", 1),
        ("vulkan/context/post_passes.rs", 6),
        ("vulkan/context/resize.rs", 1),
        ("vulkan/context/resources.rs", 5),
        ("vulkan/context/skinned_blas_refit.rs", 2),
        ("vulkan/context/teardown.rs", 1),
        ("vulkan/device.rs", 2),
        ("vulkan/egui_pass.rs", 1),
        ("vulkan/frame_upscaler.rs", 5),
        ("vulkan/gpu_timers.rs", 2),
        ("vulkan/groundcover.rs", 3),
        ("vulkan/image.rs", 1),
        ("vulkan/material_tests.rs", 1),
        ("vulkan/pipeline.rs", 1),
        ("vulkan/presentation.rs", 2),
        ("vulkan/scene_buffer/upload.rs", 1),
        ("vulkan/skin_compute.rs", 2),
        ("vulkan/sky_cube.rs", 2),
        ("vulkan/svgf.rs", 4),
        ("vulkan/sync.rs", 2),
        ("vulkan/taa.rs", 1),
        ("vulkan/texture.rs", 3),
        ("vulkan/water.rs", 2),
    ];

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("readable source directory") {
            let path = entry.expect("readable directory entry").path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }

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
