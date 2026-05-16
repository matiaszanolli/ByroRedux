//! BSA v103+ folder / file name hash functions.
//!
//! Only built under `#[cfg(any(debug_assertions, test))]` — production
//! release builds never invoke these; the parse path's hash-mismatch
//! warning is a debug-only diagnostic that turns into dead code in
//! release. See #622 / SK-D2-07.

/// Bethesda BSA v103+ folder-name hash. Used by the v103/v104/v105
/// directory tables to identify folders without scanning names.
///
/// Algorithm (lower-cased, UTF-8):
/// - `hash_low` packs: last char (b7..b0), second-to-last char (b15..b8),
///   length (b23..b16), first char (b31..b24).
/// - `hash_high` is a rolling `(h * 0x1003f) + c` over the middle chars
///   `[1 .. len-2)`.
///
/// **Caller contract**: `name` must already be ASCII-lowercased. Both
/// production callers (debug-only validation hooks at the folder-name
/// and file-name pass) pre-lowercase via `String::to_lowercase`, so an
/// inner `to_ascii_lowercase` collect was a no-op heap allocation per
/// entry — ~22k pointless allocs per Skyrim Meshes0 open in debug
/// builds. See #622 / SK-D2-02.
///
/// See UESP `Oblivion_Mod:BSA_File_Format#Hash_Calculation` and the
/// BSArch / libbsarch reference implementations. See #361.
#[cfg(any(debug_assertions, test))]
pub(super) fn genhash_folder(name: &[u8]) -> u64 {
    let len = name.len();

    let mut hash_low: u32 = 0;
    if len > 0 {
        hash_low |= name[len - 1] as u32;
    }
    if len >= 3 {
        hash_low |= (name[len - 2] as u32) << 8;
    }
    hash_low |= (len as u32) << 16;
    if len > 0 {
        hash_low |= (name[0] as u32) << 24;
    }

    let mut hash_high: u32 = 0;
    // Middle range `[1, len - 2)` — empty for len <= 3.
    if len > 3 {
        for &c in &name[1..len - 2] {
            hash_high = hash_high.wrapping_mul(0x1003f).wrapping_add(c as u32);
        }
    }

    ((hash_high as u64) << 32) | (hash_low as u64)
}

/// Bethesda BSA v103+ file-name hash. The stem uses the same algorithm
/// as `genhash_folder`; the extension contributes both a stem XOR (for a
/// handful of privileged extensions) and an extra rolling hash pass
/// that gets folded into the high word.
///
/// **Caller contract**: `name` must already be ASCII-lowercased — see
/// `genhash_folder` for rationale. `name` is the filename only — no
/// directory component.
#[cfg(any(debug_assertions, test))]
pub(super) fn genhash_file(name: &[u8]) -> u64 {
    let (stem_bytes, ext_bytes) = match name.iter().rposition(|&c| c == b'.') {
        Some(i) => (&name[..i], &name[i..]),
        None => (&name[..], &name[..0]),
    };

    // Base hash over the stem.
    let mut hash = genhash_folder(stem_bytes);

    // Extension adds a known XOR constant to the low word for the most
    // common asset types.
    let ext_xor: u32 = match ext_bytes {
        b".kf" => 0x80,
        b".nif" => 0x8000,
        b".dds" => 0x8080,
        b".wav" => 0x80000000,
        b".adp" => 0x00202e1a,
        _ => 0,
    };
    let hash_low = (hash as u32) ^ ext_xor;

    // Rolling hash over the whole extension (including the leading dot)
    // is computed INDEPENDENTLY from zero, then added into the stem's
    // high word. Pre-#449 this path folded the ext bytes on top of the
    // stem_high via sequential multiplication (`hash_high * 0x1003f + c`
    // starting from `stem_high`), which produces the wrong high word for
    // every file with stem length > 3. Low word matches either way so
    // HashMap lookup (path-keyed) worked, but the #361 debug-assertion
    // validation emitted 119k warnings per FO3 archive open.
    //
    // Verified against BSArchPro / libbsarch reference and a real FNV
    // stored hash: `meshes\armor\raiderarmor01\f\glover.nif` stores
    // `0xc86aec30_6706e572`; `rolling("lov") + rolling(".nif")` =
    // `0x359da633 + 0x92cd45fd` = `0xc86aec30` matches.
    let mut hash_ext = 0u32;
    for &c in ext_bytes {
        hash_ext = hash_ext.wrapping_mul(0x1003f).wrapping_add(c as u32);
    }
    let hash_high = ((hash >> 32) as u32).wrapping_add(hash_ext);

    hash = ((hash_high as u64) << 32) | (hash_low as u64);
    hash
}
