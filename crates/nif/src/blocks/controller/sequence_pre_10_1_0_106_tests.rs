//! Byte-exact regression coverage for #2345 / NIF-OBL-D1-02 — the
//! pre-10.1.0.106 `NiSequence` / `ControlledBlock` layout.
//!
//! Before this fix the parser implemented only the `>= 10.1.0.104`
//! `ControlledBlock` shape and read every `NiControllerSequence` field
//! unconditionally. On a sub-10.1.0.106 file that meant:
//!
//! * the `until="10.1.0.103"` `NiSequence` prologue (Accum Root Name +
//!   Text Keys) was never read — an under-read;
//! * `Target Name` (`until="10.1.0.103"`) was never read — an under-read;
//! * `Interpolator` (`since="10.1.0.106"`) was read anyway — an over-read;
//! * `Priority` was gated on `bsver > 0` alone, missing its
//!   `since="10.1.0.106"` half — a phantom byte on Bethesda content;
//! * the five IDTag strings (`since="10.1.0.104"`) were read anyway;
//! * every `NiControllerSequence` field (all `since="10.1.0.106"`) was
//!   read anyway — ~30 phantom bytes.
//!
//! Those errors do not cancel out, and this version band has no
//! per-block size anchor to resynchronise from, so the whole rest of the
//! file was lost. Gates transcribed from `nif.xml`
//! (`<niobject name="NiSequence">` / `<niobject name="NiControllerSequence">`
//! / `<struct name="ControlledBlock">`), the authority named in
//! `docs/engine/` for wire-format questions.
//!
//! Empirically unreached on vanilla content — Oblivion's sub-10.1.0.106
//! files carry no controller sequences — so this test is the only thing
//! exercising the band. Exposure is mod / non-Bethesda Gamebryo content.

use super::sequence::NiControllerSequence;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;

/// v10.1.0.101 with `bsver = 4` — the exact configuration the issue names:
/// old enough that every gate above excludes its field, but Bethesda-flagged
/// (`bsver > 0`) so the `Priority` byte would have been read on the old
/// `bsver`-only gate.
fn pre_106_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V10_1_0_101,
        little_endian: true,
        user_version: 0,
        user_version_2: 4,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

fn push_inline_string(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

#[test]
fn pre_10_1_0_106_sequence_layout_is_byte_exact() {
    let mut data = Vec::new();

    // ── NiSequence ──
    push_inline_string(&mut data, "Seq"); //  7 B  Name
    push_inline_string(&mut data, "Root"); //  8 B  Accum Root Name (until 10.1.0.103)
    data.extend_from_slice(&3i32.to_le_bytes()); //  4 B  Text Keys ref (until 10.1.0.103)
                                                 //        #3468: a REAL index, not -1. The
                                                 //        original fixture wrote NULL, so the
                                                 //        `is_null()` assertion below passed
                                                 //        both before and after the payload was
                                                 //        carried forward — it could not fail.
    data.extend_from_slice(&1u32.to_le_bytes()); //  4 B  Num Controlled Blocks
                                                 //        Array Grow By ABSENT (since 10.1.0.106)

    // ── ControlledBlock[0] ──
    push_inline_string(&mut data, "Target"); // 10 B  Target Name (until 10.1.0.103)
                                             //        Interpolator ABSENT (since 10.1.0.106)
    data.extend_from_slice(&7i32.to_le_bytes()); //  4 B  Controller ref
                                                 //        Blend Interpolator/Index ABSENT (since 10.1.0.104)
                                                 //        Priority ABSENT (since 10.1.0.106)
                                                 //        IDTag strings ABSENT (since 10.1.0.104)

    // ── NiControllerSequence's own fields: ALL ABSENT (since 10.1.0.106) ──
    // ── Deprecated string palette ABSENT (since 10.1.0.113) ──
    // ── Anim notes ABSENT (bsver 4 < 24) ──

    let expected_len = 7 + 8 + 4 + 4 + 10 + 4;
    assert_eq!(
        data.len(),
        expected_len,
        "fixture bookkeeping: the hand-counted layout and the emitted bytes disagree"
    );

    let header = pre_106_header();
    let mut stream = NifStream::new(&data, &header);
    let seq = NiControllerSequence::parse(&mut stream).expect("pre-10.1.0.106 sequence parses");

    // The load-bearing assertion: the parser consumed the block EXACTLY.
    // In this version band there is no per-block size to resynchronise on,
    // so any drift here silently destroys every following block.
    assert_eq!(
        stream.position(),
        expected_len as u64,
        "stream must land exactly on the block end — over/under-read here is \
         unrecoverable in a sizeless-format version band (#2345)"
    );

    assert_eq!(seq.name.as_deref(), Some("Seq"));
    // Sourced from the NiSequence base field, not the derived class's
    // same-named one (which is `since=10.1.0.106` and absent here).
    assert_eq!(seq.accum_root_name.as_deref(), Some("Root"));
    assert_eq!(seq.array_grow_by, 0, "Array Grow By is since=10.1.0.106");

    assert_eq!(seq.controlled_blocks.len(), 1);
    let cb = &seq.controlled_blocks[0];
    assert!(
        cb.interpolator_ref.is_null(),
        "Interpolator is since=10.1.0.106 — must not be read, and must not \
         alias the Controller ref that follows it on disk"
    );
    assert_eq!(
        cb.controller_ref.index(),
        Some(7),
        "the Controller ref must land on its own u32, not one shifted by a \
         phantom Interpolator read"
    );
    assert_eq!(cb.priority, 0, "Priority is since=10.1.0.106 && #BSSTREAM#");
    // #3468 SIBLING — also retargeted. The IDTag strings really are
    // `since="10.1.0.104"` and absent here, but `Target Name`
    // (`until="10.1.0.103"`, read in the prologue) is the same
    // "which node does this block drive" concept for this band. Leaving
    // `node_name` None short-circuited `anim/controlled_block.rs`'s target
    // resolution, so no channel in the sequence bound at all.
    assert_eq!(
        cb.node_name.as_deref(),
        Some("Target"),
        "#3468: Target Name must back node_name below 10.1.0.104"
    );
    assert!(cb.interpolator_id.is_none());

    // nif.xml's own defaults for the absent derived-class fields.
    assert_eq!(seq.weight, 1.0);
    assert_eq!(seq.frequency, 1.0);
    assert_eq!(seq.cycle_type, 0, "CYCLE_CLAMP");
    // #3468 — retargeted from `is_null()`. The prologue ref
    // (`until="10.1.0.103"`) is the NiSequence-side declaration of the field
    // NiControllerSequence re-declares `since="10.1.0.106"`; exactly one is
    // present for any version, so it must reach the same slot the way
    // `accum_root_name` already does. Dropping it yielded zero text-key
    // events (footstep / hit / sound) for every sequence on this band.
    assert_eq!(
        seq.text_keys_ref.index(),
        Some(3),
        "#3468: the pre-10.1.0.104 Text Keys ref must back text_keys_ref, \
         mirroring Accum Root Name backing accum_root_name"
    );
    assert!(seq.manager_ref.is_null());
    assert!(
        seq.anim_note_refs.is_empty(),
        "bsver 4 carries no anim notes"
    );
}
