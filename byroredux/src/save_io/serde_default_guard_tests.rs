//! Save-schema source guard tests.
//!
//! The set of scanned files is derived from `build_save_registry` and the
//! workspace's feature-gated save derives. This deliberately avoids another
//! hand-maintained shadow registry: moving or registering a type changes the
//! scan automatically.

use std::path::{Path, PathBuf};

fn rust_sources_below(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources_below(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn registered_type_names(registry_source: &str) -> Vec<&str> {
    [
        ".register_component::<",
        ".register_resource::<",
        ".register_form_id_component::<",
    ]
    .into_iter()
    .flat_map(|prefix| {
        registry_source
            .match_indices(prefix)
            .filter_map(move |(start, _)| {
                let rest = &registry_source[start + prefix.len()..];
                let end = rest.find('>')?;
                rest[..end].rsplit("::").next()
            })
    })
    .collect()
}

fn defines_type(source: &str, type_name: &str) -> bool {
    ["struct ", "enum ", "type "]
        .into_iter()
        .any(|kind| source.contains(&format!("{kind}{type_name}")))
}

fn save_type_sources() -> Vec<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let registry_source = include_str!("../save_io.rs");
    let registered = registered_type_names(registry_source);
    let mut candidates = Vec::new();
    for root in [
        manifest.join("src"),
        manifest.join("../crates/core/src"),
        manifest.join("../crates/plugin/src"),
        manifest.join("../crates/scripting/src"),
    ] {
        rust_sources_below(&root, &mut candidates);
    }

    candidates.retain(|path| {
        let Ok(source) = std::fs::read_to_string(path) else {
            return false;
        };
        source.contains("cfg_attr(feature = \"save\"")
            || registered.iter().any(|name| defines_type(&source, name))
    });
    candidates.sort();
    candidates.dedup();
    candidates
}

/// Return the body of the first serde attribute on this line, supporting
/// both `#[serde(...)]` and `#[cfg_attr(..., serde(...))]`.
fn serde_attribute_body(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("#[") {
        return None;
    }
    let serde = trimmed.find("serde(")? + "serde(".len();
    let bytes = trimmed.as_bytes();
    let mut depth = 1usize;
    let mut in_string = false;
    let mut escaped = false;
    for i in serde..bytes.len() {
        let byte = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&trimmed[serde..i]);
                }
            }
            _ => {}
        }
    }
    None
}

fn serde_attr_declares_unsafe_default(line: &str) -> bool {
    let Some(body) = serde_attribute_body(line) else {
        return false;
    };
    let mut in_string = false;
    let keys: Vec<&str> = body
        .split(|c| {
            if c == '"' {
                in_string = !in_string;
            }
            c == ',' && !in_string
        })
        .map(|entry| entry.split('=').next().unwrap_or("").trim())
        .collect();
    // `skip` fields do not exist in the on-disk shape. Serde requires a
    // construction default for them, but that cannot mask schema drift.
    keys.iter().any(|key| *key == "default") && !keys.iter().any(|key| *key == "skip")
}

#[test]
fn serde_default_on_saved_struct_requires_format_major_bump() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();
    for path in save_type_sources() {
        let src = std::fs::read_to_string(&path).unwrap();
        for (i, line) in src.lines().enumerate() {
            if serde_attr_declares_unsafe_default(line) {
                let relative = path.strip_prefix(manifest).unwrap_or(&path);
                offenders.push(format!("{}:{}", relative.display(), i + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "SAVE-D2-01 (#1714): serde(default) on serialized save data masks an \
         intra-type change. Bump byroredux_save::FORMAT_MAJOR and remove the \
         compatibility default. Offenders: {offenders:?}",
    );
}

#[test]
fn source_discovery_follows_registry_and_nested_save_modules() {
    let sources = save_type_sources();
    assert!(sources.iter().any(|path| path.ends_with("save_io.rs")));
    assert!(sources
        .iter()
        .any(|path| path.ends_with("scene/quest_alias.rs")));
    assert!(!sources.iter().any(|path| path.ends_with("settings_io.rs")));
}

#[test]
fn serde_guard_handles_bare_and_cfg_attr_forms() {
    assert!(serde_attr_declares_unsafe_default("#[serde(default)]"));
    assert!(serde_attr_declares_unsafe_default(
        "#[cfg_attr(feature = \"save\", serde(default))]"
    ));
    assert!(serde_attr_declares_unsafe_default(
        r#"#[cfg_attr(feature = "save", serde(rename = "n", default = "Vec::new"))]"#
    ));
}

#[test]
fn serde_guard_ignores_skipped_fields_and_non_keys() {
    assert!(!serde_attr_declares_unsafe_default(
        "#[cfg_attr(feature = \"save\", serde(skip, default))]"
    ));
    assert!(!serde_attr_declares_unsafe_default(
        r#"#[serde(rename = "default")]"#
    ));
    assert!(!serde_attr_declares_unsafe_default(
        r#"#[serde(skip_serializing_if = "Option::is_default")]"#
    ));
    assert!(!serde_attr_declares_unsafe_default("// #[serde(default)]"));
}
