//! Shared sub-record helpers used by every record-type parser.
//!
//! Each parser in `records/` consumes a `&[SubRecord]` slice and walks it
//! to extract the fields it cares about. These helpers cover the patterns
//! that show up in every record: null-terminated strings, full-name lookups,
//! model paths, primitive reads at known offsets.

use crate::esm::reader::SubRecord;
use std::cell::Cell;

thread_local! {
    /// Tracks whether the plugin currently being parsed set the
    /// TES4 `Localized` flag (bit `0x80`). Set by
    /// `records::parse_esm_with_load_order` at the start of a parse
    /// pass and cleared at the end. Consulted by
    /// [`read_lstring_or_zstring`] when decoding FULL / DESC /
    /// similar lstring-bearing sub-records. See audit S6-03 / #348.
    static CURRENT_PLUGIN_LOCALIZED: Cell<bool> = const { Cell::new(false) };
}

/// Set the thread-local "this plugin uses lstring indirection" flag.
/// Prefer [`LocalizedPluginGuard`] over manual set/clear pairs — the
/// guard restores the previous value on drop (including unwind), so
/// a panic inside a parse pass can't leave the flag in an undefined
/// state for subsequent parses on the same thread. See audit S6-03 /
/// #348 / #624.
pub fn set_localized_plugin(flag: bool) {
    CURRENT_PLUGIN_LOCALIZED.with(|f| f.set(flag));
}

/// Read the thread-local localization flag. Exposed for unit tests
/// and defensive record parsers that want to branch on it.
pub fn is_localized_plugin() -> bool {
    CURRENT_PLUGIN_LOCALIZED.with(|f| f.get())
}

/// RAII guard that sets the localization flag for the duration of a
/// scope and restores the previous value on drop.
///
/// Replaces manual `set_localized_plugin(true)` / `set_localized_plugin(false)`
/// pairs across `parse_esm_with_load_order` (#348). Two failure modes
/// the guard closes (#624 / SK-D6-NEW-01):
///
///   1. **Panic inside parse**. A malformed record that triggers a
///      panic mid-walk used to leave the thread-local set to the last
///      plugin's `Localized` flag forever. The next parse on this
///      thread would inherit it and read FULL/DESC of a non-localized
///      FNV plugin through the lstring branch, returning
///      `<lstring 0x…>` placeholders instead of authored strings.
///
///   2. **Overlapping parses on the same thread**. Walking two ESMs
///      via `parse_esm` calls nested inside one another (e.g. a
///      master + DLC chain that re-enters the parser) clobbered the
///      outer parse's flag. The guard restores the prior value on
///      drop so nested calls are correctly stacked.
///
/// Use as `let _guard = LocalizedPluginGuard::new(localized);` at the
/// start of a parse function; let the binding fall out of scope at
/// the end (or on early-return / panic).
pub struct LocalizedPluginGuard {
    prev: bool,
}

impl LocalizedPluginGuard {
    /// Set the flag to `localized` for the duration of `self`'s scope.
    /// Captures the previous value so `Drop` can restore it.
    pub fn new(localized: bool) -> Self {
        let prev = is_localized_plugin();
        set_localized_plugin(localized);
        Self { prev }
    }
}

impl Drop for LocalizedPluginGuard {
    fn drop(&mut self) {
        set_localized_plugin(self.prev);
    }
}

/// Read a null-terminated ASCII string from a sub-record's data buffer.
/// Trailing bytes after the first NUL are ignored. Returns `String::new()`
/// for empty buffers.
pub fn read_zstring(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).to_string()
}

/// Decode an lstring-bearing sub-record payload.
///
/// When the current plugin is localized (`FileHeader.localized == true`,
/// stashed in the thread-local above) and the payload is exactly 4
/// bytes, interpret those bytes as a little-endian u32 lstring-table
/// index and return `"<lstring 0xNNNNNNNN>"` so downstream callers
/// that naively `.as_str()` the field see a stable placeholder
/// instead of 3-character UTF-8 garbage.
///
/// Otherwise delegates to [`read_zstring`] — every non-localized
/// plugin and every non-4-byte payload reads as the usual inline
/// z-string.
///
/// Used at FULL / DESC / RNAM / CNAM / NAM1 / ITXT / EPF2 / MICO /
/// SHRT / DESC sites throughout `records/*` to close the gap where
/// Skyrim.esm's `u32 0x00012345` was interpreted as the 3-character
/// cstring `"E#\x01"` — corrupting every item / NPC / faction name.
/// Phase 2 (the real `.STRINGS` loader that turns the placeholder
/// into the authored English / language-pack string) is tracked as
/// a follow-up. See audit S6-03 / #348.
pub fn read_lstring_or_zstring(data: &[u8]) -> String {
    if is_localized_plugin() && data.len() == 4 {
        let id = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        return format!("<lstring 0x{:08X}>", id);
    }
    read_zstring(data)
}

/// Find a sub-record by 4-char type code and return its data slice.
pub fn find_sub<'a>(subs: &'a [SubRecord], code: &[u8; 4]) -> Option<&'a [u8]> {
    subs.iter()
        .find(|s| &s.sub_type == code)
        .map(|s| s.data.as_slice())
}

/// Read a sub-record as a null-terminated string. Returns `None` if absent.
pub fn read_string_sub(subs: &[SubRecord], code: &[u8; 4]) -> Option<String> {
    find_sub(subs, code).map(read_zstring)
}

pub fn read_u32_sub(subs: &[SubRecord], code: &[u8; 4]) -> Option<u32> {
    let data = find_sub(subs, code)?;
    if data.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

pub fn read_f32_sub(subs: &[SubRecord], code: &[u8; 4]) -> Option<f32> {
    let data = find_sub(subs, code)?;
    if data.len() < 4 {
        return None;
    }
    Some(f32::from_le_bytes([data[0], data[1], data[2], data[3]]))
}

/// Read a u32 form ID at a known byte offset within a sub-record's data.
pub fn read_u32_at(data: &[u8], offset: usize) -> Option<u32> {
    if data.len() < offset + 4 {
        return None;
    }
    Some(u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

/// Read a u16 at a known byte offset within a sub-record's data.
pub fn read_u16_at(data: &[u8], offset: usize) -> Option<u16> {
    if data.len() < offset + 2 {
        return None;
    }
    Some(u16::from_le_bytes([data[offset], data[offset + 1]]))
}

/// Read an i16 at a known byte offset.
pub fn read_i16_at(data: &[u8], offset: usize) -> Option<i16> {
    if data.len() < offset + 2 {
        return None;
    }
    Some(i16::from_le_bytes([data[offset], data[offset + 1]]))
}

/// Read an f32 at a known byte offset.
pub fn read_f32_at(data: &[u8], offset: usize) -> Option<f32> {
    if data.len() < offset + 4 {
        return None;
    }
    Some(f32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

/// Common name+model+value+weight bundle that nearly every item record carries.
/// Filled in by walking sub-records once before the type-specific dispatch.
#[derive(Debug, Default, Clone)]
pub struct CommonItemFields {
    pub editor_id: String,
    pub full_name: String,
    pub model_path: String,
    pub icon_path: String,
    /// Legacy attached-script reference (`SCRI`, Oblivion / FO3 / FNV).
    /// Form ID of the SCPT record bound to this item. Skyrim+ records
    /// use `VMAD` (Papyrus VM attached data) instead — see `has_script`.
    pub script_form_id: u32,
    pub value: u32,
    pub weight: f32,
    /// True when the record carries a `VMAD` sub-record — Skyrim+'s
    /// Papyrus VM attached-script blob. Full VMAD decoding (script
    /// names + property bindings) is gated on the scripting-as-ECS
    /// work tracked at M30.2 / M48; for now this flag at least makes
    /// the count of script-bearing records discoverable. See #369.
    pub has_script: bool,
}

impl CommonItemFields {
    /// Walk a sub-record list and pull out the universal item fields. Each
    /// type-specific parser starts from this and then handles its own DNAM /
    /// type-specific blocks.
    pub fn from_subs(subs: &[SubRecord]) -> Self {
        let mut out = Self::default();
        for sub in subs {
            match &sub.sub_type {
                // EDID is always an inline cstring — not localized.
                b"EDID" => out.editor_id = read_zstring(&sub.data),
                // FULL is an lstring on Skyrim-localized plugins (#348).
                b"FULL" => out.full_name = read_lstring_or_zstring(&sub.data),
                b"MODL" => out.model_path = read_zstring(&sub.data),
                b"ICON" => out.icon_path = read_zstring(&sub.data),
                b"SCRI" if sub.data.len() >= 4 => {
                    out.script_form_id = read_u32_at(&sub.data, 0).unwrap_or(0);
                }
                // VMAD presence-only flag — see `has_script` field doc.
                b"VMAD" => out.has_script = true,
                _ => {}
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    /// Regression: #369 — VMAD presence on item records flips
    /// `has_script`. Full Papyrus VM data decoding is deferred.
    #[test]
    fn item_vmad_flips_has_script() {
        let subs = vec![
            sub(b"EDID", b"ScriptedItem\0"),
            sub(b"VMAD", b"\x05\x00\x02\x00\x00\x00"),
        ];
        let c = CommonItemFields::from_subs(&subs);
        assert!(c.has_script);
        assert_eq!(c.editor_id, "ScriptedItem");
    }

    #[test]
    fn item_without_vmad_has_script_false() {
        let subs = vec![sub(b"EDID", b"PlainItem\0")];
        let c = CommonItemFields::from_subs(&subs);
        assert!(!c.has_script);
    }

    // ── #348 / audit S6-03 — lstring helper regression tests ────────
    //
    // The thread-local guard means these tests run one-at-a-time
    // (single-threaded within the default `cargo test` harness is fine
    // since the flag lives in `thread_local!`). Each test sets + clears
    // the flag so ordering doesn't leak into unrelated cases in this
    // module.

    /// On a non-localized plugin (pre-Skyrim, or Skyrim without the
    /// flag set), `read_lstring_or_zstring` must be a pass-through to
    /// `read_zstring`. Guards against an incorrect placeholder firing
    /// on legitimate inline names.
    #[test]
    fn read_lstring_or_zstring_non_localized_reads_inline_cstring() {
        set_localized_plugin(false);
        let data = b"IronSword\0";
        assert_eq!(read_lstring_or_zstring(data), "IronSword");
    }

    /// On a localized plugin, a 4-byte FULL payload is an lstring
    /// table reference — render as `<lstring 0xNNNNNNNN>`. Mirrors
    /// the Skyrim.esm `0x00012345` example the audit calls out as
    /// the `"E#\x01"` corruption source.
    #[test]
    fn read_lstring_or_zstring_localized_4_bytes_is_placeholder() {
        set_localized_plugin(true);
        let data = [0x45u8, 0x23, 0x01, 0x00]; // u32 LE = 0x00012345
        assert_eq!(read_lstring_or_zstring(&data), "<lstring 0x00012345>");
        set_localized_plugin(false);
    }

    /// Localized plugins sometimes ship legitimate inline strings on
    /// sub-records that AREN'T lstring-indirected — the helper only
    /// triggers the placeholder on exactly-4-byte payloads. Any other
    /// length (including 3-byte "abc\0" counting as a trailing-null
    /// cstring — payload 3 bytes of data + an implicit NUL isn't how
    /// subrecords work; always explicit) falls through to the inline
    /// path.
    #[test]
    fn read_lstring_or_zstring_localized_wrong_size_falls_through() {
        set_localized_plugin(true);
        // 5 bytes: not lstring-shaped, read as regular cstring.
        let data = b"hi!\0\0";
        assert_eq!(read_lstring_or_zstring(data), "hi!");
        set_localized_plugin(false);
    }

    /// Empty payload must NOT hit the placeholder path — it's
    /// well-defined as an empty string regardless of localization.
    #[test]
    fn read_lstring_or_zstring_empty_payload_returns_empty_string() {
        set_localized_plugin(true);
        assert_eq!(read_lstring_or_zstring(&[]), "");
        set_localized_plugin(false);
    }

    /// Regression for the full integration path: CommonItemFields
    /// routes FULL through the lstring helper. On a localized plugin
    /// with 4-byte FULL payload, the item record's `full_name`
    /// surfaces as `<lstring 0x…>` instead of three garbage UTF-8
    /// characters. Pre-#348 this produced names like `"E#\x01"`.
    #[test]
    fn common_item_fields_localized_full_becomes_lstring_placeholder() {
        set_localized_plugin(true);
        let subs = vec![
            sub(b"EDID", b"WeapIronSword\0"),
            sub(b"FULL", &[0x45u8, 0x23, 0x01, 0x00]),
        ];
        let c = CommonItemFields::from_subs(&subs);
        assert_eq!(c.editor_id, "WeapIronSword");
        assert_eq!(c.full_name, "<lstring 0x00012345>");
        set_localized_plugin(false);
    }

    /// Symmetric guard: clearing the localization flag after a
    /// localized parse must route subsequent parses back through the
    /// inline cstring path. Pins the "clear on exit" invariant
    /// `parse_esm_with_load_order` relies on so stale state doesn't
    /// leak across plugin boundaries.
    #[test]
    fn set_localized_plugin_toggle_clears_stale_state() {
        set_localized_plugin(true);
        assert!(is_localized_plugin());
        set_localized_plugin(false);
        assert!(!is_localized_plugin());
        // Post-clear FULL reads as inline cstring.
        let subs = vec![sub(b"FULL", b"PlainName\0")];
        let c = CommonItemFields::from_subs(&subs);
        assert_eq!(c.full_name, "PlainName");
    }

    /// Regression for #624 / SK-D6-NEW-01. Pre-fix the manual
    /// set/clear pair around `parse_esm_with_load_order` could leak
    /// state in two ways:
    ///   1. A panic mid-parse skipped the clear, leaving the
    ///      thread-local set forever.
    ///   2. Nested / overlapping parses on the same thread clobbered
    ///      the outer parse's flag.
    /// The `LocalizedPluginGuard` restores the previous value on drop
    /// (including unwind), closing both holes.
    #[test]
    fn localized_plugin_guard_restores_value_on_panic_unwind() {
        // Establish baseline.
        set_localized_plugin(false);
        assert!(!is_localized_plugin());

        // Run a scope that creates a guard, then panics. `catch_unwind`
        // captures the panic; the guard's Drop runs during the unwind
        // and must restore the prior `false` value.
        let result = std::panic::catch_unwind(|| {
            let _guard = LocalizedPluginGuard::new(true);
            assert!(is_localized_plugin(), "guard set the flag");
            panic!("simulated panic mid-parse");
        });
        assert!(result.is_err(), "panic must propagate");
        assert!(
            !is_localized_plugin(),
            "guard's Drop must restore the prior value on panic unwind"
        );
    }

    #[test]
    fn localized_plugin_guard_restores_value_on_normal_drop() {
        set_localized_plugin(false);
        {
            let _guard = LocalizedPluginGuard::new(true);
            assert!(is_localized_plugin());
        }
        assert!(
            !is_localized_plugin(),
            "guard's Drop must restore the prior value at end of scope"
        );
    }

    /// Nested guards must stack — the inner guard restores to the
    /// outer's value, not blanket false. Pre-fix the explicit `false`
    /// clear at the end of `parse_esm_with_load_order` collapsed any
    /// nested parse's flag regardless of caller intent.
    #[test]
    fn localized_plugin_guard_nests_correctly() {
        set_localized_plugin(false);
        {
            let _outer = LocalizedPluginGuard::new(true);
            assert!(is_localized_plugin(), "outer set true");
            {
                let _inner = LocalizedPluginGuard::new(false);
                assert!(!is_localized_plugin(), "inner overrode to false");
            }
            // Inner dropped — must restore to outer's true, NOT to a
            // blanket false.
            assert!(
                is_localized_plugin(),
                "inner Drop must restore to outer's true, not false"
            );
        }
        assert!(!is_localized_plugin(), "outer Drop restores baseline");
    }
}
