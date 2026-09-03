//! [`Snapshot`] — the serialised world state — and the versioned binary
//! container ([`encode`] / [`decode`]) that wraps it with integrity and
//! version metadata.
//!
//! ## Container layout (little-endian)
//!
//! ```text
//! offset  size  field
//! 0       8     magic           b"BYRSAVE\0"
//! 8       2     version_major   reject on mismatch
//! 10      2     version_minor   advisory; newer minor still loads
//! 12      8     schema_fpr      registry fingerprint (type-set drift)
//! 20      4     crc32           over the payload bytes only
//! 24      8     payload_len     length of the JSON payload
//! 32      …     payload         serde_json of `Snapshot`
//! ```
//!
//! The payload is JSON (the "simple serde format" v1 mandates); the
//! framing is binary so corruption, truncation, and version skew are
//! detected before any deserialisation is attempted.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::SaveError;

/// Container magic bytes.
pub const FORMAT_MAGIC: &[u8; 8] = b"BYRSAVE\0";
/// Incompatible-format version. Bumped only when old saves can't be read.
///
/// # SAVE-D2-01 invariant — intra-type shape changes need a MAJOR bump
///
/// [`SaveRegistry::schema_fingerprint`](crate::SaveRegistry::schema_fingerprint)
/// is deliberately coarse: it hashes the ordered set of column *type keys*,
/// not field layout, so it catches add/remove/rename of a *type* but **not**
/// a field change *within* a saved type. The intended backstop for intra-type
/// change is `serde_json::from_value` failing at load — but that only fires
/// when the new field is *required*. A field added with `#[serde(default)]`,
/// or as `Option<T>` (serde silently defaults a missing `Option` to `None`),
/// loads an OLD save **silently default-filled** instead of rejecting it.
///
/// Until a versioned migrator chain exists, the only safe way to change the
/// serialised shape of any save-participating struct — including types
/// *nested* inside a saved column (e.g. an `Inventory` item, an
/// `AnimationStack` layer) — is to bump `FORMAT_MAJOR` here, which `decode`
/// rejects across. A `serde_default_on_saved_struct_requires_format_major_bump`
/// guard test (in the binary's `save_io`, beside `build_save_registry`) trips
/// on the `#[serde(default)]` half of this footgun; the new-`Option` half
/// rides this doc rule (legitimate `Option`s — e.g. `AnimationStack::root_entity`
/// — already exist, so it can't be caught statically). See #1714.
/// Version 2 invalidates version-1 snapshots that serialized Skyrim Health
/// under engine-enum key `24`. ActorValues now use the authored/remapped
/// `AVHealth` FormID in the same key space as CTDA `GetActorValue`.
/// Version 3 makes quest lifecycle fields required. Version 2 briefly added
/// them with `serde(default)`, which allowed older snapshots to load with
/// silently invented state (#3020).
/// Version 4 adds required `reach`/`speed` fields to `EquippedWeapon`
/// (#3096). Pre-v4 saves have no on-disk value for either — rejecting them
/// outright is correct here (per this doc's rule) rather than defaulting
/// to `0.0`, which is indistinguishable from an authored "undecoded" weapon.
/// Version 5 adds required `Material.water_shader_flags`,
/// `Material.is_water_shader`, `RigidBodyData.collidable`, and the saved
/// `CharacterController` column (#3164/#3165).
/// Version 6 adds required `Material.texture_clamp_mode`,
/// `Material.src_blend_mode`, `Material.dst_blend_mode` — the NIFAL
/// canonical-boundary fields added by #2571 (OBL-D5-01).
/// Version 7 adds required canonical glass-optics and Bethesda authored
/// lighting-response fields to saved `Material` / material-texture state.
/// Older snapshots cannot reconstruct those source values safely.
/// Version 8 moves `EquipmentSlots`' wielded weapon out of biped occupancy
/// bit 31 into a required `weapon` field (#3112). Bit 31 is a real
/// authorable Skyrim+ `BOD2` slot (body-part 61 / `FX01`), so pre-v8 saves
/// cannot distinguish "an armor occupies slot 61" from "this is the wielded
/// weapon" — the ambiguity the field split exists to remove. Defaulting the
/// new field would re-introduce exactly that guess, so old snapshots are
/// rejected instead.
/// Version 9 adds a required `Seated.animation_restore` — the actor's
/// `AnimationPlayer` playback state as it stood *before* `sandbox_seat_system`
/// parked it on the sit-enter clip's final frame (#3333). Both `Seated` and
/// `AnimationPlayer` are saved columns, so a pre-v9 snapshot of a seated actor
/// persists the *parked* player (`playing = false`, pinned on the last frame)
/// with no record of what preceded it. Un-seating such an actor after load —
/// which `ambient_ai_package_system` does on any schedule handover — would
/// have nothing to restore, and the actor walks its next package frozen in a
/// chair pose for the rest of the session. Defaulting the new field would be a
/// guess at the actor's idle clip and phase, the exact footgun this doc's
/// `Option` / `serde(default)` rule exists to prevent, so old snapshots are
/// rejected instead.
/// Version 10 adds a required `Material.parallax_height_in_alpha` — the
/// NIFAL canonical-boundary flag recording that a material's height map
/// carries its values in the texture's alpha channel rather than `.r` (#3530,
/// Oblivion's `APPLY_HILIGHT2` parallax route). Same class as the v5/v6/v7
/// `Material` additions.
///
/// Unlike v8 and v9, `false` would in fact have been the correct value for
/// every pre-v10 snapshot: no build before this one could detect Oblivion
/// parallax, so no saved material was ever in the alpha-height state. The
/// bump is taken anyway because this doc's rule and its
/// `serde_default_on_saved_struct_requires_format_major_bump` guard are
/// deliberately blanket — a per-field "but this default is safe" judgement is
/// the exact reasoning #1714 (SAVE-D2-01) removed from the loop, and the
/// judgement is only safe while the field's meaning never changes. Recorded
/// here so a future migrator chain can reclassify this one as compatible
/// rather than re-derive whether it was.
/// Version 11 changes saved `PapyrusProviderContinuationQueue` calls from a
/// flat vector of already-resolved values to typed literal/local argument
/// expressions. The old flat JSON shape does not deserialize into the tagged
/// argument enum, and no migrator exists yet, so pre-v11 saves are rejected.
/// Version 12 adds the stable legacy-script principal to every suspended
/// provider continuation. Resuming without it would collapse principal-private
/// compatibility storage into an unauthenticated global namespace.
/// Version 13 extends the same ownership requirement to provider barriers
/// embedded in saved quest/scene fragment continuations.
/// Version 14 adds principal-owned, in-progress ModEvent builder handles to
/// extension state so a suspended Papyrus handler can resume them safely.
/// Version 15 adds fixed SendModEvent statements with a resolved portable
/// sender to saved provider tails. Pre-v15 continuations cannot represent that
/// tagged instruction and are rejected rather than resumed anonymously.
/// Version 16 adds explicit JContainers retain counts and recovery tags to the
/// principal-private container registry. Older saves cannot reconstruct those
/// ownership relationships without risking premature collection or leaks.
/// Version 17 adds typed scalar provider expressions to saved continuation
/// tails. Older saves cannot represent the expression node without silently
/// changing a resumed handler's result.
/// Version 18 adds computed provider receiver arguments and their proven
/// object types to saved continuation tails. Older saves cannot reconstruct
/// the receiver expression or its dispatch type without changing a resumed
/// handler's result, so they are rejected rather than default-filled.
/// Version 19 (#3073, NIFAL-D1) adds `Material::parallax_height_scale` /
/// `parallax_max_passes` as required fields on the registered `Material`
/// column, resolved once at the parser→`Material` boundary instead of
/// re-derived `.unwrap_or(0.04)`/`.unwrap_or(4.0)` at each spawn site. Like
/// v10's `parallax_height_in_alpha`, the shared defaults happen to be the
/// correct value for every pre-v19 snapshot — the bump is taken anyway per
/// the same blanket rule that field's doc explains.
/// Version 20 (#3701, ECS-2026-08-30-D10-01) adds `AnimationLayer::
/// blend_in_target` — the weight a blending-in layer ramps toward — as a
/// required field on the registered `AnimationStack` column's nested layer
/// type (this doc's own "an AnimationStack layer" example, above). Fixes a
/// latent bug where `with_blend_in` zeroed `weight` itself and
/// `effective_weight()` multiplied that zero by the ramp progress, so a
/// blending-in layer contributed nothing until snapping to full weight on
/// completion; `weight` is now the live per-tick value and
/// `blend_in_target` is what it ramps toward. No pre-v20 snapshot has a
/// correct value to infer here — the field didn't exist for `weight` to be
/// derived from — so old snapshots are rejected rather than guessed at.
/// Version 21 (#2573, OBL-D5-03) adds `Material::specular_authored` as a
/// required field on the registered `Material` column — whether
/// `specular_color` was actually authored by a bound `NiMaterialProperty` /
/// `BSLightingShaderProperty`, as opposed to still holding the unauthored
/// struct default. Like v10's `parallax_height_in_alpha` and v19's
/// `parallax_height_scale`/`parallax_max_passes`, `false` happens to be the
/// correct value for every pre-v21 snapshot (every prior build's
/// `resolve_pbr` treated the field as unconditionally absent), but the
/// bump is taken anyway per this doc's blanket rule.
pub const FORMAT_MAJOR: u16 = 21;
/// Additive-format version. Bumped when fields are added compatibly.
pub const FORMAT_MINOR: u16 = 0;

/// Fixed header size in bytes (everything before the payload).
const HEADER_LEN: usize = 32;

/// The serialised world state.
///
/// Columns are keyed by the stable registry name. `BTreeMap` keeps the
/// JSON output deterministic (stable diffs, reproducible CRCs across
/// runs at equal state).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// The entity-id high-water mark at save time. Restored verbatim so
    /// later spawns stay monotonic and saved inter-entity references stay
    /// valid without remapping.
    pub next_entity: u32,
    /// `StringPool` dump in symbol order — re-interning this exact
    /// sequence reproduces every `FixedString` symbol the components
    /// reference.
    pub strings: Vec<String>,
    /// One column per component type: `name → [[entity, value], …]`.
    pub components: BTreeMap<String, serde_json::Value>,
    /// One blob per saved resource: `name → value`.
    pub resources: BTreeMap<String, serde_json::Value>,
}

impl Snapshot {
    /// Total live component rows across all columns — a cheap size/sanity
    /// signal for logging.
    pub fn row_count(&self) -> usize {
        self.components
            .values()
            .map(|v| v.as_array().map_or(0, |a| a.len()))
            .sum()
    }
}

/// Wrap a [`Snapshot`] in the versioned, CRC-protected binary container.
///
/// `schema_fpr` is [`SaveRegistry::schema_fingerprint`](crate::SaveRegistry::schema_fingerprint)
/// — stored so [`decode`] can refuse a save written against a different
/// type set.
pub fn encode(snapshot: &Snapshot, schema_fpr: u64) -> Result<Vec<u8>, SaveError> {
    let payload = serde_json::to_vec(snapshot).map_err(|source| SaveError::Serde {
        column: "<snapshot>".to_string(),
        source,
    })?;
    let crc = crc32fast::hash(&payload);

    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(FORMAT_MAGIC);
    out.extend_from_slice(&FORMAT_MAJOR.to_le_bytes());
    out.extend_from_slice(&FORMAT_MINOR.to_le_bytes());
    out.extend_from_slice(&schema_fpr.to_le_bytes());
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Validate the container and return the contained [`Snapshot`].
///
/// `expected_fpr` is the live registry's fingerprint; a mismatch is
/// refused ([`SaveError::SchemaMismatch`]) since no migrator chain exists
/// yet. Checks run header-first so a truncated/corrupt/version-skewed
/// file fails before any JSON parse.
pub fn decode(bytes: &[u8], expected_fpr: u64) -> Result<Snapshot, SaveError> {
    if bytes.len() < HEADER_LEN {
        return Err(SaveError::Truncated(bytes.len(), HEADER_LEN));
    }
    if &bytes[0..8] != FORMAT_MAGIC {
        return Err(SaveError::BadMagic);
    }
    let major = u16::from_le_bytes([bytes[8], bytes[9]]);
    if major != FORMAT_MAJOR {
        return Err(SaveError::UnsupportedVersion {
            found: major,
            supported: FORMAT_MAJOR,
        });
    }
    // minor (bytes[10..12]) is advisory — a newer minor still loads; serde
    // default-fills any field this engine's `Snapshot` doesn't carry.
    let file_fpr = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
    if file_fpr != expected_fpr {
        return Err(SaveError::SchemaMismatch {
            file: file_fpr,
            engine: expected_fpr,
        });
    }
    let stored_crc = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    let payload_len = u64::from_le_bytes(bytes[24..32].try_into().unwrap()) as usize;

    let payload_end = HEADER_LEN
        .checked_add(payload_len)
        .ok_or(SaveError::Truncated(bytes.len(), usize::MAX))?;
    if bytes.len() < payload_end {
        return Err(SaveError::Truncated(bytes.len(), payload_end));
    }
    let payload = &bytes[HEADER_LEN..payload_end];

    let computed_crc = crc32fast::hash(payload);
    if computed_crc != stored_crc {
        return Err(SaveError::CrcMismatch {
            stored: stored_crc,
            computed: computed_crc,
        });
    }

    serde_json::from_slice(payload).map_err(|source| SaveError::Serde {
        column: "<snapshot>".to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Snapshot {
        let mut components = BTreeMap::new();
        components.insert(
            "Transform".to_string(),
            serde_json::json!([[0, {"x": 1.0}]]),
        );
        Snapshot {
            next_entity: 7,
            strings: vec!["scene root".into(), "bip01".into()],
            components,
            resources: BTreeMap::new(),
        }
    }

    #[test]
    fn round_trips_through_container() {
        let snap = sample();
        let bytes = encode(&snap, 0xABCD).unwrap();
        let back = decode(&bytes, 0xABCD).unwrap();
        assert_eq!(back.next_entity, 7);
        assert_eq!(back.strings, snap.strings);
        assert_eq!(back.row_count(), 1);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut bytes = encode(&sample(), 1).unwrap();
        bytes[0] = b'X';
        assert!(matches!(decode(&bytes, 1), Err(SaveError::BadMagic)));
    }

    #[test]
    fn rejects_truncated() {
        let bytes = encode(&sample(), 1).unwrap();
        assert!(matches!(
            decode(&bytes[..20], 1),
            Err(SaveError::Truncated(20, HEADER_LEN))
        ));
    }

    #[test]
    fn rejects_payload_truncation() {
        let bytes = encode(&sample(), 1).unwrap();
        // Drop the last payload byte — header says more than is present.
        let chopped = &bytes[..bytes.len() - 1];
        assert!(matches!(
            decode(chopped, 1),
            Err(SaveError::Truncated(_, _))
        ));
    }

    #[test]
    fn detects_crc_corruption() {
        let mut bytes = encode(&sample(), 1).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        assert!(matches!(
            decode(&bytes, 1),
            Err(SaveError::CrcMismatch { .. })
        ));
    }

    #[test]
    fn rejects_schema_mismatch() {
        let bytes = encode(&sample(), 0x1111).unwrap();
        assert!(matches!(
            decode(&bytes, 0x2222),
            Err(SaveError::SchemaMismatch {
                file: 0x1111,
                engine: 0x2222
            })
        ));
    }

    #[test]
    fn rejects_major_version_skew() {
        let mut bytes = encode(&sample(), 1).unwrap();
        // Bump the stored major past what the engine supports.
        let bad = (FORMAT_MAJOR + 1).to_le_bytes();
        bytes[8] = bad[0];
        bytes[9] = bad[1];
        // CRC is over the payload only, so the header edit doesn't trip CRC.
        assert!(matches!(
            decode(&bytes, 1),
            Err(SaveError::UnsupportedVersion { .. })
        ));
    }
}
