//! Regression coverage for #633 / FNV-D1 — `BhkBreakableConstraint`
//! must read its `threshold` + `remove_when_broken` trailer fields
//! on FNV/FO3 too, not only on Oblivion. The wrapped-CInfo size
//! table is now version-aware so the parser doesn't over- or
//! under-consume on `bsver != 20.0.0.5`.
use super::*;
use crate::header::NifHeader;
use crate::version::NifVersion;

fn oblivion_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_0_0_5,
        little_endian: true,
        user_version: 11,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

fn fnv_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 11,
        user_version_2: 34,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

/// Build the fixed prefix every BhkBreakableConstraint shares:
/// outer bhkConstraintCInfo (16 B) + wrapped_type u32 + inner
/// bhkConstraintCInfo (16 B). Returns 36 bytes — the per-block
/// size before the wrapped payload.
fn shared_prefix(wrapped_type: u32) -> Vec<u8> {
    let mut d = Vec::with_capacity(36);
    // Outer bhkConstraintCInfo: num_entities + entity_a + entity_b + priority.
    d.extend_from_slice(&2u32.to_le_bytes());
    d.extend_from_slice(&1u32.to_le_bytes()); // entity_a
    d.extend_from_slice(&2u32.to_le_bytes()); // entity_b
    d.extend_from_slice(&3u32.to_le_bytes()); // priority
                                              // wrapped type discriminator
    d.extend_from_slice(&wrapped_type.to_le_bytes());
    // Inner bhkConstraintCInfo (same 16-byte shape, ignored by parser).
    d.extend_from_slice(&[0u8; 16]);
    d
}

fn trailer(threshold: f32, remove_when_broken: bool) -> Vec<u8> {
    let mut d = Vec::with_capacity(5);
    d.extend_from_slice(&threshold.to_le_bytes());
    d.push(remove_when_broken as u8);
    d
}

/// Oblivion baseline — pre-fix this path already worked. Locks
/// the existing behaviour so the version-aware refactor doesn't
/// regress the Oblivion table.
#[test]
fn oblivion_stiff_spring_reads_trailer_fields() {
    let mut bytes = shared_prefix(8); // wrapped_type = 8 (StiffSpring)
    bytes.extend(vec![0xAA; 36]); // 36 bytes wrapped payload
    bytes.extend(trailer(42.5, true));

    let header = oblivion_header();
    let mut stream = NifStream::new(&bytes, &header);
    let block = BhkBreakableConstraint::parse(&mut stream).unwrap();
    assert_eq!(block.wrapped_type, 8);
    assert_eq!(block.threshold, 42.5);
    assert!(block.remove_when_broken);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// FNV StiffSpring — pre-#633 the `is_oblivion` gate skipped the
/// trailer read entirely, so threshold/remove_when_broken came
/// back as defaults regardless of disk content. Post-fix the
/// version-aware table accepts the 36 B StiffSpring payload on
/// FNV (no version difference) and reads the trailer.
#[test]
fn fnv_stiff_spring_now_reads_trailer_fields() {
    let mut bytes = shared_prefix(8); // wrapped_type = 8
    bytes.extend(vec![0xAA; 36]);
    bytes.extend(trailer(99.0, false));

    let header = fnv_header();
    let mut stream = NifStream::new(&bytes, &header);
    let block = BhkBreakableConstraint::parse(&mut stream).unwrap();
    assert_eq!(block.threshold, 99.0, "FNV trailer must round-trip (#633)");
    assert!(!block.remove_when_broken);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// FNV BallAndSocket — same shape as StiffSpring (size invariant
/// across versions). Confirms the both-versions row in the table.
#[test]
fn fnv_ball_and_socket_reads_trailer_fields() {
    let mut bytes = shared_prefix(0); // wrapped_type = 0
    bytes.extend(vec![0xBB; 32]);
    bytes.extend(trailer(7.5, true));

    let header = fnv_header();
    let mut stream = NifStream::new(&bytes, &header);
    let block = BhkBreakableConstraint::parse(&mut stream).unwrap();
    assert_eq!(block.threshold, 7.5);
    assert!(block.remove_when_broken);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// FNV Hinge — wrapped CInfo is 128 B (vs Oblivion's 80) per
/// nif.xml. This row is the headline FNV-D1-02 finding: the
/// Oblivion-only table would have under-consumed by 48 bytes.
/// Post-fix the FNV path consumes 128 B + 5 B trailer.
#[test]
fn fnv_hinge_uses_128_byte_size_not_oblivion_80() {
    let mut bytes = shared_prefix(1); // wrapped_type = 1 (Hinge)
    bytes.extend(vec![0xCC; 128]); // FNV: 8 × Vec4
    bytes.extend(trailer(123.0, false));

    let header = fnv_header();
    let mut stream = NifStream::new(&bytes, &header);
    let block = BhkBreakableConstraint::parse(&mut stream).unwrap();
    assert_eq!(block.threshold, 123.0);
    assert!(!block.remove_when_broken);
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "FNV hinge must consume 16 + 4 + 16 + 128 + 5 = 169 bytes"
    );
    assert_eq!(bytes.len(), 169);
}

/// FNV LimitedHinge with motor type 0 (NONE) — the most common
/// motor flavour in vanilla content. Wrapped payload is 140 B
/// (8 × Vec4 + 3 × f32) + 1 B motor type + 0 B motor payload.
/// Total block: 16 (prefix base) + 4 (wrapped_type) + 16 (inner
/// CInfo) + 140 + 1 + 5 (trailer) = 182 bytes — matches the
/// "expected 182 bytes, consumed 36" warning observed in the
/// vanilla FNV corpus pre-#633.
#[test]
fn fnv_limited_hinge_with_motor_none_consumes_full_block() {
    let mut bytes = shared_prefix(2); // wrapped_type = 2
    bytes.extend(vec![0xCC; 140]); // 8 × Vec4 + 3 × f32
    bytes.push(0u8); // motor type = NONE
    bytes.extend(trailer(50.0, true));

    assert_eq!(bytes.len(), 16 + 4 + 16 + 140 + 1 + 5);
    let header = fnv_header();
    let mut stream = NifStream::new(&bytes, &header);
    let block = BhkBreakableConstraint::parse(&mut stream).unwrap();
    assert_eq!(block.threshold, 50.0);
    assert!(block.remove_when_broken);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// FNV LimitedHinge with motor type 1 (Position) — 25-byte motor
/// payload after the type byte. Confirms the type-dispatch in
/// `consume_motor`.
#[test]
fn fnv_limited_hinge_with_position_motor_consumes_25_extra_bytes() {
    let mut bytes = shared_prefix(2);
    bytes.extend(vec![0xCC; 140]);
    bytes.push(1u8); // motor type = POSITION
    bytes.extend(vec![0xDD; 25]); // motor payload
    bytes.extend(trailer(75.0, false));

    let header = fnv_header();
    let mut stream = NifStream::new(&bytes, &header);
    let block = BhkBreakableConstraint::parse(&mut stream).unwrap();
    assert_eq!(block.threshold, 75.0);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// FNV Ragdoll with motor type 0 — 152 B prefix.
#[test]
fn fnv_ragdoll_with_motor_none_consumes_full_block() {
    let mut bytes = shared_prefix(7); // wrapped_type = 7
    bytes.extend(vec![0xEE; 152]); // 8 × Vec4 + 6 × f32
    bytes.push(0u8);
    bytes.extend(trailer(11.0, false));

    let header = fnv_header();
    let mut stream = NifStream::new(&bytes, &header);
    let block = BhkBreakableConstraint::parse(&mut stream).unwrap();
    assert_eq!(block.threshold, 11.0);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// FNV Prismatic with motor type 3 (Spring) — 17-byte motor.
#[test]
fn fnv_prismatic_with_spring_motor_consumes_17_extra_bytes() {
    let mut bytes = shared_prefix(6); // wrapped_type = 6
    bytes.extend(vec![0xFF; 140]);
    bytes.push(3u8); // motor type = SPRING
    bytes.extend(vec![0xAB; 17]);
    bytes.extend(trailer(33.0, true));

    let header = fnv_header();
    let mut stream = NifStream::new(&bytes, &header);
    let block = BhkBreakableConstraint::parse(&mut stream).unwrap();
    assert_eq!(block.threshold, 33.0);
    assert!(block.remove_when_broken);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Unknown motor type → hard error so the corpus test catches the
/// gap instead of silently drifting. Locks the error path.
#[test]
fn fnv_unknown_motor_type_errors() {
    let mut bytes = shared_prefix(2);
    bytes.extend(vec![0xCC; 140]);
    bytes.push(99u8); // unknown motor type
    let header = fnv_header();
    let mut stream = NifStream::new(&bytes, &header);
    assert!(BhkBreakableConstraint::parse(&mut stream).is_err());
}

/// Malleable (wrapped_type == 13) wraps another CInfo with its
/// own type dispatch — outside this table on either version. Hits
/// the short-stub fallback with trailer fields zeroed; `block_size`
/// recovery in the outer walker handles the byte skip.
#[test]
fn fnv_malleable_falls_through_to_short_stub() {
    let bytes = shared_prefix(13); // wrapped_type = 13 (Malleable)
    let header = fnv_header();
    let mut stream = NifStream::new(&bytes, &header);
    let block = BhkBreakableConstraint::parse(&mut stream).unwrap();
    assert_eq!(block.wrapped_type, 13);
    assert_eq!(block.threshold, 0.0, "Malleable → trailer defaults");
    assert!(!block.remove_when_broken);
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Oblivion non-Stiff-Spring rows still work — covers the
/// Hinge / LimitedHinge / Ragdoll / Prismatic paths on the
/// Oblivion branch where they have known sizes.
#[test]
fn oblivion_hinge_still_uses_80_byte_size() {
    let mut bytes = shared_prefix(1); // wrapped_type = 1 (Hinge)
    bytes.extend(vec![0xDD; 80]); // Oblivion: 5 × Vec4
    bytes.extend(trailer(1.5, true));

    let header = oblivion_header();
    let mut stream = NifStream::new(&bytes, &header);
    let block = BhkBreakableConstraint::parse(&mut stream).unwrap();
    assert_eq!(block.threshold, 1.5);
    assert!(block.remove_when_broken);
    assert_eq!(stream.position() as usize, bytes.len());
}
