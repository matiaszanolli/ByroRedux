//! Byte-stable synthetic `.spt` regression sample — #998.
//!
//! `tests/parse_real_spt.rs` is env-var gated and `#[ignore]`-marked
//! so CI without a vanilla BSA installed runs zero corpus coverage on
//! the parser. This file plugs that gap: a deterministic generator
//! emits a fixture covering every [`SptTagKind`] dispatch arm, the
//! exact byte sequence is pinned inline so a future refactor that
//! drifts the encoder can't silently land, and the parser is
//! round-tripped over it.
//!
//! Clean-room policy: vanilla `.spt` files are not redistributable, so
//! the in-tree fixture is hand-authored from the recovered tag
//! dictionary (`crates/spt/docs/format-notes.md`, 2026-05-09 entry).
//!
//! Why bytes-pinned via `assert_eq!` and not a hash:
//! - Same regression-catching power: any byte-level change fails the
//!   assertion. A hash would only add a layer of indirection and a
//!   workspace dep (`sha2` / `blake3` aren't in tree).
//! - On a mismatch the diff shows the actual divergent bytes, which is
//!   strictly more debuggable than "hash differed".

use byroredux_spt::{parse_spt, SptValue};

/// Build a synthetic `.spt` parameter stream covering every dispatch
/// arm in `SptTagKind`. Returns the raw bytes, deterministic across
/// platforms (everything is hand-encoded as little-endian).
///
/// Layout:
/// 1. 20-byte magic (`E8 03 00 00 0C 00 00 00 __IdvSpt_02_`)
/// 2. Bare marker (`1002`, 0 payload bytes)
/// 3. U8 (`2002`, 1 payload byte = `0x42`)
/// 4. U32 (`2001`, 4 payload bytes = f32 `1100.0`)
/// 5. Vec3 (`4001`, 12 payload bytes = `(1.0, 2.0, 3.0)`)
/// 6. FixedBytes(52) (`8003`, 52 payload bytes of zeros)
/// 7. FixedBytes(7) (`13013`, 7 payload bytes of `0xAB`)
/// 8. String (`2000`, length-prefix 18 + ASCII `trees/oak/bark.dds`)
/// 9. ArrayBytes{stride=1} (`10002`, count=4 + 4 bytes `0xDE 0xAD 0xBE 0xEF`)
/// 10. ArrayBytes{stride=8} (`10003`, count=2 + 16 bytes)
/// 11. Geometry-tail sentinel (`0x4E25` — out-of-range u32)
fn build_synthetic_spt() -> Vec<u8> {
    let mut bytes = Vec::new();
    // Magic header.
    bytes.extend_from_slice(&[0xE8, 0x03, 0x00, 0x00, 0x0C, 0x00, 0x00, 0x00]);
    bytes.extend_from_slice(b"__IdvSpt_02_");
    // 1002 — Bare marker.
    bytes.extend_from_slice(&1002u32.to_le_bytes());
    // 2002 — U8 = 0x42.
    bytes.extend_from_slice(&2002u32.to_le_bytes());
    bytes.push(0x42);
    // 2001 — U32 bits of f32 1100.0.
    bytes.extend_from_slice(&2001u32.to_le_bytes());
    bytes.extend_from_slice(&1100.0f32.to_le_bytes());
    // 4001 — Vec3 (1.0, 2.0, 3.0).
    bytes.extend_from_slice(&4001u32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&2.0f32.to_le_bytes());
    bytes.extend_from_slice(&3.0f32.to_le_bytes());
    // 8003 — FixedBytes(52) zeros.
    bytes.extend_from_slice(&8003u32.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 52]);
    // 13013 — FixedBytes(7) of 0xAB.
    bytes.extend_from_slice(&13013u32.to_le_bytes());
    bytes.extend_from_slice(&[0xABu8; 7]);
    // 2000 — String (bark texture path).
    bytes.extend_from_slice(&2000u32.to_le_bytes());
    let bark = b"trees/oak/bark.dds";
    bytes.extend_from_slice(&(bark.len() as u32).to_le_bytes());
    bytes.extend_from_slice(bark);
    // 10002 — ArrayBytes{stride=1}, count=4.
    bytes.extend_from_slice(&10002u32.to_le_bytes());
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
    // 10003 — ArrayBytes{stride=8}, count=2 (16 bytes total).
    bytes.extend_from_slice(&10003u32.to_le_bytes());
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&[
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, // first stride-8 entry
        0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, // second stride-8 entry
    ]);
    // Geometry-tail sentinel — out-of-range u32 so the walker bails
    // cleanly without recording it as an unknown tag.
    bytes.extend_from_slice(&0x4E25u32.to_le_bytes());
    bytes
}

/// Pinned byte sequence — derived once from `build_synthetic_spt`
/// and reproduced verbatim here. Any unintended drift in the encoder
/// (struct layout reshuffle, endian flip, new dispatch arm absorbing
/// an existing tag value) will fail this assertion before the
/// parser-side test even runs.
const PINNED_BYTES: &[u8] = &[
    // Magic (20 B).
    0xE8, 0x03, 0x00, 0x00, 0x0C, 0x00, 0x00, 0x00, b'_', b'_', b'I', b'd', b'v', b'S', b'p', b't',
    b'_', b'0', b'2', b'_',
    // Tag 1002 — Bare.
    0xEA, 0x03, 0x00, 0x00,
    // Tag 2002 — U8 0x42.
    0xD2, 0x07, 0x00, 0x00, 0x42,
    // Tag 2001 — U32 (f32 1100.0 = 0x44898000 LE).
    0xD1, 0x07, 0x00, 0x00, 0x00, 0x80, 0x89, 0x44,
    // Tag 4001 — Vec3 (1.0, 2.0, 3.0).
    0xA1, 0x0F, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3F, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x40, 0x40,
    // Tag 8003 — FixedBytes(52) of zeros.
    0x43, 0x1F, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // Tag 13013 — FixedBytes(7) of 0xAB.
    0xD5, 0x32, 0x00, 0x00, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB, 0xAB,
    // Tag 2000 — String "trees/oak/bark.dds" (length 18).
    0xD0, 0x07, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00, b't', b'r', b'e', b'e', b's', b'/', b'o', b'a',
    b'k', b'/', b'b', b'a', b'r', b'k', b'.', b'd', b'd', b's',
    // Tag 10002 — ArrayBytes(stride=1), count=4, payload {DE AD BE EF}.
    0x12, 0x27, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xDE, 0xAD, 0xBE, 0xEF,
    // Tag 10003 — ArrayBytes(stride=8), count=2, two 8-byte entries.
    0x13, 0x27, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18,
    // Geometry-tail sentinel (0x4E25 — out of [TAG_MIN, TAG_MAX]).
    0x25, 0x4E, 0x00, 0x00,
];

#[test]
fn generator_output_matches_pinned_bytes() {
    let generated = build_synthetic_spt();
    assert_eq!(
        generated.len(),
        PINNED_BYTES.len(),
        "fixture length drift — generator wrote {} bytes, pinned spec is {}",
        generated.len(),
        PINNED_BYTES.len(),
    );
    assert_eq!(
        generated, PINNED_BYTES,
        "fixture byte drift — re-pin PINNED_BYTES if this is an intentional encoder change"
    );
}

#[test]
fn parser_decodes_every_dispatch_arm_against_pinned_fixture() {
    let scene = parse_spt(PINNED_BYTES).expect("pinned fixture must parse");
    assert!(
        scene.unknown_tags.is_empty(),
        "no unknown tags in the pinned fixture — got {:?}",
        scene.unknown_tags,
    );
    assert!(!scene.reached_eof, "walker stops at the geometry-tail sentinel, not EOF");
    assert_eq!(scene.entries.len(), 9, "every dispatch arm contributes one entry");

    // Verify each entry's tag + value kind in stream order.
    let (tag, value) = (scene.entries[0].tag, &scene.entries[0].value);
    assert_eq!(tag, 1002);
    assert_eq!(value, &SptValue::Bare);

    let (tag, value) = (scene.entries[1].tag, &scene.entries[1].value);
    assert_eq!(tag, 2002);
    assert_eq!(value, &SptValue::U8(0x42));

    let (tag, value) = (scene.entries[2].tag, &scene.entries[2].value);
    assert_eq!(tag, 2001);
    assert_eq!(value.as_f32(), Some(1100.0));

    let (tag, value) = (scene.entries[3].tag, &scene.entries[3].value);
    assert_eq!(tag, 4001);
    assert_eq!(value, &SptValue::Vec3([1.0, 2.0, 3.0]));

    // FixedBytes(52) → SptValue::Fixed with 52 bytes.
    let (tag, value) = (scene.entries[4].tag, &scene.entries[4].value);
    assert_eq!(tag, 8003);
    match value {
        SptValue::Fixed(bytes) => assert_eq!(bytes.len(), 52),
        _ => panic!("tag 8003 must decode as Fixed(52)"),
    }

    let (tag, value) = (scene.entries[5].tag, &scene.entries[5].value);
    assert_eq!(tag, 13013);
    match value {
        SptValue::Fixed(bytes) => {
            assert_eq!(bytes.len(), 7);
            assert!(bytes.iter().all(|&b| b == 0xAB));
        }
        _ => panic!("tag 13013 must decode as Fixed(7)"),
    }

    let (tag, value) = (scene.entries[6].tag, &scene.entries[6].value);
    assert_eq!(tag, 2000);
    assert_eq!(value.as_str(), Some("trees/oak/bark.dds"));

    let (tag, value) = (scene.entries[7].tag, &scene.entries[7].value);
    assert_eq!(tag, 10002);
    match value {
        SptValue::ArrayBytes { stride, count, bytes } => {
            assert_eq!(*stride, 1);
            assert_eq!(*count, 4);
            assert_eq!(bytes.as_slice(), &[0xDE, 0xAD, 0xBE, 0xEF]);
        }
        _ => panic!("tag 10002 must decode as ArrayBytes(stride=1)"),
    }

    let (tag, value) = (scene.entries[8].tag, &scene.entries[8].value);
    assert_eq!(tag, 10003);
    match value {
        SptValue::ArrayBytes { stride, count, bytes } => {
            assert_eq!(*stride, 8);
            assert_eq!(*count, 2);
            assert_eq!(bytes.len(), 16);
            assert_eq!(&bytes[0..8], &[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        }
        _ => panic!("tag 10003 must decode as ArrayBytes(stride=8)"),
    }
}

#[test]
fn typed_accessors_resolve_against_pinned_fixture() {
    let scene = parse_spt(PINNED_BYTES).expect("pinned fixture must parse");
    assert_eq!(
        scene.bark_textures(),
        vec!["trees/oak/bark.dds"],
        "tag 2000 must flow through `bark_textures()` accessor",
    );
    assert!(scene.leaf_textures().is_empty(), "fixture authors no tag 4003");
    assert!(scene.curves().is_empty(), "fixture authors no tag 6000-6007");
}
