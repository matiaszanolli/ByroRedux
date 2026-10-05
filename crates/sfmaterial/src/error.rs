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

    #[error(
        "unsupported endianness — Starfield CDB is always little-endian; magic byte-swap detected"
    )]
    BigEndianUnsupported,

    #[error("bad header size: got {got}, expected 8")]
    BadHeaderSize { got: u32 },

    #[error("unsupported CDB file version: got {got}, supported 4")]
    UnsupportedVersion { got: u32 },

    #[error("chunkCount = 0 (must include at least BETH chunk)")]
    EmptyChunkList,

    #[error("expected {wanted:?} chunk, got {got:?}")]
    WrongChunkType { wanted: ChunkType, got: ChunkType },

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

    #[error(
        "CLAS chunk #{class_index} ({class_name:?}) has unknown class flags \
         {raw:#06x} (known: IsUser | IsStruct)"
    )]
    UnknownClassFlags {
        raw: u16,
        /// 0-based index of the offending class within the TYPE block —
        /// names *which* class aborted the parse (#1569), instead of just
        /// the raw flag value with no positional anchor.
        class_index: usize,
        /// Editor name of the offending class, resolved from the string
        /// table before the flag check.
        class_name: String,
    },

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

    #[error(
        "class {class_name:?} declares field {field_name:?} more than once — the Gibbed \
         reference (Dictionary.Add) hard-fails on this rather than silently keeping the \
         last value"
    )]
    DuplicateFieldName {
        class_name: String,
        field_name: String,
    },

    #[error(
        "duplicate CLAS name_offset {name_offset}: class #{duplicate_class_index} \
         ({duplicate_class_name:?}) reuses the name_offset already claimed by class \
         #{first_class_index} ({first_class_name:?}) — the Gibbed reference (typeMap.Add) \
         hard-fails on this rather than silently discarding the earlier class"
    )]
    DuplicateClassNameOffset {
        name_offset: i32,
        first_class_index: usize,
        first_class_name: String,
        duplicate_class_index: usize,
        duplicate_class_name: String,
    },

    #[error(
        "chunk #{index} (unrecognized type {raw:#010x}) declares size {size} but only \
         {remaining} bytes remain in stream"
    )]
    ChunkOverflowUnknownType {
        index: usize,
        raw: u32,
        size: u32,
        remaining: usize,
    },

    #[error("object/list/map chunk had {leftover} trailing bytes after read")]
    ObjectTrailingBytes { leftover: usize },

    #[error("{what} count {raw} is negative — corrupt or adversarial CDB data")]
    NegativeCount { what: &'static str, raw: i32 },

    #[error(
        "CDB value nesting exceeds {limit} levels — a self-referential or adversarial \
         class graph (vanilla nesting is shallow)"
    )]
    NestingTooDeep { limit: usize },

    #[error("CDB parse budget exceeded: {requested} instances (limit {limit})")]
    ParseBudgetExceeded { requested: usize, limit: usize },

    #[error("ran out of chunks while reading {context}")]
    ChunkQueueEmpty { context: &'static str },

    // ── #5320 (PAR-D2-2026-10-05-02) — MaterialIndex::build used to
    // degrade silently on all of these; each is now a typed error the
    // consumer logs instead of an `Ok` with an empty or misaligned
    // index.

    #[error("CDB carries no BSComponentDB2::DBFileIndex instance — the object key join cannot be built")]
    MissingDbFileIndex,

    #[error("a second BSComponentDB2::DBFileIndex instance (the row/instance join allows exactly one)")]
    DuplicateDbFileIndex,

    #[error(
        "Components rows ({rows}) do not match the {instances} stream instances after the \
         DBFileIndex — the row/instance join would attribute materials to the wrong objects"
    )]
    RowInstanceMismatch { rows: usize, instances: usize },

    #[error(
        "DBFileIndex instance carried as a {kind:?} chunk — only OBJT/USER are supported; \
         reading a diff payload in offset order would misparse every field"
    )]
    UnsupportedDbFileIndexChunk { kind: ChunkType },

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
