use crate::chunk::ChunkType;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("unexpected end of input at offset {offset} (need {need} bytes, have {have})")]
    UnexpectedEof {
        offset: u64,
        need: usize,
        have: usize,
    },

    #[error("bad magic at offset 0: got {got:#010x}, expected BETH (0x{expected:08X})")]
    BadMagic { got: u32, expected: u32 },

    #[error("unsupported endianness — Starfield CDB is always little-endian; magic byte-swap detected")]
    BigEndianUnsupported,

    #[error("bad header size: got {got}, expected 8")]
    BadHeaderSize { got: u32 },

    #[error("unsupported CDB file version: got {got}, supported 4")]
    UnsupportedVersion { got: u32 },

    #[error("chunkCount = 0 (must include at least BETH chunk)")]
    EmptyChunkList,

    #[error("expected {wanted:?} chunk, got {got:?}")]
    WrongChunkType {
        wanted: ChunkType,
        got: ChunkType,
    },

    #[error("unknown chunk type {raw:#010x} at chunk index {index}")]
    UnknownChunkType { raw: u32, index: usize },

    #[error("chunk #{index} ({chunk_type:?}) declares size {size} but only {remaining} bytes remain in stream")]
    ChunkOverflow {
        index: usize,
        chunk_type: ChunkType,
        size: u32,
        remaining: usize,
    },

    #[error("TYPE chunk must be exactly 4 bytes, got {got}")]
    BadTypeChunkSize { got: usize },

    #[error("CLAS chunk has unknown class flags {raw:#06x} (known: IsUser | IsStruct)")]
    UnknownClassFlags { raw: u16 },

    #[error("class chunk had {leftover} trailing bytes after fields")]
    ClassTrailingBytes { leftover: usize },

    #[error("string table offset {offset} out of bounds (table size {len})")]
    StringTableOob { offset: i32, len: usize },

    #[error("unknown TypeReference id {id} (negative ids must map to BuiltinType, positive must index TYPE chunk)")]
    UnknownTypeRef { id: i32 },

    #[error("unsupported BuiltinType {raw:#010x} at value read")]
    UnsupportedBuiltin { raw: u32 },

    #[error("DIFF chunk requested a field index {idx} but the class has only {count} fields")]
    DiffFieldOutOfRange { idx: u16, count: usize },

    #[error("object/list/map chunk had {leftover} trailing bytes after read")]
    ObjectTrailingBytes { leftover: usize },

    #[error("ran out of chunks while reading {context}")]
    ChunkQueueEmpty { context: &'static str },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
