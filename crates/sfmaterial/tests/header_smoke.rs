//! Synthetic header round-trip: smallest valid CDB is 16-byte header
//! + STRT + TYPE (with type_count=0). Confirms the parser at least
//! gets through the front-matter cleanly.

use byroredux_sfmaterial::{ChunkType, ComponentDatabaseFile};

fn make_synthetic_minimum() -> Vec<u8> {
    let mut bytes = Vec::new();
    // BETH header
    bytes.extend_from_slice(&0x48544542u32.to_le_bytes());
    bytes.extend_from_slice(&8u32.to_le_bytes()); // headerSize
    bytes.extend_from_slice(&4u32.to_le_bytes()); // fileVersion
    // chunkCount = 3 (BETH + STRT + TYPE — type_count 0 means no CLAS chunks)
    bytes.extend_from_slice(&3u32.to_le_bytes());

    // STRT chunk (empty body)
    bytes.extend_from_slice(&(ChunkType::Strt as u32).to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes()); // size = 0

    // TYPE chunk (4-byte payload = type_count)
    bytes.extend_from_slice(&(ChunkType::Type as u32).to_le_bytes());
    bytes.extend_from_slice(&4u32.to_le_bytes()); // size
    bytes.extend_from_slice(&0u32.to_le_bytes()); // type_count

    bytes
}

#[test]
fn synthetic_minimum_parses_with_zero_classes_and_zero_instances() {
    let bytes = make_synthetic_minimum();
    let cdb = ComponentDatabaseFile::parse(&bytes).expect("parse synthetic minimum");
    assert!(cdb.classes.is_empty());
    assert!(cdb.instances.is_empty());
}

#[test]
fn peek_magic_recognises_beth() {
    let bytes = make_synthetic_minimum();
    assert!(ComponentDatabaseFile::peek_magic(&bytes));
}

#[test]
fn peek_magic_rejects_garbage() {
    assert!(!ComponentDatabaseFile::peek_magic(b"GARBAGE_HEADER"));
    assert!(!ComponentDatabaseFile::peek_magic(b"BG"));
}

#[test]
fn rejects_bad_magic() {
    let mut bytes = make_synthetic_minimum();
    bytes[0] = b'X';
    let err = ComponentDatabaseFile::parse(&bytes).unwrap_err();
    assert!(matches!(err, byroredux_sfmaterial::Error::BadMagic { .. }));
}

#[test]
fn rejects_be_magic() {
    let mut bytes = Vec::new();
    // BETH bytes reversed
    bytes.extend_from_slice(&0x48544542u32.swap_bytes().to_le_bytes());
    bytes.extend_from_slice(&8u32.to_le_bytes());
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    let err = ComponentDatabaseFile::parse(&bytes).unwrap_err();
    assert!(matches!(
        err,
        byroredux_sfmaterial::Error::BigEndianUnsupported
    ));
}

#[test]
fn rejects_unsupported_version() {
    let mut bytes = make_synthetic_minimum();
    // fileVersion is at offset 8.
    bytes[8..12].copy_from_slice(&999u32.to_le_bytes());
    let err = ComponentDatabaseFile::parse(&bytes).unwrap_err();
    assert!(matches!(
        err,
        byroredux_sfmaterial::Error::UnsupportedVersion { got: 999 }
    ));
}
