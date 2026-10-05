//! #3398 Phase 2 — the compact per-material index over a Component
//! Database, built in one streaming pass without materialising the ~9 GB
//! generic value tree a full `parse` produces (#4274).
//!
//! Everything here is a measured contract from the 2026-09-30 probe run
//! against the vanilla base CDB (`Starfield - Materials.ba2`, 105 MB,
//! 1,438,780 instances), recorded in
//! `docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md` and its addendum:
//!
//! - **Key.** A material path maps to `BSResource::ID` by reflected
//!   CRC-32 (poly `0xEDB88320`, init 0, no final XOR) over the
//!   lowercased backslash path, hashed as two strings — stem, then
//!   directory (the struct's field labels are rotated; the columns are
//!   positional).
//! - **Join.** The `BSComponentDB2::DBFileIndex.Objects` table carries
//!   `PersistentID → DBID`; the two index instances (CompiledDB,
//!   DBFileIndex) precede the component stream, and
//!   `Components[j] ↔ stream instance j + 2` aligned for **1,438,778 of
//!   1,438,778** instances.
//! - **Graph.** A material object's components reference children by
//!   DBID (`.ID.Value`): `LayerID` → layer object → `MaterialID` →
//!   layer-material object → `TextureSetID` → texture-set object, whose
//!   `MRTextureFile` components carry the texture paths with the
//!   per-object component `Index` selecting the slot.
//!
//! What survives the walk (the rest is skipped without decoding):
//! texture slots, ID-shaped child references, `MaterialParamFloat` /
//! `ParamBool` values, alpha / effect / translucency settings, and the
//! two `DBFileIndex` tables needed for the join. Roughly 100–200 MB for
//! the vanilla base CDB, versus 9.19 GB for `parse`.

use crate::error::{Error, Result};
use crate::reader::{
    consume_top_level_value, parse_schema, peek_top_level_class_name, state_has_more_chunks,
    skip_top_level_value, Cursor, State,
};
use crate::types::TypeReference;
use crate::value::Value;
use crate::ChunkType;

/// `MRTextureFile` slot numbers, derived from the full-corpus filename
/// suffix census (2026-09-30; e.g. slot 0 carries 41,106 `*_color.*`
/// paths, slot 1 carries 46,310 `*_normal.*`). Slots 3/4/5/8 (roughness,
/// metalness, AO, transmissive) have no canonical
/// `MaterialTextureSet` role yet — see the parked table in
/// `docs/engine/nifal.md` and the #4429 XOR guard.
pub const SLOT_COLOR: u8 = 0;
pub const SLOT_NORMAL: u8 = 1;
pub const SLOT_OPACITY: u8 = 2;
pub const SLOT_ROUGHNESS: u8 = 3;
pub const SLOT_METALNESS: u8 = 4;
pub const SLOT_AMBIENT_OCCLUSION: u8 = 5;
pub const SLOT_HEIGHT: u8 = 6;
pub const SLOT_EMISSIVE: u8 = 7;
pub const SLOT_TRANSMISSIVE: u8 = 8;

/// The kind of an ID-carrying component — how child objects attach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Layer,
    Material,
    TextureSet,
    Other,
}

impl RefKind {
    fn from_class_name(name: &str) -> Self {
        match name {
            "BSMaterial::LayerID" => RefKind::Layer,
            "BSMaterial::MaterialID" => RefKind::Material,
            "BSMaterial::TextureSetID" => RefKind::TextureSet,
            _ => RefKind::Other,
        }
    }
}

/// One resolved material, the lookup result. Field names follow the
/// spike's class tables; `None`/absent means the walk found no such
/// component on any visited object.
#[derive(Debug, Clone, Default)]
pub struct CdbMaterial {
    /// Slot (`SLOT_*`) → texture path, first layer wins per slot.
    pub textures: Vec<(u8, String)>,
    /// `MaterialParamFloat` values by per-object component Index. The
    /// index→semantics mapping (which is roughness, which metalness) is
    /// NOT yet verified, so these are carried but deliberately
    /// untranslated.
    pub param_floats: Vec<(u8, f32)>,
    /// `AlphaSettingsComponent.AlphaTestThreshold`.
    pub alpha_test_threshold: Option<f32>,
    /// `AlphaSettingsComponent.HasOpacity`.
    pub has_opacity: Option<bool>,
    /// `EffectSettingsComponent.IsGlass`.
    pub is_glass: Option<bool>,
    /// `TranslucencySettings.UseSSS`.
    pub use_sss: Option<bool>,
    /// `TranslucencySettings.TransmissiveScale`.
    pub transmissive_scale: Option<f32>,
    /// Enabled `TextureReplacement` colours per texture slot
    /// (`Components.Index`, the same slot space `MRTextureFile` uses) —
    /// Starfield's flat-color materials author a solid colour INSTEAD of
    /// the texture in the slot they replace (measured: 36,866 corpus
    /// instances, `Color` + `Enabled` shapes only, never a path;
    /// 2026-09-30 census). #5190 — keyed per slot: pre-fix the first
    /// walked replacement won for the whole set whatever slot it sat on,
    /// so a normal/roughness/AO replacement fabricated an albedo tint.
    /// Disabled (`Enabled == false`) replacements are skipped at capture;
    /// `Enabled` absent is treated as enabled (4,481 corpus instances).
    pub flat_color_slots: Vec<(u8, [f32; 4])>,
}

/// #5190 — one texture-set object's enabled `TextureReplacement`
/// entries, keyed by the texture slot each replaces (first capture per
/// slot wins).
type TextureReplacements = Vec<(u8, [f32; 4], Option<bool>)>;

/// Compact per-CDB material index. Build once per CDB payload, share
/// across provider rebuilds, look up by material path.
#[derive(Debug, Default)]
pub struct MaterialIndex {
    /// `(stem_crc, dir_crc)` → object DBID, from
    /// `DBFileIndex.Objects.PersistentID`.
    objects_by_key: std::collections::HashMap<(u32, u32), u32>,
    /// `Components[j]` → (ObjectID, Index) — the alignment table that
    /// attributes stream instance `j + 2` to its owning object.
    rows: Vec<Row>,
    /// Object → ID-shaped child references.
    child_refs: std::collections::HashMap<u32, Vec<(RefKind, u32, u8)>>,
    /// Object → `MRTextureFile`/`TextureFile` slots (slot, path).
    textures: std::collections::HashMap<u32, Vec<(u8, String)>>,
    /// Object → per-slot `TextureReplacement` (slot, rgba, enabled) —
    /// #5190, first entry per slot wins at lookup.
    flat_colors: std::collections::HashMap<u32, TextureReplacements>,
    /// Object → `MaterialParamFloat` (Index, Value).
    param_floats: std::collections::HashMap<u32, Vec<(u8, f32)>>,
    alpha: std::collections::HashMap<u32, (Option<f32>, Option<bool>)>,
    effect: std::collections::HashMap<u32, Option<bool>>,
    translucency: std::collections::HashMap<u32, (Option<bool>, Option<f32>)>,
}

#[derive(Debug, Clone, Copy)]
struct Row {
    object: u32,
    index: u8,
}

/// Reflected CRC-32, poly `0xEDB88320`, init 0, xorout 0 — the variant
/// the 2026-08-29 key probe identified (98.3% of real NIF-named paths
/// resolve; the reversed column assignment matched 0).
fn bethesda_crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc
}

/// Normalise a material path to the CDB key: lowercased, backslash
/// separators, no `data\` prefix, split into (directory, stem-without-
/// extension). Returns `(stem_crc, dir_crc)` in `PersistentID` column
/// order (stem first — the labels are rotated, this is positional).
pub fn material_key(path: &str) -> (u32, u32) {
    let t = path.trim_end_matches(|c: char| c == '\0' || c.is_ascii_whitespace());
    let mut t = t.replace('/', "\\").to_ascii_lowercase();
    if let Some(stripped) = t.strip_prefix("data\\") {
        t = stripped.to_string();
    }
    let (dir, file) = match t.rsplit_once('\\') {
        Some((d, f)) => (d.to_string(), f.to_string()),
        None => (String::new(), t.clone()),
    };
    let stem = match file.rsplit_once('.') {
        Some((s, _)) => s.to_string(),
        None => file,
    };
    (bethesda_crc32(stem.as_bytes()), bethesda_crc32(dir.as_bytes()))
}

impl MaterialIndex {
    /// Build the index from one CDB payload in a single streaming pass.
    /// The `DBFileIndex` instance's big collection chunks are consumed
    /// element-by-element (never as whole `Value::List`s); every other
    /// uninteresting instance is skipped without decoding.
    pub fn build(bytes: &[u8]) -> Result<Self> {
        let mut state = parse_schema(bytes, crate::ParseLimits::unlimited())?;
        let mut idx = MaterialIndex::default();
        let mut pos: usize = 0;
        // Stream position of the first instance attributed by the
        // Components table: everything before `DBFileIndex` (the
        // `CompiledDB` key map on real files) is skipped whole, and
        // `Components[j]` describes stream instance `base + j`.
        let mut base: Option<usize> = None;
        while state_has_more_chunks(&state) {
            let class_name = peek_top_level_class_name(&state)?;
            if class_name.as_deref() == Some("BSComponentDB2::DBFileIndex") {
                if base.is_some() {
                    // #5320 — a second index poisons the row/instance
                    // join; name the cause instead of the old
                    // contextless WrongChunkType { Objt, Objt }.
                    return Err(Error::DuplicateDbFileIndex);
                }
                stream_db_file_index(&mut state, &mut idx)?;
                base = Some(pos + 1);
            } else if let (Some(name), Some(base)) = (&class_name, base) {
                if pos >= base {
                    let value = consume_top_level_value(&mut state)?;
                    idx.capture_instance(pos - base, name, &value);
                } else {
                    skip_top_level_value(&mut state)?;
                }
            } else {
                // `CompiledDB` (48,749-entry key map on the base CDB) and
                // any pre-index noise: skipped without materialising.
                skip_top_level_value(&mut state)?;
            }
            pos += 1;
        }
        // #5320 (PAR-D2-2026-10-05-02) — a degraded build must not look
        // like a successful one. No index means `base` stayed `None` and
        // every instance was skipped: the old return was `Ok` with an
        // empty index, surfaced only as an info-level "0 keyed objects".
        let Some(base) = base else {
            return Err(Error::MissingDbFileIndex);
        };
        // The `Components[j]` ↔ instance `base + j` alignment is the
        // load-bearing join (measured 1,438,778 / 1,438,778 on the base
        // CDB); a mismatch means `capture_instance` silently dropped
        // rows or left rows uncaptured, attributing every texture to the
        // wrong object. Count the post-index stream instances and refuse
        // the misaligned join.
        let instances = pos - base;
        if idx.rows.len() != instances {
            return Err(Error::RowInstanceMismatch {
                rows: idx.rows.len(),
                instances,
            });
        }
        Ok(idx)
    }

    /// Number of keyed objects (vanilla base CDB: 500,403 — the whole
    /// `DBFileIndex.Objects` table, not just material roots; layers and
    /// texture sets are keyed too).
    pub fn material_count(&self) -> usize {
        self.objects_by_key.len()
    }

    /// Resolve one material path against this index, following the
    /// measured graph shape: material -> `LayerID` children (authored
    /// order) -> each layer's `MaterialID` -> layer material -> its
    /// `TextureSetID` -> the texture set's `MRTextureFile` slots. The
    /// first layer wins a slot. Blender / UV-stream / LOD refs are
    /// blend-graph controls, not texture sources, and are not walked
    /// (their mask textures are an unmodeled feature - deliberate).
    pub fn lookup(&self, path: &str) -> Option<CdbMaterial> {
        let key = material_key(path);
        let root = *self.objects_by_key.get(&key)?;
        let mut out = CdbMaterial::default();
        self.collect_settings(root, &mut out);

        // Layers in authored order; a single-layer material may instead
        // name its texture set directly off the root.
        let mut layer_objects: Vec<(u8, u32)> = self
            .child_refs
            .get(&root)
            .map(|refs| {
                refs.iter()
                    .filter(|(k, _, _)| *k == RefKind::Layer)
                    .map(|(_, child, i)| (*i, *child))
                    .collect()
            })
            .unwrap_or_default();
        layer_objects.sort_unstable();

        let mut layer_materials: Vec<u32> = Vec::new();
        for (_, layer) in &layer_objects {
            if let Some(refs) = self.child_refs.get(layer) {
                for (kind, child, _) in refs {
                    if *kind == RefKind::Material {
                        layer_materials.push(*child);
                    }
                }
            }
        }
        if let Some(refs) = self.child_refs.get(&root) {
            for (kind, child, _) in refs {
                if *kind == RefKind::TextureSet {
                    // Root-named texture set: treated as layer order 0.
                    self.collect_slots(*child, &mut out);
                }
            }
        }

        for mat_obj in layer_materials {
            self.collect_settings(mat_obj, &mut out);
            if let Some(refs) = self.child_refs.get(&mat_obj) {
                for (kind, child, _) in refs {
                    if *kind == RefKind::TextureSet {
                        self.collect_slots(*child, &mut out);
                    }
                }
            }
        }
        Some(out)
    }

    /// First-wins slot merge from one texture-set object.
    fn collect_slots(&self, tex_set: u32, out: &mut CdbMaterial) {
        if let Some(slots) = self.textures.get(&tex_set) {
            for (slot, path) in slots {
                if !out.textures.iter().any(|(s, _)| s == slot) {
                    out.textures.push((*slot, path.clone()));
                }
            }
        }
        if let Some(replacements) = self.flat_colors.get(&tex_set) {
            for (slot, color, enabled) in replacements {
                // `Enabled` absent (4,481 corpus instances) cannot gate —
                // treat as enabled; only an explicit false skips. First
                // texture set to supply a slot's replacement wins, mirroring
                // the texture-slot merge above.
                if *enabled == Some(false) {
                    continue;
                }
                if !out.flat_color_slots.iter().any(|(s, _)| s == slot) {
                    out.flat_color_slots.push((*slot, *color));
                }
            }
        }
    }

    /// Scalars and settings carried directly on one object (params,
    /// alpha, effect/glass, translucency) - first-wins.
    fn collect_settings(&self, obj: u32, out: &mut CdbMaterial) {
        if let Some(params) = self.param_floats.get(&obj) {
            for (i, v) in params {
                if !out.param_floats.iter().any(|(s, _)| s == i) {
                    out.param_floats.push((*i, *v));
                }
            }
        }
        if let Some((threshold, opacity)) = self.alpha.get(&obj) {
            if out.alpha_test_threshold.is_none() {
                out.alpha_test_threshold = *threshold;
            }
            if out.has_opacity.is_none() {
                out.has_opacity = *opacity;
            }
        }
        if let Some(glass) = self.effect.get(&obj) {
            if out.is_glass.is_none() {
                out.is_glass = *glass;
            }
        }
        if let Some((sss, scale)) = self.translucency.get(&obj) {
            if out.use_sss.is_none() {
                out.use_sss = *sss;
            }
            if out.transmissive_scale.is_none() {
                out.transmissive_scale = *scale;
            }
        }
    }

    /// Attribute one decoded stream instance (`row_index` = position
    /// past the DBFileIndex) to its owning object via the alignment
    /// table, then file its payload.
    fn capture_instance(&mut self, row_index: usize, class_name: &str, value: &Value) {
        let Some(row) = self.rows.get(row_index) else {
            return;
        };
        let Value::Object(o) = value else {
            return;
        };
        match class_name {
            "BSMaterial::MRTextureFile" | "BSMaterial::TextureFile" => {
                let Some(Value::String(path)) = o.fields.get("FileName") else {
                    return;
                };
                self.textures
                    .entry(row.object)
                    .or_default()
                    .push((row.index, path.clone()));
            }
            "BSMaterial::TextureReplacement" => {
                let color = xmcolor4(o, "Color");
                let enabled = match o.fields.get("Enabled") {
                    Some(Value::Bool(v)) => Some(*v),
                    _ => None,
                };
                if let Some(color) = color {
                    // #5190 — `row.index` is the texture SLOT this
                    // replacement replaces; keep every slot's replacement
                    // (first-wins per slot) instead of the first one for
                    // the whole set.
                    let slots = self.flat_colors.entry(row.object).or_default();
                    if !slots.iter().any(|(s, _, _)| *s == row.index) {
                        slots.push((row.index, color, enabled));
                    }
                }
            }
            "BSMaterial::LayerID"
            | "BSMaterial::MaterialID"
            | "BSMaterial::TextureSetID"
            | "BSMaterial::BlenderID"
            | "BSMaterial::UVStreamID"
            | "BSMaterial::LODMaterialID" => {
                let Some(dbid) = id_field(o) else {
                    return;
                };
                if dbid == 0 {
                    return;
                }
                let kind = RefKind::from_class_name(class_name);
                self.child_refs
                    .entry(row.object)
                    .or_default()
                    .push((kind, dbid, row.index));
            }
            "BSMaterial::MaterialParamFloat" => {
                let Some(Value::Float(v)) = o.fields.get("Value") else {
                    return;
                };
                self.param_floats
                    .entry(row.object)
                    .or_default()
                    .push((row.index, *v));
            }
            "BSMaterial::AlphaSettingsComponent" => {
                let threshold = match o.fields.get("AlphaTestThreshold") {
                    Some(Value::Float(v)) => Some(*v),
                    _ => None,
                };
                let opacity = match o.fields.get("HasOpacity") {
                    Some(Value::Bool(v)) => Some(*v),
                    _ => None,
                };
                self.alpha
                    .entry(row.object)
                    .and_modify(|e| {
                        if e.0.is_none() {
                            e.0 = threshold;
                        }
                        if e.1.is_none() {
                            e.1 = opacity;
                        }
                    })
                    .or_insert((threshold, opacity));
            }
            "BSMaterial::EffectSettingsComponent" => {
                let glass = match o.fields.get("IsGlass") {
                    Some(Value::Bool(v)) => Some(*v),
                    _ => None,
                };
                self.effect.entry(row.object).or_insert(glass);
            }
            "BSMaterial::TranslucencySettings" => {
                let sss = match o.fields.get("UseSSS") {
                    Some(Value::Bool(v)) => Some(*v),
                    _ => None,
                };
                let scale = match o.fields.get("TransmissiveScale") {
                    Some(Value::Float(v)) => Some(*v),
                    _ => None,
                };
                self.translucency
                    .entry(row.object)
                    .and_modify(|e| {
                        if e.0.is_none() {
                            e.0 = sss;
                        }
                        if e.1.is_none() {
                            e.1 = scale;
                        }
                    })
                    .or_insert((sss, scale));
            }
            _ => {}
        }
    }
}

/// An `XMFLOAT4` value reached through one named wrapper field —
/// `Value` for a `BSMaterial::Color` component, `Color` inside a
/// `TextureReplacement` (`.Color.Value.x…`).
fn xmcolor4(o: &crate::value::ObjectInstance, wrapper: &str) -> Option<[f32; 4]> {
    let Value::Object(color) = o.fields.get(wrapper)? else {
        return None;
    };
    let Value::Object(inner) = color.fields.get("Value")? else {
        return None;
    };
    let g = |n: &str| match inner.fields.get(n) {
        Some(Value::Float(v)) => Some(*v),
        _ => None,
    };
    Some([g("x")?, g("y")?, g("z")?, g("w")?])
}

/// `Some(dbid)` when the object carries an `ID` field of the
/// `BSComponentDB2::ID` shape (`.ID.Value`).
fn id_field(o: &crate::value::ObjectInstance) -> Option<u32> {
    match o.fields.get("ID") {
        Some(Value::Object(inner)) => match inner.fields.get("Value") {
            Some(Value::U32(v)) => Some(*v),
            _ => None,
        },
        _ => None,
    }
}

/// Stream the `BSComponentDB2::DBFileIndex` instance without building
/// its collection `Value`s whole: consume the OBJT payload's inline
/// fields by layout, then its side-chunk collection fields in read
/// order, decoding the big LISTs one element at a time.
fn stream_db_file_index(state: &mut State<'_>, idx: &mut MaterialIndex) -> Result<()> {
    let kind = state.peek_kind()?;
    let (is_cast, is_diff) = match kind {
        ChunkType::Objt => (false, false),
        ChunkType::User => (true, false),
        // #5320 — DIFF/USRD were "supported" here but the payload's
        // inline fields were read in offset order, which only holds for
        // whole (non-diff) payloads; a diff payload indexes fields by
        // declaration slot. Vanilla's index is an OBJT, so the only
        // thing the old arms produced was a silent misparse of mod
        // data. Reject loudly instead.
        ChunkType::Diff | ChunkType::Usrd => {
            return Err(Error::UnsupportedDbFileIndexChunk { kind });
        }
        _ => {
            return Err(Error::WrongChunkType {
                wanted: ChunkType::Objt,
                got: kind,
            });
        }
    };
    let payload = state.consume_chunk(kind)?;
    let mut cur = Cursor::new(payload);
    if is_cast {
        let _target = cur.read_i32()?;
    }
    let type_ref = TypeReference::new(cur.read_i32()?);
    let (field_layout, read_order) = {
        let class = state.class_for(type_ref)?;
        (class.fields.clone(), class.read_order.clone())
    };

    let mut chunk_fields = Vec::new();
    for i in &read_order {
        let field = &field_layout[*i as usize];
        if state.is_chunk_type(field.type_ref) {
            chunk_fields.push(field.clone());
        } else {
            let _ = crate::reader::read_value(state, field.type_ref, &mut cur, is_diff)?;
        }
    }

    for field in chunk_fields {
        match field.name.as_str() {
            "Components" => stream_list(state, |v| {
                let Value::Object(o) = v else { return };
                let Some(object) = id_named(o, "ObjectID") else { return };
                let index = match o.fields.get("Index") {
                    Some(Value::U16(i)) => *i,
                    _ => return,
                };
                idx.rows.push(Row {
                    object,
                    index: index.min(u8::MAX as u16) as u8,
                });
            })?,
            "Objects" => stream_list(state, |v| {
                let Value::Object(o) = v else { return };
                let Some(dbid) = id_named(o, "DBID") else { return };
                let pid = match o.fields.get("PersistentID") {
                    Some(Value::Object(pid)) => {
                        let mut cols = [0u32; 3];
                        let mut n = 0;
                        for v in pid.fields.values() {
                            if let Value::U32(x) = v {
                                if n < 3 {
                                    cols[n] = *x;
                                }
                                n += 1;
                            }
                        }
                        if n == 3 {
                            Some(cols)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some([stem, dir, _ext]) = pid {
                    idx.objects_by_key.insert((stem, dir), dbid);
                }
            })?,
            // Edges encode the blend graph; lookups follow ID refs
            // directly (measured), so count these past without keeping
            // them.
            "Edges" => stream_list(state, |_| {})?,
            _other => {
                // ComponentTypes (a MAPC) and any future collection
                // field: consume generically without retaining.
                skip_top_level_value(state)?;
            }
        }
    }

    if is_cast {
        let _trailing = cur.read_u32()?;
    }
    // #5320 — mirror `consume_object`'s ObjectTrailingBytes check: the
    // inline payload must end exactly at the last declared field, or the
    // index was built against a payload whose layout doesn't match its
    // class declaration.
    let leftover = payload.len() - cur.pos();
    if leftover != 0 {
        return Err(Error::ObjectTrailingBytes { leftover });
    }
    Ok(())
}

fn id_named(o: &crate::value::ObjectInstance, name: &str) -> Option<u32> {
    match o.fields.get(name) {
        Some(Value::Object(inner)) => match inner.fields.get("Value") {
            Some(Value::U32(v)) => Some(*v),
            _ => None,
        },
        _ => None,
    }
}

/// Consume one LIST chunk, handing each element's decoded value to `f`
/// without retaining it. Mirrors `consume_list`'s on-disk guards.
fn stream_list(state: &mut State<'_>, mut f: impl FnMut(&Value)) -> Result<()> {
    let payload = state.consume_chunk(ChunkType::List)?;
    let mut cur = Cursor::new(payload);
    let elem_ref = TypeReference::new(cur.read_i32()?);
    let raw_count = cur.read_i32()?;
    let count = usize::try_from(raw_count)
        .map_err(|_| Error::NegativeCount {
            what: "LIST element",
            raw: raw_count,
        })?
        .min(payload.len());
    for _ in 0..count {
        let v = crate::reader::read_value(state, elem_ref, &mut cur, false)?;
        f(&v);
    }
    Ok(())
}


/// #3398 — synthetic CDB fixture shared with the downstream merge-arm
/// tests in `byroredux`. `#[doc(hidden)]`: an implementation detail of
/// the test suite, not API.
#[doc(hidden)]
pub mod test_support {
    /// One synthetic-CDB chunk: fourCC + payload, header stripped so
    /// tests can splice the instance stream freely.
    pub type SyntheticCdbChunks = Vec<(&'static [u8; 4], Vec<u8>)>;

    use super::*;

    // ── synthetic CDB builder ────────────────────────────────────────
    //
    // A minimal but structurally faithful CDB: STRT names, TYPE + CLAS
    // schemas, one DBFileIndex instance whose Components/Objects tables
    // live in LIST side-chunks, then a component stream mirroring the
    // measured shape (CTName/TextureSetID/MaterialID/LayerID/MRTextureFile).

    /// STRT payload: a flat NUL-separated blob with offset 0 = "" (the
    /// leading NUL), names following.
    fn strt(strings: &[&str]) -> Vec<u8> {
    // Leading NUL = the empty string at offset 0; strings[0] IS that
    // empty string, so it adds nothing and is skipped.
    let mut blob = vec![0u8];
    for s in &strings[1..] {
        blob.extend_from_slice(s.as_bytes());
        blob.push(0);
    }
    blob
    }

    /// STRT offset of `name` under the blob layout above: 1 + the sum of
    /// (len+1) for every preceding name (names[0] is the empty string at
    /// offset 0, so real names start at 1).
    fn strt_offsets<'a>(strings: &[&'a str]) -> std::collections::HashMap<&'a str, i32> {
    let mut map: std::collections::HashMap<&str, i32> = std::collections::HashMap::new();
    let mut off = 1i32;
    for (i, s) in strings.iter().enumerate() {
        if i == 0 {
            continue; // the empty string lives at 0
        }
        map.insert(*s, off);
        off += s.len() as i32 + 1;
    }
    map
    }

    /// Field declaration: (name offset, type ref, offset, size).
    type FieldDecl = (i32, i32, u16, u16);

    fn clas(name_off: i32, type_id: u32, flags: u16, fields: &[FieldDecl]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&name_off.to_le_bytes());
    p.extend_from_slice(&type_id.to_le_bytes());
    p.extend_from_slice(&flags.to_le_bytes());
    p.extend_from_slice(&(fields.len() as u16).to_le_bytes());
    for (n, t, o, s) in fields {
        p.extend_from_slice(&n.to_le_bytes());
        p.extend_from_slice(&t.to_le_bytes());
        p.extend_from_slice(&o.to_le_bytes());
        p.extend_from_slice(&s.to_le_bytes());
    }
    p
    }

    fn push_chunk(bytes: &mut Vec<u8>, kind_fourcc: &[u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(kind_fourcc);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
    }

    /// A string builtin type ref (`0xFFFFFF02`).
    const T_STRING: i32 = 0xFFFF_FF02u32 as i32;
    /// A Bool builtin type ref.
    const T_BOOL: i32 = 0xFFFF_FF10u32 as i32;
    /// A u32 builtin type ref.
    const T_U32: i32 = 0xFFFF_FF0Du32 as i32;
    /// A u16 builtin type ref (`ComponentInfo::Type` / `::Index`).
    const T_U16: i32 = 0xFFFF_FF0Bu32 as i32;
    /// A Float builtin type ref (`XMFLOAT4` channels).
    const T_FLOAT: i32 = 0xFFFF_FF11u32 as i32;
    /// A List builtin type ref.
    const T_LIST: i32 = 0xFFFF_FF03u32 as i32;
    /// A Map builtin type ref.
    const T_MAP: i32 = 0xFFFF_FF04u32 as i32;

    /// Build a CDB whose stream is:
    ///   [0] CTName (noise, proves skipping)
    ///   [1] DBFileIndex (Objects/Components/Edges in LIST side-chunks)
    ///   [2..] component instances attributed by the Components table.
    ///
    /// Classes are declared in this order; `TYPE` slots index them.
    /// The graph mirrors the measured shape:
    ///   material 10 ──LayerID(0)──> layer 11 ──MaterialID──> 12
    ///   12 ──TextureSetID──> 13; 13 carries MRTextureFile slots 0/1/3.
    pub fn synthetic_material_cdb() -> Vec<u8> {
        synthetic_material_cdb_with_color("Data\\Textures\\widget_color.DDS")
    }

    /// #5197 — the same graph with a slot-0 colour path whose *filename*
    /// the PBR keyword classifier would fabricate a conductor from
    /// (`iron`). Lets consumer-side tests pin that a CDB hit stamps
    /// dielectric-neutral scalars instead of re-running the classifier
    /// over the resolved colour path.
    pub fn synthetic_material_cdb_with_iron_color() -> Vec<u8> {
        synthetic_material_cdb_with_color(
            "Data\\Textures\\actors\\human\\faces\\eyes\\iris_iron_color.DDS",
        )
    }

    pub fn synthetic_material_cdb_with_color(color_path: &'static str) -> Vec<u8> {
        assemble_synthetic_cdb(&synthetic_cdb_chunks(color_path))
    }

    /// #5320 — the synthetic CDB as (fourCC, payload) chunks, WITHOUT the
    /// BETH header, so tests can mutate the instance stream (drop /
    /// duplicate / corrupt the DBFileIndex, append unlisted instances)
    /// and re-serialize with [`assemble_synthetic_cdb`]. The unmutated
    /// list assembles byte-identical to the pre-refactor fixture.
    pub fn synthetic_cdb_chunks(color_path: &'static str) -> SyntheticCdbChunks {
    let names = [
        "", // STRT index 0: empty string
        "BSComponentDB2::ID",
        "BSResource::ID",
        "BSComponentDB2::DBFileIndex::ComponentInfo",
        "BSComponentDB2::DBFileIndex::ObjectInfo",
        "BSComponentDB2::DBFileIndex",
        "Value",
        "Dir",
        "File",
        "Ext",
        "ObjectID",
        "Type",
        "Index",
        "DBID",
        "HasData",
        "Parent",
        "ParentPersistentID",
        "PersistentID",
        "Objects",
        "Components",
        "Edges",
        "ComponentTypes",
        "Optimized",
        "BSComponentDB::CTName",
        "m_Name",
        "BSMaterial::LayerID",
        "ID",
        "BSMaterial::MaterialID",
        "BSMaterial::TextureSetID",
        "BSMaterial::MRTextureFile",
        "FileName",
        "XMFLOAT4",
        "x",
        "y",
        "z",
        "w",
        "BSMaterial::Color",
        "Color",
        "Enabled",
        "BSMaterial::TextureReplacement",
    ];
    // STRT offsets (index into the STRT chunk's own table; name i sits
    // at table slot i). parse_class resolves `strings.get(name_offset)`
    // where offset 0 is the empty string.
    let n = strt_offsets(&names);
    let g = |s: &str| n[s];

    let is_struct = crate::ClassFlags::IS_STRUCT;
    // Type refs for classes use the target's name_offset (the
    // canonical type-map key).
    let t_id_class = g("BSComponentDB2::ID");
    let t_rid_class = g("BSResource::ID");
    let t_cinfo = g("BSComponentDB2::DBFileIndex::ComponentInfo");
    let t_oinfo = g("BSComponentDB2::DBFileIndex::ObjectInfo");
    let t_color = g("BSMaterial::Color");
    let t_texrep = g("BSMaterial::TextureReplacement");
    let classes: Vec<Vec<u8>> = vec![
        clas(g("BSComponentDB2::ID"), 1, is_struct, &[(g("Value"), T_U32, 0, 4)]),
        clas(
            g("BSResource::ID"),
            2,
            is_struct,
            &[(g("Dir"), T_U32, 0, 4), (g("File"), T_U32, 4, 4), (g("Ext"), T_U32, 8, 4)],
        ),
        clas(
            g("BSComponentDB2::DBFileIndex::ComponentInfo"),
            3,
            is_struct,
            &[
                (g("ObjectID"), t_id_class, 0, 4),
                (g("Type"), T_U16, 4, 2),
                (g("Index"), T_U16, 6, 2),
            ],
        ),
        clas(
            g("BSComponentDB2::DBFileIndex::ObjectInfo"),
            4,
            is_struct,
            &[
                (g("DBID"), t_id_class, 0, 4),
                (g("HasData"), T_BOOL, 4, 1),
                (g("Parent"), t_id_class, 5, 4),
                (g("ParentPersistentID"), t_rid_class, 9, 12),
                (g("PersistentID"), t_rid_class, 21, 12),
            ],
        ),
        clas(
            g("BSComponentDB2::DBFileIndex"),
            5,
            is_struct,
            &[
                (g("Objects"), T_LIST, 0, 8),
                (g("Components"), T_LIST, 8, 8),
                (g("Edges"), T_LIST, 16, 8),
                (g("ComponentTypes"), T_MAP, 24, 8),
                (g("Optimized"), T_BOOL, 32, 1),
            ],
        ),
        clas(g("BSComponentDB::CTName"), 6, is_struct, &[(g("m_Name"), T_STRING, 0, 4)]),
        clas(g("BSMaterial::LayerID"), 7, is_struct, &[(g("ID"), t_id_class, 0, 4)]),
        clas(g("BSMaterial::MaterialID"), 8, is_struct, &[(g("ID"), t_id_class, 0, 4)]),
        clas(g("BSMaterial::TextureSetID"), 9, is_struct, &[(g("ID"), t_id_class, 0, 4)]),
        clas(g("BSMaterial::MRTextureFile"), 10, is_struct, &[(g("FileName"), T_STRING, 0, 4)]),
        // 10: XMFLOAT4 { x, y, z, w }
        clas(
            g("XMFLOAT4"),
            11,
            is_struct,
            &[
                (g("x"), T_FLOAT, 0, 4),
                (g("y"), T_FLOAT, 4, 4),
                (g("z"), T_FLOAT, 8, 4),
                (g("w"), T_FLOAT, 12, 4),
            ],
        ),
        // 11: BSMaterial::Color { Value: XMFLOAT4 }
        clas(g("BSMaterial::Color"), 12, is_struct, &[(g("Value"), g("XMFLOAT4"), 0, 16)]),
        // 12: TextureReplacement { Color, Enabled }
        clas(
            g("BSMaterial::TextureReplacement"),
            13,
            is_struct,
            &[(g("Color"), t_color, 0, 16), (g("Enabled"), T_BOOL, 16, 1)],
        ),
    ];

    let strt_payload = strt(&names);

    // ── DBFileIndex instance payload + side chunks ──
    // Inline fields by read_order: Optimized (Bool) at offset 32.
    let mut dbfile_objt = Vec::new();
    dbfile_objt.extend_from_slice(&g("BSComponentDB2::DBFileIndex").to_le_bytes());
    dbfile_objt.push(0u8); // Optimized = false

    // Objects LIST: one row — DBID 10, PersistentID (stem, dir, ext).
    let (stem_crc, dir_crc) = material_key("materials\\test\\widget.mat");
    let mut objects = Vec::new();
    objects.extend_from_slice(&t_oinfo.to_le_bytes());
    objects.extend_from_slice(&1i32.to_le_bytes()); // count
    {
        let dbid = 10u32;
        objects.extend_from_slice(&dbid.to_le_bytes()); // DBID.Value
        objects.push(0); // HasData
        objects.extend_from_slice(&0u32.to_le_bytes()); // Parent.Value
        objects.extend_from_slice(&0u32.to_le_bytes()); // PPID Dir
        objects.extend_from_slice(&0u32.to_le_bytes()); // PPID File
        objects.extend_from_slice(&0u32.to_le_bytes()); // PPID Ext
        // PersistentID in read_order byte layout: Dir←stem crc,
        // File←"mat" packed ASCII, Ext←dir crc (the labels-are-rotated
        // shape measured on the real CDB).
        objects.extend_from_slice(&stem_crc.to_le_bytes());
        objects.extend_from_slice(&0x0074_616Du32.to_le_bytes());
        objects.extend_from_slice(&dir_crc.to_le_bytes());
    }

    // Components LIST: rows for stream instances 2.., in order:
    //   obj 10: LayerID(0)            → instance 2
    //   obj 11: MaterialID(0)         → instance 3
    //   obj 12: TextureSetID(0)       → instance 4
    //   obj 13: MRTextureFile(0)      → instance 5
    //   obj 13: MRTextureFile(1)      → instance 6
    //   obj 13: MRTextureFile(3)      → instance 7
    //   obj 13: TextureReplacement(0) → instance 8 (flat colour)
    //   obj 13: TextureReplacement(1) → instance 9 (flat normal, #5190 —
    //                                   a NON-colour-slot replacement that
    //                                   must not tint the albedo)
    //   obj 10: CTName(0)             → instance 10 (noise)
    let component_rows: &[(u32, u32, u32)] = &[
        (10, 7, 0),
        (11, 8, 0),
        (12, 9, 0),
        (13, 10, 0),
        (13, 10, 1),
        (13, 10, 3),
        (13, 13, 0),
        (13, 13, 1),
        (10, 6, 0),
    ];
    let mut components = Vec::new();
    components.extend_from_slice(&t_cinfo.to_le_bytes());
    components.extend_from_slice(&(component_rows.len() as i32).to_le_bytes());
    for (obj, ty, index) in component_rows {
        components.extend_from_slice(&obj.to_le_bytes());
        components.extend_from_slice(&(*ty as u16).to_le_bytes());
        components.extend_from_slice(&(*index as u16).to_le_bytes());
    }

    // Edges LIST: empty.
    let mut edges = Vec::new();
    edges.extend_from_slice(&t_id_class.to_le_bytes());
    edges.extend_from_slice(&0i32.to_le_bytes());

    // ComponentTypes MAPC: empty (the index never reads it; the
    // reader consumes it generically).
    let mut component_types = Vec::new();
    component_types.extend_from_slice(&T_U32.to_le_bytes());
    component_types.extend_from_slice(&T_U32.to_le_bytes());
    component_types.extend_from_slice(&0i32.to_le_bytes());

    // ── component stream payloads ──
    let ctname_objt = {
        let mut p = Vec::new();
        p.extend_from_slice(&g("BSComponentDB::CTName").to_le_bytes());
        let name = b"widget_material1";
        p.extend_from_slice(&(name.len() as u16).to_le_bytes());
        p.extend_from_slice(name);
        p
    };
    let layer_objt = id_objt(g("BSMaterial::LayerID"), 11);
    let material_objt = id_objt(g("BSMaterial::MaterialID"), 12);
    let texset_objt = id_objt(g("BSMaterial::TextureSetID"), 13);
    let tex0 = texture_objt(g("BSMaterial::MRTextureFile"), color_path);
    let tex1 = texture_objt(g("BSMaterial::MRTextureFile"), "Data\\Textures\\widget_normal.DDS");
    let tex3 = texture_objt(g("BSMaterial::MRTextureFile"), "Data\\Textures\\widget_rough.DDS");
    // TextureReplacement: Color.Value = (0.25, 0.5, 0.75, 1.0), Enabled.
    // Inline class fields carry no per-field type tag — the class layout
    // (Color → BSMaterial::Color → Value → XMFLOAT4) drives the read, so
    // the payload after the instance type ref is just 16 float bytes +
    // the Enabled bool.
    let mut texrep = Vec::new();
    texrep.extend_from_slice(&t_texrep.to_le_bytes());
    for f in [0.25f32, 0.5, 0.75, 1.0] {
        texrep.extend_from_slice(&f.to_le_bytes());
    }
    texrep.push(1u8); // Enabled
    // #5190 — a second replacement on slot 1 (the NORMAL slot): a flat
    // ≈[0.1, 0.2, 0.3, 1] that a slot-agnostic consumer would have applied
    // as an albedo tint. Capture must key it to slot 1; the merge must
    // park it.
    let mut texrep_normal = Vec::new();
    texrep_normal.extend_from_slice(&t_texrep.to_le_bytes());
    for f in [0.1f32, 0.2, 0.3, 1.0] {
        texrep_normal.extend_from_slice(&f.to_le_bytes());
    }
    texrep_normal.push(1u8); // Enabled

    let mut chunks: SyntheticCdbChunks = vec![
        (b"STRT", strt_payload),
        (b"TYPE", (classes.len() as u32).to_le_bytes().to_vec()),
        (b"OBJT", dbfile_objt),
        (b"LIST", objects),
        (b"LIST", components),
        (b"LIST", edges),
        (b"MAPC", component_types),
        (b"OBJT", layer_objt),
        (b"OBJT", material_objt),
        (b"OBJT", texset_objt),
        (b"OBJT", tex0),
        (b"OBJT", tex1),
        (b"OBJT", tex3),
        (b"OBJT", texrep),
        (b"OBJT", texrep_normal),
        (b"OBJT", ctname_objt),
    ];
    // One CLAS per class, right after TYPE.
    chunks.splice(
        2..2,
        classes
            .into_iter()
            .map(|c| (b"CLAS" as &'static [u8; 4], c)),
    );
    chunks
    }

    /// Serialize a chunk list (see [`synthetic_cdb_chunks`]) back into a
    /// full BETH-prefixed CDB payload. Chunk count includes BETH, as the
    /// on-disk format counts it.
    pub fn assemble_synthetic_cdb(chunks: &SyntheticCdbChunks) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0x4854_4542u32.to_le_bytes()); // BETH
    bytes.extend_from_slice(&8u32.to_le_bytes()); // header size
    bytes.extend_from_slice(&4u32.to_le_bytes()); // file version
    bytes.extend_from_slice(&((chunks.len() + 1) as u32).to_le_bytes());
    for (kind, payload) in chunks {
        push_chunk(&mut bytes, kind, payload);
    }
    bytes
    }

    fn id_objt(type_ref: i32, value: u32) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&type_ref.to_le_bytes());
    p.extend_from_slice(&value.to_le_bytes());
    p
    }

    fn texture_objt(type_ref: i32, path: &str) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&type_ref.to_le_bytes());
    p.extend_from_slice(&(path.len() as u16).to_le_bytes());
    p.extend_from_slice(path.as_bytes());
    p
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{
        assemble_synthetic_cdb, synthetic_cdb_chunks, synthetic_material_cdb, SyntheticCdbChunks,
    };
    use super::*;

    /// The chunk list of the unmutated synthetic CDB, with the position
    /// of the DBFileIndex instance (the first OBJT) handy.
    fn mutable_chunks() -> (SyntheticCdbChunks, usize) {
        let chunks = synthetic_cdb_chunks("Data\\Textures\\widget_color.DDS");
        let dbfile = chunks
            .iter()
            .position(|(kind, _)| **kind == *b"OBJT")
            .expect("the fixture carries the DBFileIndex OBJT");
        (chunks, dbfile)
    }

    /// #5320 (PAR-D2-2026-10-05-02) — a CDB with no DBFileIndex used to
    /// build `Ok` with an empty index (only visible as an info-level
    /// "0 keyed objects"); it must be a typed error instead.
    #[test]
    fn build_rejects_a_cdb_without_a_dbfile_index() {
        let (mut chunks, dbfile) = mutable_chunks();
        chunks.remove(dbfile);
        let err = MaterialIndex::build(&assemble_synthetic_cdb(&chunks))
            .expect_err("an index-less CDB must not build an empty index");
        assert!(
            matches!(err, Error::MissingDbFileIndex),
            "expected MissingDbFileIndex, got {err:?}"
        );
    }

    /// #5320 — a second DBFileIndex poisons the row/instance join; name
    /// the cause instead of the old contextless
    /// WrongChunkType { wanted: Objt, got: Objt }.
    #[test]
    fn build_rejects_a_second_dbfile_index() {
        let (mut chunks, dbfile) = mutable_chunks();
        let dup = chunks[dbfile].clone();
        // After the DBFileIndex's four queued side chunks (Objects /
        // Components / Edges LISTs + ComponentTypes MAPC) — inside them
        // would corrupt the FIRST index's own stream instead.
        chunks.insert(dbfile + 5, dup);
        let err = MaterialIndex::build(&assemble_synthetic_cdb(&chunks))
            .expect_err("a duplicate DBFileIndex must be rejected");
        assert!(
            matches!(err, Error::DuplicateDbFileIndex),
            "expected DuplicateDbFileIndex, got {err:?}"
        );
    }

    /// #5320 — a stream instance without a Components row shifts the
    /// load-bearing join; every capture past the gap attributes materials
    /// to the wrong object. `build` must refuse rather than misattribute.
    #[test]
    fn build_rejects_a_row_instance_mismatch() {
        let (mut chunks, _) = mutable_chunks();
        // The trailing CTName instance duplicated: rows stay at 9, the
        // post-index instance stream grows to 10.
        let extra = chunks.last().expect("stream instances").clone();
        chunks.push(extra);
        let err = MaterialIndex::build(&assemble_synthetic_cdb(&chunks))
            .expect_err("an unlisted stream instance must break the join");
        assert!(
            matches!(err, Error::RowInstanceMismatch { rows: 9, instances: 10 }),
            "expected RowInstanceMismatch {{ rows: 9, instances: 10 }}, got {err:?}"
        );
    }

    /// #5320 — the DBFileIndex inline payload must end exactly at its
    /// last declared field, mirroring consume_object's trailing-bytes
    /// contract (stream_db_file_index had no such check).
    #[test]
    fn build_rejects_trailing_bytes_in_the_dbfile_index() {
        let (mut chunks, dbfile) = mutable_chunks();
        chunks[dbfile].1.push(0xAB);
        let err = MaterialIndex::build(&assemble_synthetic_cdb(&chunks))
            .expect_err("trailing bytes past the declared fields must be rejected");
        assert!(
            matches!(err, Error::ObjectTrailingBytes { leftover: 1 }),
            "expected ObjectTrailingBytes {{ leftover: 1 }}, got {err:?}"
        );
    }

    /// #5320 — a DIFF-shaped DBFileIndex was "supported" but misread (its
    /// inline fields were consumed in offset order, not diff field-index
    /// order); it must now be a loud typed error.
    #[test]
    fn build_rejects_a_diff_shaped_dbfile_index() {
        let (mut chunks, dbfile) = mutable_chunks();
        chunks[dbfile].0 = b"DIFF";
        let err = MaterialIndex::build(&assemble_synthetic_cdb(&chunks))
            .expect_err("a DIFF-shaped DBFileIndex must be rejected");
        assert!(
            matches!(
                err,
                Error::UnsupportedDbFileIndexChunk {
                    kind: ChunkType::Diff
                }
            ),
            "expected UnsupportedDbFileIndexChunk {{ Diff }}, got {err:?}"
        );
    }

    #[test]
    fn index_builds_and_resolves_the_full_join_chain() {
        let cdb = synthetic_material_cdb();
        let index = MaterialIndex::build(&cdb).expect("synthetic CDB indexes");
        assert_eq!(index.material_count(), 1, "one keyed material object");

        let mat = index
            .lookup("Materials\\Test\\Widget.mat")
            .expect("path resolves through the CRC key + PersistentID join");
        let slot = |s: u8| {
            mat.textures
                .iter()
                .find(|(k, _)| *k == s)
                .map(|(_, p)| p.clone())
        };
        assert_eq!(slot(SLOT_COLOR).as_deref(), Some("Data\\Textures\\widget_color.DDS"));
        assert_eq!(slot(SLOT_NORMAL).as_deref(), Some("Data\\Textures\\widget_normal.DDS"));
        assert_eq!(slot(SLOT_ROUGHNESS).as_deref(), Some("Data\\Textures\\widget_rough.DDS"));
        // #5190 — per-slot replacements, keyed by the texture-set
        // Components.Index: the fixture carries slot 0 (colour) and slot 1
        // (normal).
        assert_eq!(
            mat.flat_color_slots,
            vec![
                (SLOT_COLOR, [0.25, 0.5, 0.75, 1.0]),
                (SLOT_NORMAL, [0.1, 0.2, 0.3, 1.0]),
            ],
            "enabled TextureReplacements land per slot, not slot-agnostically"
        );
    }

    #[test]
    fn lookup_misses_return_none() {
        let cdb = synthetic_material_cdb();
        let index = MaterialIndex::build(&cdb).unwrap();
        assert!(index.lookup("materials\\test\\absent.mat").is_none());
        assert!(index.lookup("materials\\other\\widget.mat").is_none());
    }

    #[test]
    fn material_key_normalises_paths() {
        // Same key across case, separators, data\ prefix, and the
        // extension the reference happens to carry (.bgsm/.mat both key
        // on the stem — the CDB's ext column is always "mat").
        let a = material_key("Materials\\Test\\Widget.mat");
        assert_eq!(a, material_key("materials/test/widget.mat"));
        assert_eq!(a, material_key("Data\\Materials\\TEST\\Widget.BGSM"));
        assert_eq!(a, material_key("materials\\test\\widget"));
        // A different directory or stem is a different key.
        assert_ne!(a, material_key("materials\\other\\widget.mat"));
        assert_ne!(a, material_key("materials\\test\\gadget.mat"));
    }
}
