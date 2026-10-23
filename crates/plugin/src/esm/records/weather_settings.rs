//! WTHS (Weather Settings) record parser — Starfield's replacement for
//! the WTHR weather type (#5364).
//!
//! A WTHS is an `EDID` + BGS-parameter-blob pair, not the WTHR
//! sub-record schema. The blob rides the `REFL` sub-record on root
//! ("template") records and the `RDIF` (Reflection Diff) sub-record on
//! child records that point up an `RFDP` (Reflection Parent) chain:
//! vanilla `Starfield.esm` ships 181 WTHS — 18 roots with a full REFL
//! (`WeatherClearTemplate`, `DefaultWeatherSettings`, …) and 163
//! children carrying an RFDP + RDIF (e.g. Akila's
//! `WeatherUniqueAkila_Clear_C0_Clear` → `Weather_Clear_C0_Clear` →
//! `WeatherClearTemplate`).
//!
//! The container grammar and its decoder live in [`super::reflection`]; this
//! module is the WTHS record on top of it — the `RFDP` parent chain, the diff
//! application and the synthesis of a WTHR-shaped weather from the value tree.

use super::common::{read_zstring, remap_fid};
use super::reflection::{parse_bgs_reflection_with_remap, BgsReflection};
use super::weather::WeatherRecord;
use super::weather_settings_instance::{DiffOverride, ReflectionObject, ReflectionValue};
use crate::esm::reader::{FormIdRemap, SubRecord};
use crate::esm::sub_reader::SubReader;
use std::collections::HashMap;

/// Parsed WTHS record.
#[derive(Debug, Clone, Default)]
pub struct WeatherSettingsRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// The `REFL` container — full reflection, on root (template)
    /// records. `None` on children, which carry `reflection_diff` +
    /// `reflection_parent` instead.
    pub reflection: Option<BgsReflection>,
    /// The `RDIF` container — same grammar, plus a `DIFF` chunk naming
    /// the overridden fields. `None` on roots.
    pub reflection_diff: Option<BgsReflection>,
    /// `RFDP` — the parent WTHS this record diffs against. Load-order
    /// remapped like every cross-record FormID. `None` on roots.
    pub reflection_parent: Option<u32>,
}

/// Parse a WTHS record from its sub-records. Starfield-only; the label
/// is not routed here for other games (their `WTHR` arm stays the
/// weather path).
pub fn parse_wths(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<FormIdRemap>,
) -> WeatherSettingsRecord {
    let mut record = WeatherSettingsRecord {
        form_id,
        ..WeatherSettingsRecord::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),
            // RFDP — the parent WTHS this record diffs against.
            // Cross-record FormID: load-order remapped (#4066 pattern)
            // so `index.weather_settings.get(parent)` resolves.
            b"RFDP" if sub.data.len() >= 4 => {
                record.reflection_parent = SubReader::new(&sub.data)
                    .u32()
                    .ok()
                    .map(|raw| remap_fid(raw, remap));
            }
            b"REFL" => {
                record.reflection = Some(parse_bgs_reflection_with_remap(&sub.data, remap))
            }
            b"RDIF" => {
                record.reflection_diff =
                    Some(parse_bgs_reflection_with_remap(&sub.data, remap))
            }
            _ => {}
        }
    }
    record
}

// ── synthesis: WTHS value tree → WTHR-shaped weather (#5627) ─────────

/// Walk `record`'s `RFDP` chain (self first, root last), cycle-guarded.
/// `None` when a parent dangles — the record keeps no synthesized
/// weather and the climate side falls back.
fn resolve_wths_chain<'a>(
    record: &'a WeatherSettingsRecord,
    settings: &'a HashMap<u32, WeatherSettingsRecord>,
) -> Option<Vec<&'a WeatherSettingsRecord>> {
    let mut chain = vec![record];
    let mut visited: std::collections::HashSet<u32> =
        std::iter::once(record.form_id).collect();
    let mut cur = record;
    while let Some(parent_id) = cur.reflection_parent {
        if !visited.insert(parent_id) {
            return None; // cycle — the chain is malformed, decline
        }
        let parent = settings.get(&parent_id)?;
        chain.push(parent);
        cur = parent;
    }
    chain.reverse(); // root … self
    Some(chain)
}

/// Apply one record's DIFF overrides onto `tree` by field name. A
/// struct override merges into the existing subtree recursively (diff
/// semantics propagate into overridden structs, #5514); anything else
/// replaces.
fn apply_diff_overrides(tree: &mut ReflectionObject, overrides: &[DiffOverride]) {
    for o in overrides {
        apply_one_override(tree, &o.field_name, &o.value);
    }
}

fn apply_one_override(tree: &mut ReflectionObject, name: &str, value: &ReflectionValue) {
    let Some(idx) = tree.fields.iter().position(|(n, _)| n == name) else {
        // The parent left this field unauthored — the override adds it.
        tree.fields.push((name.to_string(), value.clone()));
        return;
    };
    match (&mut tree.fields[idx].1, value) {
        // Both subtrees are objects of the same class — merge
        // field-wise so un-overridden struct fields keep the parent's
        // values.
        (ReflectionValue::Object(existing_obj), ReflectionValue::Object(override_obj))
            if existing_obj.class == override_obj.class =>
        {
            for (name, value) in &override_obj.fields {
                apply_one_override(existing_obj, name, value);
            }
        }
        (slot, value) => *slot = value.clone(),
    }
}

/// Map an effective WTHS value tree onto the engine's weather record —
/// the fields whose engine counterparts are grounded by the decoded
/// corpus (see the per-field notes). Everything else stays at the
/// decoder defaults, exactly like any other game's unauthored WTHR.
///
/// - `pImageSpace` / `pImageSpaceNight` → [`WeatherRecord::image_spaces`]:
///   the WTHR `IMSP` table is [Sunrise, Day, Sunset, Night]; WTHS
///   authors two slots, so the day space fills the three day-side slots
///   and the night space fills Night (nearest-slot lift).
/// - `Colors.Sun` / `Colors.Sunlight` / `Colors.FogNear` → the WTHR
///   sky-color groups of the same names ([`SKY_SUN`], [`SKY_SUNLIGHT`],
///   [`SKY_FOG`]), uniform across all six time-of-day slots — WTHS
///   authors no TOD variation. The remaining WTHS colors (FogFar/High,
///   Moon*, EffectLighting, SunGlare) have no base WTHR group; they are
///   skipped rather than guessed at.
///
/// Fog distances, wind and transition fields are NOT lifted: the WTHS
/// schema authors no fog distances (its Fog* fields are colors), and
/// the wind/transition floats' units are not grounded in any shipped
/// reader — leaving them at the decoder defaults keeps the record on
/// the same path as every unauthored WTHR.
fn weather_record_from_values(
    form_id: u32,
    editor_id: &str,
    tree: &ReflectionObject,
) -> Option<WeatherRecord> {
    use crate::esm::records::weather::{SkyColor, SKY_FOG, SKY_SUN, SKY_SUNLIGHT};

    if tree.class != "BGSWeatherSettingsForm" {
        return None; // not a weather root — decline rather than guess
    }

    let mut wthr = WeatherRecord {
        form_id,
        editor_id: editor_id.to_string(),
        ..WeatherRecord::default()
    };

    // Per-weather image spaces.
    let day = tree.field("pImageSpace").and_then(ReflectionValue::as_form_id);
    let night = tree
        .field("pImageSpaceNight")
        .and_then(ReflectionValue::as_form_id);
    if day.is_some() || night.is_some() {
        wthr.image_spaces = [
            day.or(night), // Sunrise
            day,           // Day
            day.or(night), // Sunset
            night,         // Night
        ];
    }

    // Sky colors: named-group lift, uniform across the TOD slots.
    let color_of = |group: &str| -> Option<[u8; 4]> {
        let v = tree
            .field("Colors")?
            .as_object()?
            .field(group)?
            .as_object()?
            .field("Value")?
            .as_object()?;
        let ch = |name: &str| -> Option<u8> {
            let x = v.field(name)?.as_f32()?;
            Some((x.clamp(0.0, 1.0) * 255.0).round() as u8)
        };
        Some([ch("x")?, ch("y")?, ch("z")?, ch("w")?])
    };
    for (group, slot) in [
        ("Sun", SKY_SUN),
        ("Sunlight", SKY_SUNLIGHT),
        ("FogNear", SKY_FOG),
    ] {
        if let Some([r, g, b, a]) = color_of(group) {
            wthr.sky_colors[slot] = std::array::from_fn(|_| SkyColor { r, g, b, a });
        }
    }

    Some(wthr)
}

/// Synthesize the WTHR-shaped weather this record resolves to: its
/// RFDP chain walked to the root template, the root's instance values
/// merged with every descendant's DIFF overrides, the result mapped
/// onto [`WeatherRecord`]. `None` when the chain dangles/cycles, the
/// root's instance decode declined, or the tree is not a weather root —
/// the caller keeps no synthesized weather (the climate side falls
/// back, see `resolve_default_weather`).
pub fn synthesize_weather_record(
    record: &WeatherSettingsRecord,
    settings: &HashMap<u32, WeatherSettingsRecord>,
) -> Option<WeatherRecord> {
    let chain = resolve_wths_chain(record, settings)?;
    let root = chain.first()?;
    let root_reflection = root.reflection.as_ref()?;
    let mut tree = root_reflection.root_instance.clone()?;

    // Apply overrides root → self (chain[0] is the root template; its
    // REFL has no DIFF).
    for r in chain.iter().skip(1) {
        if let Some(overrides) = r
            .reflection_diff
            .as_ref()
            .and_then(|d| d.diff_values.as_ref())
        {
            apply_diff_overrides(&mut tree, overrides);
        }
    }

    weather_record_from_values(record.form_id, &record.editor_id, &tree)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::reflection::{
        builtin, builtin_type_label, parse_bgs_reflection, resolve_ref, BUILTINS,
    };
    use crate::esm::records::test_support::sub;
    use crate::esm::records::weather_settings_instance::ReflectionValue;

    /// The builtin ids are the on-disk `0xFFFFFF01..=0xFFFFFF12` tags that
    /// `byroredux-sfmaterial::BuiltinType` names (that crate is not a
    /// dependency of this one, #5615), as the same bit patterns viewed as
    /// i32. Pins the single [`BUILTINS`] table against the documented set.
    #[test]
    fn builtin_table_is_the_documented_tag_set() {
        let tags: Vec<u32> = BUILTINS.iter().map(|(id, _, _)| *id as u32).collect();
        assert_eq!(
            tags,
            [
                0xFFFFFF01, 0xFFFFFF02, 0xFFFFFF03, 0xFFFFFF04, 0xFFFFFF05, 0xFFFFFF08,
                0xFFFFFF09, 0xFFFFFF0A, 0xFFFFFF0B, 0xFFFFFF0C, 0xFFFFFF0D, 0xFFFFFF0E,
                0xFFFFFF0F, 0xFFFFFF10, 0xFFFFFF11, 0xFFFFFF12,
            ]
        );
        assert_eq!(builtin_type_label(-243), Some("UInt32"));
        assert_eq!(builtin_type_label(-249), None, "gap ids stay unknown");
        let mut labels: Vec<&str> = BUILTINS.iter().map(|(_, l, _)| *l).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), BUILTINS.len(), "labels are unique");
    }

    /// Hand-built reflection container exercising every decoded chunk
    /// kind, in authored order: BETH → STRT → TYPE → CLAS → OBJT.
    #[test]
    fn reflection_container_decodes_schema_and_census() {
        let mut blob: Vec<u8> = Vec::new();
        // BETH: version 4, 5 chunks.
        blob.extend_from_slice(b"BETH");
        blob.extend_from_slice(&8u32.to_le_bytes());
        blob.extend_from_slice(&4u32.to_le_bytes());
        blob.extend_from_slice(&5u32.to_le_bytes());
        // STRT: "BGSWeatherSettingsForm\0FogNear\0FloatValue\0"
        let strs = b"BGSWeatherSettingsForm\0FogNear\0FloatValue\0";
        let fog_near_off: i32 = 24; // after the root name + nul
        let float_value_off: i32 = fog_near_off + 8;
        blob.extend_from_slice(b"STRT");
        blob.extend_from_slice(&(strs.len() as u32).to_le_bytes());
        blob.extend_from_slice(strs);
        // TYPE: one class.
        blob.extend_from_slice(b"TYPE");
        blob.extend_from_slice(&4u32.to_le_bytes());
        blob.extend_from_slice(&1u32.to_le_bytes());
        // CLAS: root class, one FogNear: Float field at offset 0 size 4.
        let mut clas = Vec::new();
        clas.extend_from_slice(&0i32.to_le_bytes()); // name → strtab 0
        clas.extend_from_slice(&0i32.to_le_bytes()); // type
        clas.extend_from_slice(&0u16.to_le_bytes()); // flags
        clas.extend_from_slice(&1u16.to_le_bytes()); // field count
        clas.extend_from_slice(&fog_near_off.to_le_bytes());
        clas.extend_from_slice(&float_value_off.to_le_bytes());
        clas.extend_from_slice(&0u16.to_le_bytes()); // offset
        clas.extend_from_slice(&4u16.to_le_bytes()); // size
        blob.extend_from_slice(b"CLAS");
        blob.extend_from_slice(&(clas.len() as u32).to_le_bytes());
        blob.extend_from_slice(&clas);
        // OBJT: instance chunk, censused not interpreted.
        blob.extend_from_slice(b"OBJT");
        blob.extend_from_slice(&4u32.to_le_bytes());
        blob.extend_from_slice(&[0, 0, 0, 0]);

        let refl = parse_bgs_reflection(&blob);
        assert_eq!(refl.version, 4);
        assert_eq!(refl.chunk_count, 5);
        assert_eq!(refl.strings.len(), 3);
        assert_eq!(refl.class_count, 1);
        assert_eq!(refl.classes.len(), 1);
        let root = refl.weather_root_class().expect("root class resolved");
        assert_eq!(root.name, "BGSWeatherSettingsForm");
        assert_eq!(root.fields.len(), 1);
        assert_eq!(root.fields[0].name, "FogNear");
        assert_eq!(root.fields[0].type_name, "FloatValue");
        assert_eq!((root.fields[0].offset, root.fields[0].size), (0, 4));
        assert_eq!(refl.instance_chunks, vec![(*b"OBJT", 4)]);
        assert!(refl.unknown_chunks.is_empty());
        assert!(refl.diff_values.is_none());
        // The instance half declines rather than guesses: the OBJT body
        // holds only the type ref, but the class declares a FogNear
        // field — the walk truncates, so the root stays `None` and the
        // census is the evidence (the pre-#5627 contract, preserved).
        assert_eq!(refl.unconsumed_instance_chunks, 0);
        assert!(refl.root_instance.is_none());
    }

    /// #5514 — a `DIFF` chunk is an object diff, not a flat index list:
    /// each overridden field carries its inline value (skipped by field
    /// type — `String` by length prefix, built-ins by size, structs as
    /// nested diff-pair streams), and the walk stops at `0xFFFF`. The
    /// pre-fix reader treated every `u16` as an index, so value bytes
    /// and the terminator polluted `diff_field_indices`.
    #[test]
    fn diff_chunk_decodes_index_value_pairs_to_the_terminator() {
        // Class layout (strtab offsets): "Root" @0, "Struct" @5, plus
        // field names/types. Fields:
        //   0 Name:  String   (-254)
        //   1 Count: UInt32   (-243)
        //   2 Nested: Struct  (class ref = name offset 5, non-user)
        //   3 Bits:  Bool     (-240)
        // Strtab offsets: Root@0, Struct@5, Name@12, Count@17,
        // Nested@23, Bits@30 (each string + its NUL).
        let strs = b"Root\0Struct\0Name\0Count\0Nested\0Bits\0";
        let name_off = 0i32;
        let struct_off = 5i32;
        let name_str = 12i32;
        let count_str = 17i32;
        let nested_str = 23i32;
        let bits_str = 30i32;
        let field = |n: i32, t: i32, o: u16, s: u16| {
            let mut v = Vec::new();
            v.extend_from_slice(&n.to_le_bytes());
            v.extend_from_slice(&t.to_le_bytes());
            v.extend_from_slice(&o.to_le_bytes());
            v.extend_from_slice(&s.to_le_bytes());
            v
        };
        let mut clas = Vec::new();
        clas.extend_from_slice(&name_off.to_le_bytes()); // Root's name offset
        clas.extend_from_slice(&0i32.to_le_bytes()); // type
        clas.extend_from_slice(&0u16.to_le_bytes()); // flags (not User)
        clas.extend_from_slice(&4u16.to_le_bytes()); // field count
        clas.extend(field(name_str, -254, 0, 0)); // Name: String
        clas.extend(field(count_str, -243, 0, 4)); // Count: UInt32
        clas.extend(field(nested_str, struct_off, 0, 0)); // Nested: Struct
        clas.extend(field(bits_str, -240, 4, 1)); // Bits: Bool
        let mut struct_clas = Vec::new();
        struct_clas.extend_from_slice(&struct_off.to_le_bytes()); // Struct's name offset
        struct_clas.extend_from_slice(&0i32.to_le_bytes());
        struct_clas.extend_from_slice(&0u16.to_le_bytes()); // not User → inline
        struct_clas.extend_from_slice(&1u16.to_le_bytes());
        struct_clas.extend(field(name_str, -243, 0, 4)); // Name (index 0): UInt32

        // DIFF body: type ref = Root's name offset, then pairs:
        //   0 → String len 3 "abc"
        //   1 → u32 0x11223344 (a value whose low u16 bytes would have
        //       been misread as indices pre-fix)
        //   2 → nested struct diff: 0 → u32, 0xFFFF
        //   3 → bool 1
        //   0xFFFF terminator
        let mut diff = Vec::new();
        diff.extend_from_slice(&name_off.to_le_bytes());
        diff.extend_from_slice(&0u16.to_le_bytes());
        diff.extend_from_slice(&3u16.to_le_bytes());
        diff.extend_from_slice(b"abc");
        diff.extend_from_slice(&1u16.to_le_bytes());
        diff.extend_from_slice(&0x1122_3344u32.to_le_bytes());
        diff.extend_from_slice(&2u16.to_le_bytes());
        diff.extend_from_slice(&0u16.to_le_bytes()); // nested pair index
        diff.extend_from_slice(&0x5566_7788u32.to_le_bytes()); // nested value
        diff.extend_from_slice(&0xFFFFu16.to_le_bytes()); // nested terminator
        diff.extend_from_slice(&3u16.to_le_bytes());
        diff.push(1u8); // bool
        diff.extend_from_slice(&0xFFFFu16.to_le_bytes()); // terminator

        let mut blob: Vec<u8> = Vec::new();
        let chunk = |sig: &[u8; 4], payload: &[u8], out: &mut Vec<u8>| {
            out.extend_from_slice(sig);
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(payload);
        };
        chunk(b"BETH", &4u32.to_le_bytes().iter().chain(&5u32.to_le_bytes()).copied().collect::<Vec<u8>>(), &mut blob);
        chunk(b"STRT", strs, &mut blob);
        chunk(
            b"TYPE",
            1u32.to_le_bytes().as_ref(),
            &mut blob,
        );
        chunk(b"CLAS", &clas, &mut blob);
        chunk(b"CLAS", &struct_clas, &mut blob);
        chunk(b"DIFF", &diff, &mut blob);
        // A side chunk the DIFF's chunk-typed fields would own — censused.
        chunk(b"OBJT", &[0; 4], &mut blob);

        let refl = parse_bgs_reflection(&blob);
        assert_eq!(
            refl.diff_values
                .as_ref()
                .map(|v| v.iter().map(|o| o.field_index).collect::<Vec<_>>()),
            Some(vec![0, 1, 2, 3]),
            "only the authored field indices — value bytes and the \
             terminator must not pollute the list"
        );
        // #5627 — the DIFF now carries VALUES: four overrides in
        // authored order, each bound to its field name (the side-chunk
        // OBJT belongs to no authored field here, so the walk leaves it
        // unconsumed rather than binding it).
        let values = refl.diff_values.as_ref().expect("DIFF decodes");
        assert_eq!(values.len(), 4);
        assert_eq!(values[0].field_name, "Name");
        assert_eq!(values[0].value, ReflectionValue::String("abc".into()));
        assert_eq!(values[1].field_name, "Count");
        assert_eq!(values[1].value, ReflectionValue::U32(0x1122_3344));
        // The nested struct override (field 2) is a partial object: its
        // single pair overrides the struct's field 0.
        assert_eq!(
            values[2].value.as_object().expect("struct override").field("Name"),
            Some(&ReflectionValue::U32(0x5566_7788)),
            "diff semantics propagate into overridden structs (#5514)"
        );
        assert!(refl.unknown_chunks.is_empty());
        // USRD/MAPC are recognized instance chunks, not unknowns (#5514).
        let refl_usrd = parse_bgs_reflection(
            &{
                let mut b = blob.clone();
                b.extend_from_slice(b"USRD");
                b.extend_from_slice(&8u32.to_le_bytes());
                b.extend_from_slice(&[0u8; 8]);
                b
            },
        );
        assert_eq!(refl_usrd.instance_chunks.len(), 2);
        assert!(refl_usrd.unknown_chunks.is_empty());
    }

    /// #5687 — hostile `RDIF` shapes: a class whose field names the class
    /// itself (the struct edge) and a `Ref` whose target is `Ref` again
    /// (the Ref edge). Each nesting level consumes only 2–4 payload bytes,
    /// so an unbounded walk recurses ~payload/2 deep and aborts the process
    /// with a stack overflow; both edges now decline at
    /// [`MAX_NESTING_DEPTH`], leaving `diff_field_indices` unset. Parsed on
    /// a deliberately small thread so a regression aborts here, not during
    /// a production load.
    #[test]
    fn hostile_self_referential_diffs_decline_at_the_depth_cap() {
        let parse = |field_type: i32, value_bytes: Vec<u8>| {
            // One class named at strtab offset 0 ("C"), not user-flagged,
            // with the single field the DIFF payload exercises.
            let mut clas = Vec::new();
            clas.extend_from_slice(&0i32.to_le_bytes()); // name → strtab 0
            clas.extend_from_slice(&0i32.to_le_bytes()); // type
            clas.extend_from_slice(&0u16.to_le_bytes()); // flags (not User)
            clas.extend_from_slice(&1u16.to_le_bytes()); // field count
            clas.extend_from_slice(&0i32.to_le_bytes()); // field name → 0
            clas.extend_from_slice(&field_type.to_le_bytes());
            clas.extend_from_slice(&0u16.to_le_bytes()); // offset
            clas.extend_from_slice(&0u16.to_le_bytes()); // size
            let mut diff = Vec::new();
            diff.extend_from_slice(&0i32.to_le_bytes()); // DIFF names the class
            diff.extend_from_slice(&value_bytes);

            let mut blob: Vec<u8> = Vec::new();
            let chunk = |sig: &[u8; 4], payload: &[u8], out: &mut Vec<u8>| {
                out.extend_from_slice(sig);
                out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                out.extend_from_slice(payload);
            };
            chunk(
                b"BETH",
                &4u32.to_le_bytes().iter().chain(&5u32.to_le_bytes()).copied().collect::<Vec<u8>>(),
                &mut blob,
            );
            chunk(b"STRT", b"C\0", &mut blob);
            chunk(b"TYPE", 1u32.to_le_bytes().as_ref(), &mut blob);
            chunk(b"CLAS", &clas, &mut blob);
            chunk(b"DIFF", &diff, &mut blob);

            std::thread::Builder::new()
                .stack_size(256 * 1024)
                .spawn(move || parse_bgs_reflection(&blob))
                .expect("spawn")
                .join()
                .expect("no panic")
        };

        // Struct edge: field 0 names the class itself; every nested level
        // re-enters the struct arm consuming one u16 field index.
        let mut struct_body = Vec::new();
        for _ in 0..100_000 {
            struct_body.extend_from_slice(&0u16.to_le_bytes());
        }
        assert!(
            parse(0, struct_body).diff_values.is_none(),
            "self-referential struct declines at the cap, not a stack overflow"
        );

        // Ref edge: a Ref field whose inline target is Ref again; every
        // level consumes one i32 target.
        let mut ref_body = vec![0u8, 0]; // DIFF pair index 0
        for _ in 0..100_000 {
            ref_body.extend_from_slice(&builtin::REF.to_le_bytes());
        }
        assert!(
            parse(builtin::REF, ref_body).diff_values.is_none(),
            "self-referential Ref chain declines at the cap, not a stack overflow"
        );
    }

    /// Built-in type refs (negative i32) resolve to their labels; the
    /// String id is `0xFFFFFF02` = -254 as authored, and an unlisted
    /// negative keeps its hex for diagnostics.
    #[test]
    fn builtin_refs_label_and_unlisted_negatives_kept_as_hex() {
        let table = vec![(0usize, "Named".to_string())];
        assert_eq!(resolve_ref(-254, &table), "String");
        assert_eq!(resolve_ref(-243, &table), "UInt32");
        assert_eq!(resolve_ref(-239, &table), "Float");
        assert_eq!(resolve_ref(-240, &table), "Bool");
        assert_eq!(resolve_ref(-100, &table), "<builtin 0xffffff9c>");
        assert_eq!(resolve_ref(0, &table), "Named");
        assert_eq!(resolve_ref(3, &table), "Named", "mid-string offsets fall back to the containing entry");
    }

    /// An overflowing chunk size ends the walk with the decoded prefix
    /// intact — no panic, no silent wraparound.
    #[test]
    fn overflowing_chunk_stops_the_walk() {
        let mut blob: Vec<u8> = Vec::new();
        blob.extend_from_slice(b"BETH");
        blob.extend_from_slice(&8u32.to_le_bytes());
        blob.extend_from_slice(&4u32.to_le_bytes());
        blob.extend_from_slice(&2u32.to_le_bytes());
        blob.extend_from_slice(b"OBJT");
        blob.extend_from_slice(&0xFFFF_0000u32.to_le_bytes()); // overflows
        let refl = parse_bgs_reflection(&blob);
        assert_eq!(refl.version, 4);
        assert!(refl.instance_chunks.is_empty());
    }

    // ── #5627 synthesis fixtures ─────────────────────────────────────
    //
    // A miniature corpus-shaped root template: Root{pImageSpace: Ref,
    // pImageSpaceNight: Ref, Colors: ColorSettings{Sun:
    // ColorValue{Op, Value: XMFLOAT4, BlendAmount}}}, plus a child RDIF
    // overriding pImageSpace. STRT offsets: Root@0, ColorSettings@5,
    // ColorValue@19, XMFLOAT4@30, Op@39, Value@42, BlendAmount@48,
    // pImageSpace@60, pImageSpaceNight@72.

    fn synth_chunk(sig: &[u8; 4], payload: &[u8], out: &mut Vec<u8>) {
        out.extend_from_slice(sig);
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(payload);
    }

    fn synth_refl_field(name: i32, type_ref: i32, off: u16, size: u16) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&name.to_le_bytes());
        v.extend_from_slice(&type_ref.to_le_bytes());
        v.extend_from_slice(&off.to_le_bytes());
        v.extend_from_slice(&size.to_le_bytes());
        v
    }

    fn synth_clas(name: i32, flags: u16, fields: &[Vec<u8>]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&name.to_le_bytes());
        v.extend_from_slice(&0i32.to_le_bytes()); // type
        v.extend_from_slice(&flags.to_le_bytes());
        v.extend_from_slice(&(fields.len() as u16).to_le_bytes());
        for f in fields {
            v.extend_from_slice(f);
        }
        v
    }

    /// The shared schema blob: STRT + TYPE + the four classes.
    fn synth_schema() -> Vec<u8> {
        // x/y/z/w trail the named strings so their offsets exist: the
        // XMFLOAT4 leaves bind by name like every other field.
        let strs: Vec<u8> = b"BGSWeatherSettingsForm\0ColorSettings\0ColorValue\0\
XMFLOAT4\0Op\0Value\0BlendAmount\0pImageSpace\0pImageSpaceNight\0x\0y\0z\0w\0Colors\0Sun\0"
            .to_vec();
        let mut blob = Vec::new();
        synth_chunk(
            b"BETH",
            &4u32.to_le_bytes().iter().chain(&4u32.to_le_bytes()).copied().collect::<Vec<u8>>(),
            &mut blob,
        );
        synth_chunk(b"STRT", &strs, &mut blob);
        synth_chunk(b"TYPE", 4u32.to_le_bytes().as_ref(), &mut blob);
        synth_chunk(
            b"CLAS",
            &synth_clas(
                0,
                0,
                &[
                    synth_refl_field(78, -251, 280, 8),  // pImageSpace: Ref
                    synth_refl_field(90, -251, 288, 8),  // pImageSpaceNight: Ref
                    synth_refl_field(115, 23, 352, 240), // Colors: ColorSettings class
                ],
            ),
            &mut blob,
        );
        synth_chunk(
            b"CLAS",
            &synth_clas(23, 0, &[synth_refl_field(122, 37, 0, 24)]), // Sun: ColorValue class
            &mut blob,
        );
        synth_chunk(
            b"CLAS",
            &synth_clas(
                37,
                0,
                &[
                    synth_refl_field(57, -254, 0, 4),   // Op: String
                    synth_refl_field(60, 48, 4, 16),    // Value: XMFLOAT4
                    synth_refl_field(66, -239, 20, 4),  // BlendAmount: Float
                ],
            ),
            &mut blob,
        );
        synth_chunk(
            b"CLAS",
            &synth_clas(
                48,
                0,
                &[
                    synth_refl_field(107, -239, 0, 4), // x
                    synth_refl_field(109, -239, 4, 4), // y
                    synth_refl_field(111, -239, 8, 4), // z
                    synth_refl_field(113, -239, 12, 4), // w
                ],
            ),
            &mut blob,
        );
        blob
    }

    /// The root template's OBJT: refs to two form ids and a Sun color
    /// (1.0, 0.5, 0.25, w 1.0) under Op "Day" / BlendAmount 1.
    fn synth_objt() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&0i32.to_le_bytes()); // type ref = Root
        v.extend_from_slice(&(-243i32).to_le_bytes()); // pImageSpace: ref → UInt32
        v.extend_from_slice(&0x0101_2345u32.to_le_bytes());
        v.extend_from_slice(&(-243i32).to_le_bytes()); // pImageSpaceNight
        v.extend_from_slice(&0x0101_2346u32.to_le_bytes());
        // Colors (ColorSettings inline) → Sun (ColorValue inline):
        v.extend_from_slice(&4u16.to_le_bytes()); // Op: len 4
        v.extend_from_slice(b"Day\0");
        v.extend_from_slice(&1.0f32.to_le_bytes()); // Value.x
        v.extend_from_slice(&0.5f32.to_le_bytes()); // Value.y
        v.extend_from_slice(&0.25f32.to_le_bytes()); // Value.z
        v.extend_from_slice(&1.0f32.to_le_bytes()); // Value.w
        v.extend_from_slice(&1.0f32.to_le_bytes()); // BlendAmount
        v
    }

    fn synth_root_record(form_id: u32) -> WeatherSettingsRecord {
        let mut blob = synth_schema();
        synth_chunk(b"OBJT", &synth_objt(), &mut blob);
        parse_wths(
            form_id,
            &[sub(b"EDID", b"RootTemplate\0"), sub(b"REFL", &blob)],
            &None,
        )
    }

    /// The instance-value reader end to end on the miniature corpus:
    /// refs remap, the Colors struct tree binds, and the synthesized
    /// weather carries the mapped image spaces and sky color.
    #[test]
    fn root_instance_decodes_and_synthesizes_a_weather() {
        let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);
        let mut blob = synth_schema();
        synth_chunk(b"OBJT", &synth_objt(), &mut blob);
        let refl = parse_bgs_reflection_with_remap(&blob, &Some(remap));
        let root = refl.root_instance.as_ref().expect("instance decodes");
        assert_eq!(refl.unconsumed_instance_chunks, 0);
        // Ref leaves remapped into the plugin's load-order slot (raw
        // mod index 1 = this plugin → slot 2; null stays null).
        assert_eq!(
            root.field("pImageSpace").and_then(ReflectionValue::as_form_id),
            Some((2u32 << 24) | 0x0001_2345)
        );
        assert_eq!(
            root.field("pImageSpaceNight").and_then(ReflectionValue::as_form_id),
            Some((2u32 << 24) | 0x0001_2346)
        );

        let record = WeatherSettingsRecord {
            form_id: 0x000A_0001,
            editor_id: "RootTemplate".into(),
            reflection: Some(refl),
            ..WeatherSettingsRecord::default()
        };
        let settings = HashMap::from([(0x000A_0001, record.clone())]);
        let wthr = synthesize_weather_record(&record, &settings).expect("synthesizes");
        assert_eq!(wthr.editor_id, "RootTemplate");
        // The day space fills Sunrise/Day/Sunset; the night space Night.
        assert_eq!(
            wthr.image_spaces,
            [
                Some((2u32 << 24) | 0x0001_2345),
                Some((2u32 << 24) | 0x0001_2345),
                Some((2u32 << 24) | 0x0001_2345),
                Some((2u32 << 24) | 0x0001_2346),
            ]
        );
        // Sun color lifted to SKY_SUN, uniform across TOD; the other
        // groups stay at the decoder default (unauthored).
        let sun = wthr.sky_colors[crate::esm::records::weather::SKY_SUN];
        assert!(sun.iter().all(|c| (c.r, c.g, c.b, c.a) == (255, 128, 64, 255)));
        assert!(
            wthr.sky_colors[crate::esm::records::weather::SKY_FOG]
                .iter()
                .all(|c| (c.r, c.g, c.b, c.a) == (0, 0, 0, 0))
        );
        assert!(!wthr.fog_distances_authored, "WTHS authors no fog distances");
    }

    /// A child's DIFF override replaces the parent's value in the
    /// merged tree; fields it does not override inherit the root's.
    #[test]
    fn child_diff_merges_over_the_root_template() {
        let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);
        // Child schema: Root class only (the DIFF names field 0).
        let mut blob = Vec::new();
        synth_chunk(
            b"BETH",
            &4u32.to_le_bytes().iter().chain(&4u32.to_le_bytes()).copied().collect::<Vec<u8>>(),
            &mut blob,
        );
        synth_chunk(b"STRT", b"Root\0pImageSpace\0", &mut blob);
        synth_chunk(b"TYPE", 1u32.to_le_bytes().as_ref(), &mut blob);
        synth_chunk(
            b"CLAS",
            &synth_clas(0, 0, &[synth_refl_field(12, -251, 280, 8)]),
            &mut blob,
        );
        let mut diff = Vec::new();
        diff.extend_from_slice(&0i32.to_le_bytes()); // type ref = Root
        diff.extend_from_slice(&0u16.to_le_bytes()); // field 0: pImageSpace
        diff.extend_from_slice(&(-243i32).to_le_bytes()); // ref → UInt32
        diff.extend_from_slice(&0x0109_ABCDu32.to_le_bytes());
        diff.extend_from_slice(&0xFFFFu16.to_le_bytes());
        synth_chunk(b"DIFF", &diff, &mut blob);

        let child_refl = parse_bgs_reflection_with_remap(&blob, &Some(remap));
        assert_eq!(child_refl.unconsumed_instance_chunks, 0);
        let overrides = child_refl.diff_values.as_ref().expect("DIFF decodes");
        assert_eq!(overrides.len(), 1);
        assert_eq!(overrides[0].field_name, "pImageSpace");
        assert_eq!(
            overrides[0].value.as_form_id(),
            Some((2u32 << 24) | 0x0009_ABCD)
        );

        let root_record = synth_root_record(0x000A_0001);
        let child = WeatherSettingsRecord {
            form_id: 0x000A_0002,
            editor_id: "ChildWeather".into(),
            reflection_diff: Some(child_refl),
            reflection_parent: Some(0x000A_0001),
            ..WeatherSettingsRecord::default()
        };
        let settings = HashMap::from([
            (0x000A_0001, root_record),
            (0x000A_0002, child.clone()),
        ]);
        let wthr = synthesize_weather_record(&child, &settings).expect("synthesizes");
        // The child's image-space override wins…
        assert_eq!(
            wthr.image_spaces[crate::esm::records::weather::TOD_DAY],
            Some((2u32 << 24) | 0x0009_ABCD)
        );
        // …while the Sun color inherits from the root template.
        let sun = wthr.sky_colors[crate::esm::records::weather::SKY_SUN];
        assert!(sun.iter().all(|c| (c.r, c.g, c.b, c.a) == (255, 128, 64, 255)));
        assert_eq!(wthr.editor_id, "ChildWeather");
    }

    /// A dangling or cyclic RFDP chain declines (no synthesized
    /// weather) instead of panicking or half-resolving.
    #[test]
    fn dangling_and_cyclic_chains_decline() {
        let orphan = WeatherSettingsRecord {
            form_id: 0x000A_0003,
            reflection_parent: Some(0x000B_0000), // not in the map
            ..WeatherSettingsRecord::default()
        };
        let settings = HashMap::from([(0x000A_0003, orphan.clone())]);
        assert!(synthesize_weather_record(&orphan, &settings).is_none());

        let mut a = WeatherSettingsRecord {
            form_id: 0x000A_0004,
            reflection_parent: Some(0x000A_0005),
            ..WeatherSettingsRecord::default()
        };
        let mut b = WeatherSettingsRecord {
            form_id: 0x000A_0005,
            reflection_parent: Some(0x000A_0004),
            ..WeatherSettingsRecord::default()
        };
        a.reflection = Some(synth_root_record(0x000A_0004).reflection.unwrap());
        b.reflection = Some(synth_root_record(0x000A_0005).reflection.unwrap());
        let settings = HashMap::from([(0x000A_0004, a), (0x000A_0005, b)]);
        let a_rec = settings.get(&0x000A_0004).unwrap();
        assert!(synthesize_weather_record(a_rec, &settings).is_none());
    }

    /// The WTHS shell: EDID + REFL root shape and the RFDP remap.
    #[test]
    fn wths_shell_parses_refl_and_remaps_rfdp() {
        let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);
        let self_ref = (1u32 << 24) | 0x000E_8397; // parent template, own plugin
        let subs = vec![
            sub(b"EDID", b"WeatherUniqueProbe\0"),
            sub(b"RFDP", self_ref.to_le_bytes()),
            sub(
                b"RDIF",
                // BETH chunk: size 8, then version 4 / chunk count 1.
                b"BETH\x08\x00\x00\x00\x04\x00\x00\x00\x01\x00\x00\x00",
            ),
        ];
        let rec = parse_wths(0x000A_0001, &subs, &Some(remap));
        assert_eq!(rec.editor_id, "WeatherUniqueProbe");
        assert_eq!(rec.reflection_parent, Some((2u32 << 24) | 0x000E_8397));
        let diff = rec.reflection_diff.expect("RDIF decoded");
        assert_eq!(diff.version, 4);
        assert!(rec.reflection.is_none());
    }
}
