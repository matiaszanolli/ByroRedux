use crate::Error;

/// CDB chunk type — 4-character FourCC stored as `u32` little-endian.
/// The repr matches the on-disk bytes so `from_le_bytes` round-trips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ChunkType {
    /// `STRT` — string table; the only blob whose contents are
    /// addressed by offset. Always the first chunk after BETH.
    Strt = 0x54525453,
    /// `TYPE` — a single u32 type count.
    Type = 0x45505954,
    /// `CLAS` — one class declaration (name, type id, flags, fields).
    Clas = 0x53414C43,
    /// `OBJT` — top-level object payload (declared type).
    Objt = 0x544A424F,
    /// `DIFF` — object payload as a diff against the class default
    /// (field-by-index, terminated by `0xFFFF`).
    Diff = 0x46464944,
    /// `USER` — user object with explicit `(targetType, sourceType)`
    /// cast header.
    User = 0x52455355,
    /// `USRD` — user object diff (cast + DIFF semantics).
    Usrd = 0x44525355,
    /// `MAPC` — map (typed key + typed value pairs).
    Mapc = 0x4350414D,
    /// `LIST` — homogeneous list with declared element type.
    List = 0x5453494C,
}

impl ChunkType {
    pub(crate) fn from_raw(raw: u32, index: usize) -> crate::Result<Self> {
        Ok(match raw {
            0x54525453 => ChunkType::Strt,
            0x45505954 => ChunkType::Type,
            0x53414C43 => ChunkType::Clas,
            0x544A424F => ChunkType::Objt,
            0x46464944 => ChunkType::Diff,
            0x52455355 => ChunkType::User,
            0x44525355 => ChunkType::Usrd,
            0x4350414D => ChunkType::Mapc,
            0x5453494C => ChunkType::List,
            _ => return Err(Error::UnknownChunkType { raw, index }),
        })
    }
}

/// One queued chunk — the file-pass-1 pre-index just records the
/// type / position / size so pass-2 can dispatch in queue order
/// without re-walking.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Chunk {
    pub kind: ChunkType,
    pub start: usize,
    pub size: usize,
}
