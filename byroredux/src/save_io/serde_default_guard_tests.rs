//! Save-schema source guard tests.
//!
//! The set of scanned files is derived from `build_save_registry` and the
//! workspace's feature-gated save derives. This deliberately avoids another
//! hand-maintained shadow registry: moving or registering a type changes the
//! scan automatically.

use std::path::{Path, PathBuf};

fn registered_type_names(registry_source: &str) -> Vec<&str> {
    [".register_component::<", ".register_replacing_component::<", ".register_resource::<"]
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

/// Does `source` declare a type *named exactly* `type_name`?
///
/// #4141 — the match must end on a word boundary. This was a plain
/// `contains("struct Inventory")`, which also matches `struct
/// InventorySnapshot` / `InventoryEntry` / `InventoryItemView`, and
/// likewise `Material` → `MaterialKind`/`MaterialTable`/`MaterialValue`
/// and `Transform` → `TransformChannel`/`TransformValue`.
///
/// Harmless while only five roots were scanned — the prefix collisions
/// all live in crates the old list never reached. Widening discovery to
/// the whole workspace turns it into a live hazard in the other
/// direction: without this boundary check, all nine of those unrelated
/// files enter the scan, and a `#[serde(default)]` added to any of them
/// would fail the guard and demand a `FORMAT_MAJOR` bump for a type that
/// never enters a save at all.
fn defines_type(source: &str, type_name: &str) -> bool {
    ["struct ", "enum ", "type "].into_iter().any(|kind| {
        let needle = format!("{kind}{type_name}");
        source.match_indices(&needle).any(|(start, _)| {
            let rest = &source[start + needle.len()..];
            !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_')
        })
    })
}

/// Every `.rs` file under every discovered scan root, before the
/// save-relevance filter in [`save_type_sources`].
///
/// Split out so the *discovery* half is observable on its own: the
/// filtered output can't distinguish "this root was walked and held
/// nothing relevant" from "this root was never walked at all", which is
/// precisely the failure #4141 is about.
fn save_scan_candidates() -> Vec<PathBuf> {
    use super::registry_completeness_tests::{collect_rs_files, discover_scan_roots};

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for root in discover_scan_roots(manifest) {
        collect_rs_files(&root, &mut files);
    }
    files
}

/// #4141 — discovery roots come from the sibling guard's live workspace
/// enumeration, not a hardcoded list.
///
/// This file used to name five roots (`byroredux/src` plus core / plugin /
/// scripting / physics). That covered every registered type at the time,
/// which is exactly why it was easy to leave alone — and it is the same
/// shape `registry_completeness_tests.rs` stopped trusting for *itself*
/// under #3497, having found that a static list "has no defense against a
/// root never being added". Switching that file to
/// [`discover_scan_roots`] immediately surfaced four real types the old
/// list had missed, in `crates/renderer`, `crates/debug-ui` and
/// `crates/save`.
///
/// The two guards share a scan target, so they now share the scan. A
/// save-participating type landing in an unscanned crate can no longer
/// slip past the shape and serde-default guards just because nobody
/// remembered to widen a second list.
fn save_type_sources() -> Vec<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let registry_source = include_str!("../save_io.rs");
    let registered = registered_type_names(registry_source);
    let mut candidates = save_scan_candidates();

    candidates.retain(|path| {
        let Ok(source) = std::fs::read_to_string(path) else {
            return false;
        };
        // `inspect` derives make components readable by the debug registry;
        // they do not make the component part of a save. Including every
        // inspect-only file swept render-derived state such as WaterMaterial
        // into the save fingerprint even though WaterPlane is deliberately
        // NOT_SAVED_BY_DESIGN. Saved roots are covered by the live registry
        // name scan, with feature-gated save derives covering standalone
        // serialized payloads.
        source.contains("cfg_attr(feature = \"save\"")
            || registered.iter().any(|name| defines_type(&source, name))
    });
    // Non-turbofish and nested payloads cannot be derived from the registry's
    // surface syntax. Keep these explicit edges small and asserted below.
    candidates.extend([
        manifest.join("../crates/core/src/form_id.rs"),
        manifest.join("../crates/core/src/ecs/components/form_id.rs"),
        manifest.join("../crates/core/src/string/mod.rs"),
        manifest.join("../crates/plugin/src/esm/records/script_instance.rs"),
        // #5059 — nested payloads of registered columns whose defining files
        // match neither filter arm: `core` derives are `inspect`-gated (not
        // `save`-gated) and define no registered name. `lighting.rs` holds
        // `LightSource.emitter`'s `Emitter` tree; the `sdk` files hold
        // `FormRef` (`FactionRelations`) and `ScriptValue`
        // (`PapyrusProviderContinuationQueue`).
        manifest.join("../crates/core/src/lighting.rs"),
        manifest.join("../crates/sdk/src/identity.rs"),
        manifest.join("../crates/sdk/src/script_function.rs"),
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
    attribute_spans_with_offsets(source)
        .into_iter()
        .map(|(start, end)| &source[start..end])
        .collect()
}

/// [`attribute_spans`] with byte offsets, so a caller can chain spans
/// contiguously (#5403's declaration-block walk).
fn attribute_spans_with_offsets(source: &str) -> Vec<(usize, usize)> {
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
        match end {
            Some(end) => {
                out.push((start, end));
                cursor = end;
            }
            None => break,
        }
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
    keys.contains(&"default") && !keys.contains(&"skip")
}

/// Declarations that share a scanned source file with a saved payload but
/// are never held by a registered column (#5059): `crates/sdk/src/
/// script_function.rs` carries the `ScriptValue` payload of
/// `PapyrusProviderContinuationQueue` next to the extension-manifest
/// function declarations, whose `serde(default)` fields are manifest
/// compatibility, not save data. Whole-file scoping is why they need naming
/// here; [`not_saved_serde_default_exemptions_are_still_declared`] keeps the
/// list from going stale.
const NOT_SAVED_SERDE_DEFAULT_DECLARATIONS: &[&str] = &[
    "ScriptParameterDeclaration",
    "ScriptResultDeclaration",
    "ScriptFunctionDeclaration",
];

/// Name of the nearest `struct`/`enum` declaration at or before `offset`.
fn enclosing_declaration(source: &str, offset: usize) -> Option<&str> {
    source[..offset].lines().rev().find_map(|line| {
        let line = line.trim_start();
        let line = line
            .strip_prefix("pub(crate) ")
            .or_else(|| line.strip_prefix("pub(super) "))
            .or_else(|| line.strip_prefix("pub "))
            .unwrap_or(line);
        let rest = line
            .strip_prefix("struct ")
            .or_else(|| line.strip_prefix("enum "))?;
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        Some(&rest[..end])
    })
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
            // #5403 — the WHOLE contiguous attribute block above the
            // declaration belongs to it, not just the nearest `#[`. The
            // save derive can sit above a plain `#[derive(Debug, …)]`
            // (StoryManagerNodeState / SmNodeRuntime landed in that
            // order), and the nearest-`#[` read skipped them: a shape
            // change then passed every guard without a bump. Walk the
            // block through bracket-matched spans — contiguity (only
            // whitespace between an attribute's end and the block above)
            // is what proves an attribute belongs to this declaration.
            let spans: Vec<(usize, usize)> = attribute_spans_with_offsets(&source);
            let mut block_start = line_start;
            loop {
                let text_end = source[..block_start]
                    .trim_end_matches(['\n', '\r', ' ', '\t'])
                    .len();
                let Some(&(span_start, _)) =
                    spans.iter().rev().find(|&&(_, end)| end == text_end)
                else {
                    break;
                };
                block_start = span_start;
            }
            let between = &source[block_start..line_start];
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

/// #5403 — the shape scan must read the WHOLE contiguous attribute block
/// above a declaration. `StoryManagerNodeState` and `SmNodeRuntime` put
/// the save `cfg_attr` above a plain `#[derive(Debug, …)]`; the old
/// nearest-`#[` read was blind to exactly that ordering (26b6a779c
/// added both without a baseline refresh and the guard stayed green).
#[test]
fn the_shape_scan_sees_the_whole_attribute_block() {
    let shapes = normalized_serialized_shapes();
    // Shapes are whitespace-stripped declaration spans, hence the
    // unspelled markers.
    for marker in ["structStoryManagerNodeState", "structSmNodeRuntime"] {
        assert!(
            shapes.iter().any(|shape| shape.contains(marker)),
            "{marker} must contribute a serialized shape — the attribute-block \
             walk regressed to the nearest-#[ read"
        );
    }
}

/// #5403 — the block walk's two edges on one synthetic source: a save
/// derive placed ABOVE a plain derive is recognized (the blind
/// ordering), and an attribute belonging to the PREVIOUS item — with a
/// `;`-terminated body between — is never absorbed into the block (a
/// plain-derive neighbour must not inherit a save derive).
#[test]
fn attribute_block_walk_pins_the_blind_ordering_and_the_boundary() {
    let source = concat!(
        "#[derive(serde::Serialize, serde::Deserialize)]\n",
        "#[derive(Debug, Clone, Default, PartialEq)]\n",
        "pub struct BlindOrdering {\n",
        "    pub a: u32,\n",
        "}\n",
        "\n",
        "#[derive(Debug)]\n",
        "pub struct NotSerialized {\n",
        "    pub b: u32,\n",
        "}\n",
    );
    let spans = attribute_spans_with_offsets(source);
    let block_for = |decl: &str| {
        let line_start = source.find(decl).expect("decl present");
        let mut block_start = line_start;
        loop {
            let text_end = source[..block_start]
                .trim_end_matches(['\n', '\r', ' ', '\t'])
                .len();
            let Some(&(span_start, _)) =
                spans.iter().rev().find(|&&(_, end)| end == text_end)
            else {
                break;
            };
            block_start = span_start;
        }
        &source[block_start..line_start]
    };
    let blind = block_for("pub struct BlindOrdering");
    assert!(
        blind.contains("serde::Serialize") && blind.contains("derive(Debug"),
        "the block spans both attributes: {blind:?}"
    );
    let neighbour = block_for("pub struct NotSerialized");
    assert!(
        !neighbour.contains("serde::Serialize"),
        "the previous item's body terminates the walk: {neighbour:?}"
    );
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
    // JContainers compatibility objects now persist explicit retain counts and
    // recovery tags. The required registry field makes v16 intentionally
    // incompatible with v15 rather than silently discarding ownership.
    // Provider continuations now persist typed scalar expression nodes so
    // arithmetic and string concatenation survive a save/load boundary. The
    // tagged value/statement shapes make v17 intentionally incompatible with
    // v16 rather than dropping a pending assignment.
    // Computed provider receiver arguments and their proven object types now
    // persist in the same continuation value tree. The new tagged argument
    // shape makes v18 intentionally incompatible with v17 rather than
    // silently changing a resumed receiver dispatch.
    // #3073 — `Material::parallax_height_scale` / `parallax_max_passes` are
    // new required fields on the registered `Material` column, resolved
    // once at the parser→`Material` boundary (mirrors v10's
    // `parallax_height_in_alpha`, #3530). v19 is intentionally incompatible
    // with v18 per the same blanket rule, even though the shared defaults
    // happen to be correct for every pre-v19 snapshot.
    // #3489 — the fingerprint moved WITHOUT a FORMAT_MAJOR bump, deliberately,
    // and this is a THIRD case beyond the two named at #3332/#3762 above:
    // `Effect::Enable` is a new *enum variant* on `crate::translate::effects
    // ::Effect`, not a new required struct field. `PendingFragmentExecution
    // .effects: Vec<Effect>` (inside the registered `FragmentExecutionQueue`
    // — a script paused mid-`Utility.Wait()` genuinely persists queued
    // `Effect`s) is real saved data, but adding a variant no prior save can
    // ever contain doesn't change how that prior data decodes: serde only
    // has to recognize the tags actually present, and old saves have none
    // named "Enable". A required *field*, by contrast, is unconditionally
    // present in every serialized instance and genuinely needs the bump
    // (v10's `parallax_height_in_alpha`, v19 above) — that asymmetry is why
    // this one case in the enum family is compatible where the struct-field
    // ones are not.
    // #3701 (ECS-2026-08-30-D10-01) — v20 adds `AnimationLayer::
    // blend_in_target` as a required field on the registered
    // `AnimationStack` column's nested layer type. See `FORMAT_MAJOR`'s
    // own doc for why this exact case (a saved-type's own doc example)
    // needs the bump.
    // #3251 (ECS-2026-08-24-03) — the fingerprint moved WITHOUT a
    // FORMAT_MAJOR bump. `AnimatedTextureFlip::handle_for_slot`'s body
    // changed (`.map(...).unwrap_or(0)` → `.and_then(...)`, fixing an
    // out-of-range `current_index` silently aliasing to bindless handle
    // 0). `AnimatedTextureFlip` is a **tuple** struct
    // (`crates/core/src/ecs/components/animated.rs`) with no `{` of its
    // own, so `normalized_serialized_shapes`'s brace-matching walks
    // forward to the next `{` in the file — the following `impl` block's
    // — and sweeps `handle_for_slot`'s method body into the captured
    // "shape" span, a known quirk this guard's own #3332 note already
    // names ("this guard is file-scoped, not type-scoped"). No saved data
    // shape changed: `AnimatedTextureFlip` is on
    // `registry_completeness_tests.rs`'s `NOT_SAVED_BY_DESIGN` allowlist
    // ("per-frame output re-derived every tick... re-resolved by
    // attach_animation_sinks on load").
    // #2573 (OBL-D5-03) — v21 adds `Material::specular_authored` as a
    // required field on the registered `Material` column (see
    // `FORMAT_MAJOR`'s own doc). Genuine save-shape change, bump taken.
    // #3852 — the fingerprint moved WITHOUT a FORMAT_MAJOR bump, deliberately,
    // and this is a FOURTH case beyond #3332/#3762/#3489 above: nothing about
    // any saved type changed at all. `papyrus_provider.rs` was split into
    // `papyrus_provider/{mod,runtime,catalog,ir,lower_call,lower_program,
    // execute}.rs`, and this guard is file-scoped: `save_type_sources()`
    // returns a `sort()`ed path list, so relocating a saved type to a
    // different file re-orders the hashed shape stream even though every
    // shape is byte-identical. The split also raised cross-module items to
    // `pub(crate)`, which lands inside the captured span (`pub(crate)
    // qualified_name: String`) without meaning anything to serde — field
    // visibility has never affected serialization.
    //
    // Verified rather than assumed: dumping `normalized_serialized_shapes()`
    // either side of the split gives 149 shapes both times, and the two
    // sorted lists are identical once the path prefix and the visibility
    // keyword are normalized away. No saved data shape changed, so no bump.
    // #3159 — v22. `Effect` gained `SetLocked` / `SetLockLevel`, and `Effect`
    // is genuinely saved: `FragmentExecutionQueue`'s suspended-continuation
    // tail is a `Vec<Effect>`. Verified rather than assumed, per the #3852
    // note above — dumping `normalized_serialized_shapes()` either side of
    // the change gives 149 shapes both times with exactly one differing
    // line, the `Effect` enum itself. So the fingerprint move is real, not
    // a file-scoping artefact.
    // #4140 — but the bump was **blanket, not required**, and this entry
    // used to say the opposite ("a variant insertion, which shifts later
    // discriminants"). That reasoning is wrong for this format and
    // directly contradicts #3489 above, which reached the correct
    // conclusion on the *same enum* three bumps earlier: saves are
    // `serde_json`, `Effect` is externally tagged (`{"SetLocked": {…}}`,
    // keyed by variant name), and `serde_json` ignores the `variant_index`
    // serde_derive hands it. Old saves simply contain no tag named
    // `SetLocked`, exactly as #3489 says of `Enable`. Two entries in this
    // same precedent bank giving contradictory mechanisms for structurally
    // identical changes is precisely the failure this list exists to
    // prevent, which is why the wrong one is corrected rather than left.
    // The bump itself stands: it fails closed, and reverting a shipped
    // FORMAT_MAJOR is worse than the unnecessary invalidation it caused.
    // #3861 — the fingerprint moved WITHOUT a FORMAT_MAJOR bump, and this is
    // the #3852 case again (a naming/file-scoping artefact, not a data-shape
    // change) rather than a fifth category. `ImageSpaceModifierFrame` in
    // `crates/scripting/src/cinematic.rs` and `ImageSpaceModifierView` in
    // `crates/renderer` were identical 14-field twins bridged by a
    // hand-written copy; one definition now lives in
    // `crates/core/src/imagespace.rs` as `ImageSpaceModifier`.
    //
    // Verified rather than assumed, per the #3852 note above. Dumping
    // `normalized_serialized_shapes()` either side of the change gives 149
    // shapes both times, with exactly TWO differing lines, and both differ
    // only in the type's *name* and the file it was found in:
    //   - the struct itself: same 14 fields, same order, same types;
    //   - `CinematicPresentationState`'s `image_space_modifier_frame` field,
    //     whose name and position are unchanged — only its type's spelling.
    // The save format is `serde_json` (`crates/save/src/snapshot.rs:212`), and
    // `serialize_struct`'s `name` argument is not emitted by that serializer:
    // serializing the type and asserting the JSON contains no occurrence of
    // "ImageSpaceModifier" passes. Field names and order are what JSON
    // encodes, and neither moved, so every existing save still loads.
    // #3854 — the fingerprint moved WITHOUT a FORMAT_MAJOR bump, and this is
    // the #3852 / #3861 file-scoping category again, not a data-shape change.
    // `fragment.rs` (2,713 LOC) was split into `fragment/{state,effects,
    // populate,systems}.rs`, so four saved types are now found in
    // `fragment/state.rs`; items reached across the new module boundary also
    // picked up a `pub(crate)` visibility prefix.
    //
    // Verified rather than assumed, per the #3852 note above. Dumping
    // `normalized_serialized_shapes()` either side of the split gives 149
    // shapes both times with exactly FOUR differing lines
    // (`ReferenceEnableState`, `FragmentExecutionQueue`,
    // `PendingFragmentExecution`, `FragmentResumeCondition`), and every one
    // differs only in:
    //   - the file path the type was found in, and
    //   - a `pub(crate)` prefix on some fields.
    // No field is added, removed, renamed, reordered or retyped, and no enum
    // variant is inserted. Field visibility is a Rust-side concept that
    // `serde_json` never encodes (`crates/save/src/snapshot.rs`); names and
    // order are what it writes, and neither moved. Every existing save still
    // loads, so the baseline is refreshed rather than the format bumped.
    // #4136 — the fingerprint moved WITHOUT a FORMAT_MAJOR bump, and this
    // is a FIFTH case beyond #3332/#3762/#3489/#3852 above: a brand-new
    // registered *column*, not a change to an existing saved type.
    // `ReferenceLockState` (plus its `LockOverride` payload enum) is added
    // to `build_save_registry` so a scripted lock/unlock survives both a
    // save/load and an in-session cell revisit.
    //
    // No prior save contains a `ReferenceLockState` key, and
    // `restore_resources` (`crates/save/src/driver.rs`) iterates the
    // *registry's* entries and loads one only when
    // `snapshot.resources.get(name)` is `Some` — a column absent from an
    // older snapshot is skipped, leaving the resource at its `Default`.
    // That default is the empty ledger, which means "no script has touched
    // any lock", which is exactly right for a save taken before the
    // feature existed: the plugin's authored XLOC stands, the pre-#4136
    // behaviour. Verified against the driver rather than assumed.
    //
    // The reverse direction is equally safe: an older build reading a
    // newer save iterates its own registry, which has no such entry, and
    // never looks at the extra key. Same reasoning class as #3489's
    // variant insertion — and see the #3159 entry above, where the
    // *wrong* mechanism was cited for a structurally similar call (#4140).
    // v22 -> v23: `CinematicPresentationState` gained six new fields for
    // MQ101's chargen-gate effects (`SetInChargen`/`ShowRaceMenu`/
    // `RequestSave`) — see `byroredux_save::FORMAT_MAJOR`'s doc for the
    // full field list and why SAVE-D2-01 (#1714) rules out `serde(default)`
    // here.
    //
    // Fingerprint refreshed again, WITHOUT a further major bump, for
    // `Effect::SetEnemy`/`Effect::StartCombat` (MQ101's dragon-attack/
    // keep-escape combat gate). Same #4140-corrected rationale as v22's
    // own `Effect::Enable` insertion, cited in `FORMAT_MAJOR`'s doc above:
    // `Effect` is `serde_json` externally-tagged (keyed by variant name,
    // never ordinal), so inserting a variant at any position is
    // backward-compatible for decoding a pre-this-commit tail. Verified
    // the same way as those entries: `normalized_serialized_shapes()`
    // differs by exactly the two new variant lines, nothing existing
    // reordered/retyped/renamed.
    //
    // #3901 — the fingerprint moved WITHOUT a FORMAT_MAJOR bump, the same
    // way #3251 did for this file: `TextureFlipEntry.texture_slot: u32`
    // became `role: FlipTextureRole`, and the new enum sits beside it in
    // `crates/core/src/ecs/components/animated.rs`. `AnimatedTextureFlip`
    // is on `registry_completeness_tests.rs`'s `NOT_SAVED_BY_DESIGN`
    // allowlist (re-resolved by `attach_animation_sinks` on load), so no
    // snapshot has ever contained either shape.
    //
    // #4318 — fingerprint refreshed WITHOUT a major bump: `Effect::SetEnemy`
    // lost its `modify_player`/`modify_enemy` bools, invented names for
    // Papyrus's `abSelfIsNeutralToOther`/`abOtherIsNeutralToSelf` (only the
    // literal `false, false` form lowers now, so the fields carried nothing).
    // Removing fields from an externally-tagged `serde_json` struct variant
    // still decodes a pre-this-commit tail: serde ignores the two stale keys,
    // nothing in this shape sets `deny_unknown_fields`, and no remaining
    // field was reordered, renamed or retyped.
    //
    // #4322 — refreshed again WITHOUT a major bump: `SetInChargen`'s three
    // flags, on both `CinematicPresentationState` and `Effect::SetInChargen`,
    // were renamed to Skyrim's declared parameter names. Each renamed field
    // carries `serde(alias = "<old name>")`, so a v23 save written under the
    // invented names decodes into the same values —
    // `set_in_chargen_renames_still_decode_v23_keys` round-trips both carriers
    // through the old keys to prove it. No field was retyped or reordered.
    //
    // #4301 — refreshed WITHOUT a major bump, the #3251 / #3901 case again:
    // `TextureFlipEntry` gained `handles_have_alpha: Vec<bool>` (and
    // `AnimatedTextureFlip` an `active_has_alpha` method, inside the span
    // this file-scoped guard sweeps). `AnimatedTextureFlip` is on
    // `registry_completeness_tests.rs`'s `NOT_SAVED_BY_DESIGN` allowlist,
    // re-resolved by `attach_animation_sinks` on load, so no snapshot has
    // ever contained either shape.
    //
    // #4427 — refreshed WITHOUT a major bump, two NOT_SAVED_BY_DESIGN moves
    // in one refresh. `AnimatedTextureFlip` gained an `all_handles` method
    // (the #4301 case: inside the span this file-scoped guard sweeps, no
    // field touched). And `7996edf61` had already left this guard red by
    // inserting `WaterMaterial.normal_encoding` (plus the new
    // `WaterNormalEncoding` enum) in `crates/core/src/ecs/components/water.rs`
    // and retuning `GroundCoverSpecies::DEFAULT_TEMPERATE`: `WaterMaterial`
    // is only carried by `WaterPlane`, and `WaterPlane` and the ground-cover
    // types are all on `registry_completeness_tests.rs`'s
    // `NOT_SAVED_BY_DESIGN` allowlist (rebuilt from XCWT/WATR/GRAS at cell or
    // worldspace entry), so no snapshot has ever contained either shape.
    const BASELINE_MAJOR: u16 = 104;
    // #4465 — refreshed WITH a major bump (v24 -> v25). `ReferenceState`
    // gained the required `picked_up` tombstone field (the durable half of
    // the P3 `PickedUp` marker, carried through the registered
    // `PersistentReferenceStates` resource). The P3 commit had landed it
    // with `#[serde(default)]` — the exact masked intra-type change this
    // guard exists to prevent — so the default is removed and the version
    // check rejects pre-v25 saves instead of silently default-filling them.
    // (The fingerprint moved twice in this refresh: once for the new field,
    // once more once the `#[serde(default)]` attribute itself left the
    // struct's scanned span — both directions are the shape change.)
    // Lighting unification: VisibilityMask is still a u8 newtype; only its
    // import policy changed. The tuple-struct scanner also includes its impl
    // (same false positive as W2.10 below). No serialized field changed.
    // Interior volumetric beams add `FogShape::Cone`. This file-scoped guard
    // scans its inspect-only derive, but FogVolume (and its FogBounds/FogShape
    // payload) is NOT_SAVED_BY_DESIGN: it is rebuilt from cell/NIF data on
    // load. No registered save column or on-disk shape changed.
    // #4612 — refreshed WITHOUT a major bump: `QuestStageState` and
    // `QuestObjectiveState` gained a `revision: QuestRevision` change key for
    // the HUD objective cache, declared `#[serde(skip, default)]` exactly like
    // `QuestStageState.events` beside it. It is never written and is
    // defaulted (a fresh process-unique value) on read, so no snapshot's
    // bytes change in either direction; only the scanned source span moved.
    // #4415 — refreshed WITH a major bump (v26 -> v27): `SpellList` (a
    // `Vec<u32>` of spell FormIDs, `crates/scripting/src/magic.rs`) joined
    // the saved registry as a new column, the actor's spells that
    // AddSpell/RemoveSpell now mutate.
    // #4414 — refreshed WITH a major bump (v27 -> v28): `FactionRelations`
    // (a `Vec<FactionReactionOverride>` of directed `FormRef` pairs and a
    // raw reaction, `crates/scripting/src/combat.rs`) joined the saved
    // registry as a new resource column.
    // #4813 — refreshed WITH a major bump (v28 -> v29):
    // `ReferenceEnableState` gained the required `enabled` set (a scripted
    // `Enable()` is recorded so it can override a placement's authored
    // "Initially Disabled" flag), with no `serde(default)`.
    // #4819 — refreshed WITH a major bump (v29 -> v30): `ReferenceState`
    // (inside `PersistentReferenceStates`) gained the required `spells`
    // field, with no `serde(default)`.
    // #4816 — refreshed WITHOUT a major bump: `Effect::StopCombat` joined
    // `Effect`, the #4140-corrected `Effect::SetEnemy`/`StartCombat` case
    // above — externally tagged by variant name, so a new variant still
    // decodes every earlier tail.
    // The source scope is limited to saved derives and registered save roots;
    // inspect-only files no longer affect this hash (#4755 debug visibility
    // does not change the on-disk schema).
    // #5042 — refreshed WITH a major bump (v30 -> v31): `ActorValue` gained
    // the required `base_authored` flag (a modifier-only entry's placeholder
    // base vs an authored one), with no `serde(default)`.
    // #5017 — refreshed WITH a major bump (v31 -> v32): `ActorControlState`
    // gained the required `unconscious` flag and the parked `ReferenceState`
    // the required `control` field, neither with `serde(default)`.
    // 2026-09-30 (#5152/#5153) — refreshed WITHOUT a major bump, the
    // file-scoped sweep class again: `quest_stages.rs` moved because
    // `QuestObjectiveState` gained the serde(skip) `events` transition-cue
    // deque (and the INFO fragment table joined the not-saved registry).
    // A skipped field is never serialized — no save's bytes change, and
    // old and new snapshots decode identically.
    // 2026-10-05 (#5293) — refreshed WITHOUT a major bump, the
    // file-scoped sweep class again: `quest_stages.rs` moved because the
    // `pub(crate)` widening + comment on `QuestRevision::bump` (the token
    // `SceneActorBindings` now stamps as its change key) sits inside
    // `impl QuestRevision`, and `QuestRevision` is a serde-derived tuple
    // struct whose brace-matching sweeps that impl — the same
    // `VisibilityMask` / `3ce970a5a` mechanism. The type is still a plain
    // `u64` newtype that is `serde(skip)`-ed inside `QuestStageState`, so
    // no save's bytes change (`quest_revision_keys_never_reach_a_save`
    // still pins that).
    // 2026-10-06 (#4415) — refreshed WITHOUT a major bump, the file-scoped
    // sweep class again: `magic.rs` gained the `RaceSpells` component beside
    // the saved `SpellList`, and the guard's brace-matching sweeps the new
    // component's definition into the same file's hashed span. `RaceSpells`
    // itself is deliberately NOT_SAVED (spawn-derived, re-stamped every
    // reload), so no save's bytes change.
    // 2026-10-09 (#5412) — refreshed WITH a major bump (v33 -> v34):
    // `ActorValue` gained the required `set_override` layer (the player
    // `SetBase` reroute's own destination, split out of the
    // constant-spell `permanent_mod`), with no `serde(default)`
    // (#1714) — pre-v34 saves are rejected.
    // 2026-10-09 (#5394) — refreshed WITH a major bump (v34 -> v35):
    // `StoryEventAliasFill` (per-quest `StoryEventFill`: the session
    // `EventDataSlots` `serde(skip)`-ed, the reference slots' FormIds
    // saved) joined the saved registry as a NEW resource, so a restored
    // Story-Manager quest's `FromEvent` aliases re-fill after a load.
    // #5495 — refreshed without a bump: the move is the new sibling
    // `CreatureActor` marker in sandbox.rs (NOT_SAVED_BY_DESIGN), which
    // changes no registered type's on-disk shape (same file-scope rule as
    // the #3332 EscortBehavior note below).
    // 2026-10-10 (Wave 4) — refreshed WITH a major bump (v35 -> v36):
    // `RigidBodyData` gained the required `surface_material` byte (the
    // collision authoring's Havok material, carried through the NIF
    // import for the surface-aware footstep consumer). Registered column,
    // no `serde(default)` — pre-v36 saves are rejected, per the blanket
    // rule (#3530's precedent).
    // 2026-10-10 (Wave 4 follow-through) — refreshed WITHOUT a bump:
    // `RigidBodyData` gained `part_surface_materials`, the leaf-aligned
    // per-part sequence — a `serde(skip)` field the import re-derives
    // from the re-parsed NIF on load, so old and new snapshots have
    // byte-identical JSON for the type (the #3470 class; the file-scope
    // fingerprint moves on the raw source regardless).
    // 2026-10-11 (#5542) — refreshed WITH a major bump (v36 -> v37):
    // `RigidBodyData::surface_material` widened u8 -> u32 — Skyrim's
    // `SkyrimHavokMaterial` is a CRC32, which the byte truncated into 8
    // colliding material pairs and made unjoinable to MATT records. A
    // registered column's serialized shape changed; pre-v37 saves are
    // rejected, per the blanket rule.
    // 2026-10-12 (#5686) — refreshed WITHOUT a bump: the
    // `part_surface_materials` field doc in collision.rs gained the
    // live-overlay preserve-hook note (registration switched to
    // `register_component_preserving`). Comment-only, same file-scope
    // rule as #3332/#3762 — no on-disk shape changed.
    // 2026-10-12 (#5403) — refreshed WITHOUT a bump: the scan now reads
    // the whole contiguous attribute block, so `StoryManagerNodeState`
    // and `SmNodeRuntime` (save derive above a plain `#[derive(Debug…)]`)
    // contribute their shapes for the first time. Both were registered
    // unchanged since 26b6a779c — no on-disk shape moved, the scan just
    // stopped being blind to them.
    // 2026-10-12 (M47.2) — refreshed WITH a major bump (v42 -> v43): the
    // new `LocationAliasBindings` resource column.
    // 2026-10-12 (M47.2) — refreshed WITHOUT a bump: `Effect` gained an
    // appended `MovePlayerTo` variant (enums serialize tagged by name, so no
    // older save can carry it and every older one still reads), and
    // `fragment/state.rs` gained the unsaved `PlayerMoveRequest`.
    // 2026-10-12 (M47.2) — refreshed WITH a major bump (v43 -> v44):
    // `AddItem` / `RemoveItem` `count` widened `u32` -> `ItemCount`, and the
    // parked `ReferenceState` gained the actor's `Perks`.
    // 2026-10-12 (M47.2) — refreshed WITH a major bump (v44 -> v45): the new
    // `CreatedReferences` resource column.
    // 2026-10-12 (M47.2) — refreshed WITH a major bump (v45 -> v46): the
    // `CharacterLevel` component column.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: `FragmentGuard` gained
    // appended variants (`Any`, `ActorDead`, `HasPerk`, `ItemCount`,
    // `QuestCompleted`, `ScenePlaying`, `LocationKeyword`). Enums serialize
    // tagged by name, so no older save can carry them and every older one
    // still reads.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v46 -> v47): the new
    // `AliasOverrides` resource column and `Effect::ForceAlias`.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `Effect::ModQuestVariable` variant (name-tagged; no older save carries
    // it, every older one still reads).
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `Effect::SetActorValueNamed` variant.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v47 -> v48): the new
    // `ReferenceActivationBlocks` resource column and `Effect::BlockActivation`.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `Effect::SendStoryEvent` variant, and `EventDataSlots` gaining the
    // script-event keyword / values (serde-skipped inside `StoryEventFill`, so
    // no on-disk shape moved).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v48 -> v49): the new
    // `MapMarkers` resource column and `Effect::AddToMap`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v49 -> v50): the new
    // `LocationClearState` resource column, `Effect::SetLocationCleared` and
    // `FragmentGuard::LocationCleared`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v50 -> v51): the new
    // `Bleedout` component column, and the appended
    // `FragmentGuard::ActorBleedingOut` / `ActorEssential` variants.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v51 -> v52): the new
    // `ActorBaseProtectionOverrides` resource column, and the appended
    // `Effect::SetBaseProtection` variant (with `ActorBaseRef`).
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `Effect::Kill` variant (name-tagged; no older save carries it).
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `FragmentGuard::ReferenceEnabled` / `InFaction` / `Loaded3D` /
    // `Distance` variants.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `Effect::QuestScope` and `Effect::CopyQuestVariable` variants.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v52 -> v53): the new
    // `LevelUpPoints` component column.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v53 -> v54): the new
    // `ScriptedRelationships` resource column, the appended
    // `Effect::SetRelationshipRank` and `FragmentGuard::RelationshipRank`.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `Effect::RewardPlayerXp` and `Effect::PlayerCaps` variants.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `FragmentGuard::Compare` with `ValueExpr` / `ArithOp`.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `Effect::SetLocal` and `ValueExpr::Local`.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `Effect::SetReferenceVariable`.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `Effect::ShowMessage`, `Effect::ShowHelpMessage` and
    // `Effect::Notification`.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `Effect::ShowBarterMenu`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v54 -> v55):
    // `Effect::EvaluatePackage`'s actor became an `ActorRef`; also the
    // appended `ItemCount::Value`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v55 -> v56): the new
    // `ScriptedKeywords` resource column, the appended `Effect::ChangeKeyword`
    // and `FragmentGuard::HasKeyword`.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `FragmentGuard::SameReference`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v56 -> v57):
    // `ActorControlState.ghost`, and the appended `Effect::SetGhost`.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `Effect::SetAlly`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v57 -> v58):
    // `Effect::QuestScope.bindings`.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `ObjectRef::Linked`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v58 -> v59):
    // `SetActorValue{,Named}.value` became a `ValueExpr`, which gained
    // `ActorValue` / `ActorValueNamed`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v59 -> v60):
    // `SetGlobalValue.value` became a `ValueExpr`; `SetGlobalRelative` is gone.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v60 -> v61):
    // `CreatedReference.deletion`, and the appended `Effect::DeleteWhenAble`.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `Effect::WaitWhile`.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a bump: the appended
    // `Effect::Interpret` (the Papyrus VM).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v61 -> v62): the new
    // `QuestScriptObjects` resource column.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v62 -> v63):
    // `Effect::Interpret.resume` (`VmResume` / `VmFrame` / `VmStep` /
    // `VmStored` — the suspended call chain).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v63 -> v64):
    // `ScriptObject::Str` (string script variables, script states),
    // `VmFrame.owner` / `defined_in` / `state`, `Effect::Interpret.args`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v64 -> v65): the
    // new `ScriptUpdates` resource column (Papyrus update registrations).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v65 -> v66):
    // `Effect::IncrementStat`, `ValueExpr::MiscStat`, the `MiscStats` column.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v66 -> v67):
    // `UpdateRegistration.timer` (Fallout 4 StartTimer IDs).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v67 -> v68):
    // `PropertyValue::Struct` + `StructMember` (FO4 VMAD structs, which a
    // saved continuation's VMAD can carry).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v68 -> v69):
    // `PackageFragmentEvent` / `PackageScriptFragment` (PACK VMAD fragment
    // bindings).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v69 -> v70):
    // `Effect::AddAchievement`, the `Achievements` column.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v70 -> v71):
    // `Effect::SetPlayerTeammate` / `SetDoingFavor`,
    // `FragmentGuard::ActorFollower`, the `FollowerState` column.
    // 2026-10-13 (M47.2, #5763) — refreshed WITH a major bump (v71 -> v72):
    // `VmResume.carried_steps` (a saved continuation's frame-budget yield).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v72 -> v73): the
    // `DistanceWatches` resource column (Papyrus distance-event registrations).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v73 -> v74):
    // `ScriptObject` gained `Int` / `Float` / `Bool` / `Array` / `Struct`
    // (arrays and structs held in script variables).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v74 -> v75): the
    // `AliasCollections` resource column (script-filled RefCollectionAlias
    // members).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v75 -> v76):
    // `FragmentGuard::HasLocRefType`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v76 -> v77): the
    // `EventRegistrations` column and `VmStored::Array` (event arguments).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v77 -> v78):
    // `Effect::AdvanceSkill` and the `SkillProgress` column.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v78 -> v79):
    // `Effect::SetLinkedRef` and the `LinkedRefOverrides` column.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v79 -> v80):
    // `FactionChange::RemoveAll`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v80 -> v81):
    // `CreatedReferences.scripted` / `scripted_ids` (PlaceAtMe).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v81 -> v82): the
    // `AliasScriptOwners` column (alias script owner ids).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v82 -> v83): the
    // `HitRegistrations` column (`RegisterForHitEvent`).
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v83 -> v84):
    // `Effect::Kill.silent` and `Effect::MoveToEditorLocation`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v84 -> v85):
    // `FragmentGuard::ActorInCombat`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v85 -> v86): the
    // `LocationKeywordData` column, `Effect::SetLocationKeywordData`,
    // `ValueExpr::LocationKeywordData`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v86 -> v87): the
    // `ReferenceFlags` column.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v87 -> v88): the
    // `InputLayers` column.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v88 -> v89):
    // `Effect::ForceLocationTo`, `AliasOverride::ForcedLocation`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v89 -> v90):
    // `ValueExpr::ActorValueReading`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v90 -> v91):
    // `Effect::MoveTo.offset` / `.match_rotation`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v91 -> v92):
    // `Effect::SetLocRefType`, the `LocRefTypeOverrides` column.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a major bump: the file-scoped
    // hash (#3332) moved because `script_instance.rs` gained the serde-derived
    // `IndexedScriptFragment`, a plugin-record type no saved column holds.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v92 -> v93):
    // `Effect::DamageActorValue`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v93 -> v94):
    // the `FormListAdditions` column.
    // 2026-10-13 (M47.2) — refreshed WITHOUT a major bump: `script_instance.rs`'s
    // plugin-record `TerminalScriptFragment` became `IndexedScriptFragment` (TERM and
    // PERK fragments); no saved column holds it.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v94 -> v95):
    // `Effect::StopCombatAlarm`.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v95 -> v96):
    // `Effect::SetCrimeFaction`, the `CrimeFactionOverrides` column.
    // 2026-10-13 (M47.3) — refreshed WITH a major bump (v96 -> v97):
    // the `ObScriptQuestLocals` column.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v97 -> v98):
    // the `SpeechcraftOutcomes`, `AnimArchetypeOverrides` and `ScannedReferences`
    // columns.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v98 -> v99):
    // `ReferenceFlags` gained two sets.
    // 2026-10-13 (M47.2) — refreshed WITH a major bump (v99 -> v100):
    // the `OwnerOverrides` and `FactionCrimeGold` columns.
    // 2026-10-14 (M47.2) — refreshed WITH a major bump (v100 -> v101):
    // the `AnimationEventRegistrations` column.
    // 2026-10-14 (M47.2) — refreshed WITH a major bump (v101 -> v102):
    // the `DestructionStates` column.
    // 2026-10-14 (M47.2) — refreshed WITH a major bump (v102 -> v103):
    // `Effect::Activate` gained the required `native_only` field.
    // 2026-10-14 (M47.2) — refreshed WITH a major bump (v103 -> v104):
    // `MapMarkers` gained the star-map-visible set.
    // 2026-10-14 (M47.2) — refreshed WITHOUT a major bump, the #3470 class:
    // `AnimationLayer::bone_weights` and `AnimationStack::bone_index` (a layered
    // blend's per-bone masks) are runtime-only `serde(skip)` fields the behavior
    // graph rebuilds every frame; a save's JSON for both types is byte-identical.
    // 2026-10-14 (#5059) — refreshed WITHOUT a major bump: the guard's scan
    // widened to `core/src/lighting.rs` (`LightSource.emitter`'s tree) and
    // `sdk/src/{identity,script_function}.rs` (`FormRef`, `ScriptValue`), so
    // the file-scoped hash now covers shapes it always should have. No
    // serialized field changed; a save's JSON is byte-identical.
    const BASELINE_SHAPE_FINGERPRINT: u64 = 0x1176_8525_b9b7_609f;
    // 2026-10-15 (#5610) — refreshed WITHOUT a major bump: the orphan
    // `PerkList` component (zero production write sites; its save-registry
    // exemption row went with it) was deleted. No v104 snapshot can carry
    // one, so decode compatibility is unaffected.
    // ---- earlier refresh history (kept for the false-positive record) ----
    // 2026-09-21 (W2.10 flip) — refreshed WITHOUT a major bump, the same
    // `VisibilityMask` tuple-struct sweep class as the two entries below:
    // `for_legacy_projection`'s no-full-scene-shadow arm now returns
    // `ARCHITECTURE | STATIC_PROP | DYNAMIC_ACTOR` (was
    // `ARCHITECTURE | DYNAMIC_ACTOR`), so unflagged legacy room lights'
    // shadow rays stop passing through props and furniture — the Markarth
    // SilverBloodInn real-content A/B that W2.10 gated the flip on showed
    // actor silhouettes cast through STATIC_PROP occluders. The type is
    // still a plain `u8` newtype: no serialized field changed, and the
    // runtime mask difference is a lighting-policy value, not a shape.
    // #4422 — refreshed WITH a major bump (v23 -> v24). `Material` gained
    // the required field `detail_neutral` (the encoded-space
    // detail-combine neutral declared at the NIFAL boundary — FaceTint
    // 65/255 vs the classic MODULATE2X 128/255), changing the registered
    // `Material` column's serialized shape exactly like v19's
    // `parallax_height_scale`/`parallax_max_passes` and v21's
    // `specular_authored` did. `sanitize_finite` covers the new field, so
    // a poisoned value cannot survive a restore.
    // HardcoreMode is a newly saved resource, not a shape change to an
    // existing column. The registry fingerprint rejects earlier snapshots
    // so their missing mode flag cannot retain an outgoing live selection.
    //
    // `3ce970a5a` — refreshed WITHOUT a major bump. `VisibilityMask` is a
    // tuple struct (`struct VisibilityMask(u8)`), so this file-scoped
    // guard's brace-matching falls through the struct's own `;` terminator
    // onto the following `impl VisibilityMask` block and sweeps its method
    // bodies in too (the same mechanism that picks up `AnimatedTextureFlip`'s
    // method additions above). `for_legacy_projection`'s no-full-scene-shadow
    // arm now returns `ARCHITECTURE | DYNAMIC_ACTOR` instead of bare
    // `ARCHITECTURE`, so legacy local emitters cast dynamic-actor contact
    // shadows. `VisibilityMask`'s own shape is still a plain `u8` newtype —
    // no field was added, removed, or retyped — so a save's serialized
    // bytes for this type are unchanged.
    // W2.10 (light & shadow campaign) — refreshed WITHOUT a major bump,
    // the #3ce970a5a false-positive class again: `VisibilityMask` is a
    // tuple struct, so the file-scoped guard's brace-matching sweeps the
    // whole `impl VisibilityMask` block into the fingerprint, and W2.10's
    // new `for_legacy_local_light` policy method (a doc-table + a const
    // fn delegating to `for_legacy_projection`) moved the scan span. The
    // type's serialized shape is still a plain `u8` newtype — no field
    // added, removed, or retyped — so no snapshot decodes differently.
    //
    // v32 -> v33 (#5367 Phase L) — refreshed WITH a major bump:
    // `DialogueSpokenInfoForms` joined the saved registry as a NEW
    // resource (the spoken Say-Once ledger) — its absence in pre-v33
    // snapshots means a said-once line can legitimately re-qualify after
    // loading an old save, the same forward-default the format gates.
    //
    // v33 -> v34 (#5412) — refreshed WITH a major bump: `ActorValue`
    // gained the required `set_override` layer (the player `SetBase`
    // reroute's own destination, split out of the constant-spell
    // `permanent_mod` so an ability's add/remove stays lossless beside a
    // routed `setav`). No `serde(default)` (#1714); pre-v34 saves are
    // rejected.
    //
    // 2026-10-11 (#5300) — refreshed WITH a major bump (v37 -> v38):
    // `DialogueSpokenInfoForms` (the Say-Once ledger) changed its element
    // from an INFO FormID `u32` to a `(speaker reference FormID, INFO
    // FormID)` pair — Say Once is per actor (CK Topic Info). The set's
    // element shape changed, so a v37 save is rejected.
    //
    // M47.2 — refreshed WITHOUT a major bump: `Effect::SetActorValue` was
    // appended to the saved `Effect` enum. Saves serialize enums tagged by
    // variant name (serde_json), so every v42 document still decodes; no
    // existing variant or field changed shape.
    //
    // v41 -> v42 (M47.2) — refreshed WITH a major bump:
    // `ScriptedFactionChanges` joined the saved registry as a NEW resource
    // and `Effect::ChangeFaction` the saved `Effect` enum.
    //
    // v40 -> v41 (M47.2) — refreshed WITH a major bump: the saved
    // `Effect::Conditional` guard list became `FragmentGuard`s.
    //
    // v39 -> v40 (M47.2) — refreshed WITH a major bump:
    // `QuestScriptVariables` joined the saved registry as a NEW resource
    // (quest-script property writes); `ActorRef::Speaker` and
    // `Effect::SetQuestVariable` joined the saved `Effect` enum.
    //
    // v38 -> v39 (M47.2) — refreshed WITH a major bump: the fragment
    // `Effect::AddItem` container widened `ObjectRef` -> `ActorRef` (the
    // `Game.GetPlayer().AddItem` reward shape) and `Effect::RemoveItem`
    // joined the enum; queued fragment tails serialize `Effect`s.
    //
    // v25 -> v26 (#4282/#4334) — refreshed WITH a major bump, both
    // directions at once: `Material` gained the optional capture-only
    // `wetness`/`luminance` fields (the NIFAL sink for the
    // BSSPWetnessParams/BSSPLuminanceParams envelopes; `sanitize_finite`
    // descends into both, so a poisoned value cannot survive a restore),
    // and `ReferenceScriptState` joined the saved registry as a NEW
    // resource (the once-only-script ledger) — its absence in pre-v26
    // snapshots is what keeps an `onlyOnce` trigger re-armed across a
    // load from being possible.
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

/// #4612 — the quest resources' `revision` change key is transient: it is
/// never serialized, and a v26 document (which has no such key) still
/// decodes. This is what lets the shape fingerprint above move without a
/// FORMAT_MAJOR bump.
#[test]
fn quest_revision_keys_never_reach_a_save() {
    use byroredux_scripting::quest_stages::{QuestObjectiveState, QuestStageState};
    use byroredux_scripting::QuestFormId;

    let quest = QuestFormId(0x1000);
    let mut stages = QuestStageState::default();
    stages.start_quest(quest, Some(10));
    let mut objectives = QuestObjectiveState::default();
    objectives.set_displayed(quest, 10, true);

    let stages_json = serde_json::to_value(&stages).expect("serialize stages");
    let objectives_json = serde_json::to_value(&objectives).expect("serialize objectives");
    for json in [&stages_json, &objectives_json] {
        assert!(
            json.get("revision").is_none(),
            "revision must stay out of the save: {json}"
        );
    }

    let restored: QuestStageState =
        serde_json::from_value(stages_json).expect("a revision-less document decodes");
    assert_eq!(restored.get_stage(quest), 10);
    assert_ne!(
        restored.revision(),
        stages.revision(),
        "a loaded resource starts with a fresh revision"
    );
    let restored: QuestObjectiveState =
        serde_json::from_value(objectives_json).expect("a revision-less document decodes");
    assert!(restored.get(quest, 10).displayed);
}

/// #4322 — `SetInChargen`'s flags were renamed to Skyrim's declared
/// parameter names. Both saved carriers — the resource and the queued
/// `Effect` — must still decode a v23 save written under the invented
/// names, into the same values, through their `serde(alias)`es.
#[test]
fn set_in_chargen_renames_still_decode_v23_keys() {
    use byroredux_scripting::translate::effects::Effect;
    use byroredux_scripting::CinematicPresentationState;

    fn rename_keys(
        object: &mut serde_json::Map<String, serde_json::Value>,
        pairs: [(&str, &str); 3],
    ) {
        for (new, old) in pairs {
            let value = object
                .remove(new)
                .unwrap_or_else(|| panic!("`{new}` serialized"));
            object.insert(old.to_owned(), value);
        }
    }

    let mut state = CinematicPresentationState::default();
    state.set_in_chargen(true, false, true);
    let mut json = serde_json::to_value(&state).unwrap();
    rename_keys(
        json.as_object_mut().unwrap(),
        [
            ("disable_saving", "in_chargen"),
            ("disable_waiting", "chargen_wait_for_race_sex"),
            (
                "show_controls_disabled_message",
                "chargen_stay_in_first_person",
            ),
        ],
    );
    let decoded: CinematicPresentationState = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, state);

    let effect = Effect::SetInChargen {
        disable_saving: true,
        disable_waiting: false,
        show_controls_disabled_message: true,
    };
    let mut json = serde_json::to_value(&effect).unwrap();
    rename_keys(
        json.get_mut("SetInChargen")
            .and_then(serde_json::Value::as_object_mut)
            .expect("externally tagged"),
        [
            ("disable_saving", "enabled"),
            ("disable_waiting", "wait_for_race_sex"),
            ("show_controls_disabled_message", "stay_in_first_person"),
        ],
    );
    let decoded: Effect = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, effect);
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
                if enclosing_declaration(&src, offset)
                    .is_some_and(|name| NOT_SAVED_SERDE_DEFAULT_DECLARATIONS.contains(&name))
                {
                    continue;
                }
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
fn not_saved_serde_default_exemptions_are_still_declared() {
    let sources = save_type_sources();
    for name in NOT_SAVED_SERDE_DEFAULT_DECLARATIONS {
        assert!(
            sources.iter().any(|path| {
                std::fs::read_to_string(path).is_ok_and(|src| defines_type(&src, name))
            }),
            "NOT_SAVED_SERDE_DEFAULT_DECLARATIONS names {name}, which no scanned file \
             declares any more — drop the stale exemption"
        );
    }
}

#[test]
fn enclosing_declaration_names_the_owning_type() {
    let src = "pub struct A {\n    #[serde(default)]\n    x: u8,\n}\n\nenum B {\n    #[serde(default)]\n    V,\n}\n";
    let a = src.find("#[serde").unwrap();
    assert_eq!(enclosing_declaration(src, a), Some("A"));
    let b = src.rfind("#[serde").unwrap();
    assert_eq!(enclosing_declaration(src, b), Some("B"));
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
        "core/src/lighting.rs",
        "sdk/src/identity.rs",
        "sdk/src/script_function.rs",
    ] {
        assert!(
            sources.iter().any(|path| path.ends_with(suffix)),
            "save schema discovery must include nested/non-turbofish payload source {suffix}"
        );
    }
    assert!(!sources.iter().any(|path| path.ends_with("settings_io.rs")));
}

/// Regression: #4141 — the shape/serde-default guards' discovery roots
/// must be the live workspace enumeration, not a hardcoded list.
///
/// This file scanned exactly five roots (`byroredux/src` plus core /
/// plugin / scripting / physics) while its sibling
/// `registry_completeness_tests.rs` had already abandoned that approach
/// under #3497 — the static list there "has no defense against a root
/// never being added", and replacing it immediately surfaced four real
/// types in crates the old list never reached.
///
/// Asserted against [`save_scan_candidates`] rather than the filtered
/// output on purpose: a root that is walked but holds nothing
/// save-relevant is indistinguishable, downstream of the filter, from a
/// root that was never walked. That ambiguity is the whole defect.
#[test]
fn save_type_discovery_walks_every_workspace_crate() {
    use super::registry_completeness_tests::discover_scan_roots;

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidates = save_scan_candidates();
    let normalize = |p: &Path| p.to_string_lossy().replace('\\', "/");

    for root in discover_scan_roots(manifest) {
        let root_str = normalize(&root);
        assert!(
            candidates.iter().any(|c| normalize(c).starts_with(&root_str)),
            "scan root {root_str} yielded no candidate file — save_type_sources is not walking every discovered root"
        );
    }

    // The four roots the old hardcoded list happened to name must still be
    // covered, and `crates/sdk` — the crate #3497 caught unscanned, and the
    // one this issue names as a plausible future home for a saved type —
    // must now be reachable too.
    for expected in [
        "byroredux/src/",
        "crates/core/src/",
        "crates/plugin/src/",
        "crates/scripting/src/",
        "crates/physics/src/",
        "crates/sdk/src/",
    ] {
        assert!(
            candidates.iter().any(|c| normalize(c).contains(expected)),
            "{expected} missing from the save-schema scan candidates"
        );
    }
}

/// Regression: #4141. `defines_type` matched on a bare `contains`, so
/// `struct Inventory` also matched `struct InventorySnapshot` and
/// `struct Material` matched `MaterialKind` / `MaterialTable` /
/// `MaterialValue`.
///
/// Harmless while five roots were scanned. Once discovery covers the
/// whole workspace it inverts into a false-positive source: seventeen
/// unrelated files across `crates/{bgsm,debug-ui,nif,renderer,sdk,ui}`
/// and `byroredux/src` match a registered name only by prefix, and a
/// `#[serde(default)]` added to any of them would fail the guard and
/// demand a `FORMAT_MAJOR` bump for a type that never enters a save.
#[test]
fn defines_type_requires_a_whole_identifier_match() {
    assert!(defines_type("pub struct Inventory {", "Inventory"));
    assert!(defines_type("struct Inventory;", "Inventory"));
    assert!(defines_type("pub enum Material<T> {", "Material"));

    assert!(!defines_type("pub struct InventorySnapshot {", "Inventory"));
    assert!(!defines_type("pub struct MaterialKind;", "Material"));
    assert!(!defines_type("struct TransformChannel {", "Transform"));
    assert!(!defines_type("pub struct NameIndex {", "Name"));

    // A prefix collision must not mask a real declaration in the same file.
    assert!(defines_type(
        "pub struct InventorySnapshot {}\npub struct Inventory {}",
        "Inventory"
    ));
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

