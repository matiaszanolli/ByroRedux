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
    [".register_component::<", ".register_resource::<"]
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
        manifest.join("../crates/physics/src"),
    ] {
        rust_sources_below(&root, &mut candidates);
    }

    candidates.retain(|path| {
        let Ok(source) = std::fs::read_to_string(path) else {
            return false;
        };
        source.contains("cfg_attr(feature = \"save\"")
            || source.contains("cfg_attr(feature = \"inspect\"")
            || registered.iter().any(|name| defines_type(&source, name))
    });
    // Non-turbofish and nested payloads cannot be derived from the registry's
    // surface syntax. Keep these explicit edges small and asserted below.
    candidates.extend([
        manifest.join("../crates/core/src/form_id.rs"),
        manifest.join("../crates/core/src/ecs/components/form_id.rs"),
        manifest.join("../crates/core/src/string/mod.rs"),
        manifest.join("../crates/plugin/src/esm/records/script_instance.rs"),
    ]);
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

/// Collect complete `#[...]` spans, including rustfmt-wrapped multi-line
/// attributes. Bracket matching ignores strings so serde expressions such as
/// `default = "Vec::new"` cannot terminate a span early.
fn attribute_spans(source: &str) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = source[cursor..].find("#[") {
        let start = cursor + relative;
        let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
        if !source[line_start..start].trim().is_empty() {
            cursor = start + 2;
            continue;
        }
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        let mut end = None;
        for (i, &byte) in bytes[start..].iter().enumerate() {
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
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(start + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        out.push(&source[start..end]);
        cursor = end;
    }
    out
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

fn normalized_serialized_shapes() -> Vec<String> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut shapes = Vec::new();
    for path in save_type_sources() {
        let source = std::fs::read_to_string(&path).unwrap();
        // Walk declarations separately: a serialized derive belongs to the
        // next struct/enum declaration before any intervening item body.
        for (offset, _) in source
            .match_indices("struct ")
            .chain(source.match_indices("enum "))
        {
            let line_start = source[..offset].rfind('\n').map_or(0, |i| i + 1);
            let prefix = source[line_start..offset].trim();
            if !matches!(prefix, "" | "pub" | "pub(crate)" | "pub(super)") {
                continue;
            }
            let prior = &source[..line_start];
            let Some(attr_start) = prior.rfind("#[") else {
                continue;
            };
            let between = &source[attr_start..line_start];
            if !between.contains("derive")
                || !(between.contains("serde::Serialize")
                    || between.contains("Serialize, Deserialize")
                    || between.contains("Serialize,")
                    || between.contains("Deserialize, Serialize"))
            {
                continue;
            }
            if between.contains("}\n") || between.contains(";\n") {
                continue;
            }

            let rest = &source[offset..];
            let end = if let Some(open_rel) = rest.find('{') {
                let open = offset + open_rel;
                let mut depth = 0usize;
                let mut in_string = false;
                let mut escaped = false;
                let mut found = None;
                for (i, byte) in source.as_bytes()[open..].iter().copied().enumerate() {
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
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                found = Some(open + i + 1);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                found.unwrap_or(source.len())
            } else {
                source[offset..]
                    .find(';')
                    .map_or(source.len(), |i| offset + i + 1)
            };
            let relative = path.strip_prefix(manifest).unwrap_or(&path);
            let normalized: String = source[offset..end]
                .lines()
                .map(|line| line.split("//").next().unwrap_or(""))
                .collect::<String>()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            shapes.push(format!("{}:{normalized}", relative.display()));
        }
    }
    shapes.sort();
    shapes.dedup();
    shapes
}

fn save_shape_fingerprint() -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for shape in normalized_serialized_shapes() {
        for byte in shape.bytes().chain(std::iter::once(0)) {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    }
    hash
}

#[test]
fn saved_type_shape_changes_require_format_major_bump() {
    // #3112 — v8 moves the wielded weapon out of `EquipmentSlots`
    // occupancy bit 31 into its own required field.
    //
    // #3460 — the fingerprint moved WITHOUT a FORMAT_MAJOR bump, deliberately.
    // `Material` dropped three write-only bools (`soft_lighting`,
    // `rim_lighting`, `back_lighting`); the fact they carried still ships in
    // `effect_shader_flags`, whose shape is unchanged. Removing a field is
    // read-compatible here because no saved type sets
    // `#[serde(deny_unknown_fields)]`, so a v8 save written with those keys
    // still deserializes — the extra keys are ignored and the surviving packed
    // word is authoritative. A bump would have invalidated existing saves to
    // delete two unread booleans. Adding a *required* field, or changing the
    // meaning of an existing one, still requires the bump.
    //
    // #3332 — this guard is **file**-scoped, not type-scoped. It hashes every
    // serialized-shape span in each file of `save_type_sources()`, and
    // `crates/core/src/ecs/components/escort.rs` is in that set because
    // `EscortState` is registered. The field added there
    // (`EscortBehavior::collect_distance`) is on the *sibling* type, which
    // `registry_completeness_tests` lists as deliberately NOT saved
    // ("active-package-derived config rebuilt at spawn and replaced by
    // ambient_ai_package_system"). No on-disk shape changed, so no save can
    // be invalidated by it. If a future edit moves this fingerprint, check
    // whether the changed type is actually registered before reaching for a
    // FORMAT_MAJOR bump — that change alone invalidated nothing.
    //
    // #3333 — v9 is a genuine one: `Seated.animation_restore` is a required
    // field on a registered column, and both it and `AnimationPlayer` are
    // saved, so a pre-v9 snapshot of a seated actor carries the *parked*
    // player with no record of what preceded it.
    // #3470 — the fingerprint moved WITHOUT a FORMAT_MAJOR bump, deliberately,
    // and this is the case the #3332 note above tells you to check for.
    // `AnimationPlayer::last_delta` and `AnimationLayer::last_delta` are new
    // fields on two registered columns, but both carry
    // `#[cfg_attr(feature = "inspect", serde(skip))]`: they are never written
    // to disk and are defaulted on read, so old and new snapshots have
    // byte-identical JSON for both types and no save is invalidated.
    //
    // Defaulting is not a guess here either, which is what separates this from
    // the `serde(default)` footgun the sibling test bans: `last_delta` is
    // per-frame transient state rewritten by the next `advance_*` before any
    // consumer reads it, and a freshly-loaded save has by definition not
    // advanced — so `0.0` is the correct value, not a fabricated one.
    //
    // NOTE this fingerprint hashes raw struct source, so it moves for a
    // `serde(skip)` field even though the on-disk shape did not. Teaching
    // `normalized_serialized_shapes` to drop skipped fields would be more
    // precise, but this file is itself inside the scanned set, so the change
    // perturbs its own input — left alone rather than made self-referential.
    // #3530 — v10 is a genuine bump of the v5/v6/v7 `Material` class:
    // `Material::parallax_height_in_alpha` is a new required field on a
    // registered column. See `FORMAT_MAJOR`'s doc for why the bump was taken
    // even though `false` happens to be the correct value for every pre-v10
    // snapshot — the blanket rule is the point.
    // #3762 — the fingerprint moved WITHOUT a FORMAT_MAJOR bump, and this is
    // the #3332 case again: `crates/core/src/ecs/components/creature_attack.rs`
    // is swept into this file-scoped scan because it carries a
    // `cfg_attr(feature = "inspect", ...)` derive, but `CreatureAttack` is NOT
    // registered in `build_save_registry` — it is on
    // `registry_completeness_tests`' NOT_SAVED_BY_DESIGN list (write-once at
    // NPC spawn from the CREA record's `DATA.Damage`, re-derived on reload,
    // the `FactionRanks` class). No on-disk column gained, lost or changed a
    // field, so no existing save is invalidated and there is nothing for a
    // major bump to protect.
    // Provider continuation persistence adds a new independently-keyed
    // resource rather than changing an existing serialized type. The save
    // registry fingerprint already rejects a snapshot produced without that
    // resource schema, so FORMAT_MAJOR does not need to duplicate that gate.
    // Provider invocation arguments now preserve literal-vs-local identity in
    // the already-saved continuation queue. That changes an existing nested
    // on-disk type, so v11 deliberately rejects older suspended handlers.
    // Provider continuations now retain the owning legacy-script principal so
    // a resumed stateful compatibility call cannot lose namespace isolation.
    // That required field makes v12 intentionally incompatible with v11.
    // Fragment provider barriers now retain that same owner through saved
    // quest/scene continuations, making v13 intentionally incompatible with
    // v12 rather than restoring those calls into a global namespace.
    // In-progress ModEvent builders are now nested in extension save state so
    // latent scripts can resume without losing principal ownership. The new
    // serialized field makes v14 intentionally incompatible with v13.
    // Fixed SendModEvent statements now retain their resolved portable sender
    // through saved provider tails. The new tagged statement shape makes v15
    // intentionally incompatible with v14 rather than dropping sender identity.
    const BASELINE_MAJOR: u16 = 15;
    const BASELINE_SHAPE_FINGERPRINT: u64 = 0x9295_0be7_8d8f_b13f;
    assert_eq!(
        byroredux_save::FORMAT_MAJOR,
        BASELINE_MAJOR,
        "FORMAT_MAJOR changed; regenerate the saved-type shape baseline deliberately"
    );
    let actual = save_shape_fingerprint();
    assert_eq!(
        actual, BASELINE_SHAPE_FINGERPRINT,
        "saved serialized type shape changed without updating FORMAT_MAJOR/baseline; actual={actual:#018x}"
    );
}

#[test]
fn serde_default_on_saved_struct_requires_format_major_bump() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();
    for path in save_type_sources() {
        let src = std::fs::read_to_string(&path).unwrap();
        for attribute in attribute_spans(&src) {
            if serde_attr_declares_unsafe_default(attribute) {
                let relative = path.strip_prefix(manifest).unwrap_or(&path);
                let offset = attribute.as_ptr() as usize - src.as_ptr() as usize;
                let line = src[..offset].bytes().filter(|&b| b == b'\n').count() + 1;
                offenders.push(format!("{}:{line}", relative.display()));
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
    for suffix in [
        "core/src/form_id.rs",
        "core/src/ecs/components/form_id.rs",
        "core/src/string/mod.rs",
        "plugin/src/esm/records/script_instance.rs",
    ] {
        assert!(
            sources.iter().any(|path| path.ends_with(suffix)),
            "save schema discovery must include nested/non-turbofish payload source {suffix}"
        );
    }
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
    let wrapped = "#[cfg_attr(\n    feature = \"save\",\n    serde(rename = \"n\", default)\n)]";
    let spans = attribute_spans(wrapped);
    assert_eq!(spans.len(), 1);
    assert!(serde_attr_declares_unsafe_default(spans[0]));
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
