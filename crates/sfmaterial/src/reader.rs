use crate::chunk::{Chunk, ChunkType};
use crate::string_table::StringTable;
use crate::types::{BuiltinType, Class, ClassFlags, Field, TypeReference};
use crate::value::{ObjectInstance, Ref, Value};
use crate::{Error, Result};
use std::collections::{BTreeMap, HashMap, VecDeque};

const SIGNATURE_BETH: u32 = 0x48544542;
const HEADER_SIZE: u32 = 8;
const FILE_VERSION: u32 = 4;

/// Top-level CDB document. Mirrors `Gibbed.Starfield.FileFormats.
/// ComponentDatabaseFile` but typed.
#[derive(Debug)]
pub struct ComponentDatabaseFile {
    /// Declared classes in TYPE order. Stored alongside the lookup
    /// map for callers that want positional iteration.
    pub classes: Vec<Class>,
    /// Class lookup by content-addressed `name_offset` (the canonical
    /// type-map key — Gibbed uses it for the `typeMap`).
    pub class_by_name_offset: HashMap<i32, usize>,
    /// All top-level instances in the order they appeared on disk.
    pub instances: Vec<Value>,
    /// Resolved string table (kept around so callers can look up
    /// offsets that may be embedded in `TypeReference` debug output).
    pub strings: StringTable,
}

impl ComponentDatabaseFile {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let mut p = Parser::new(bytes);
        p.parse_header()?;
        let chunks = p.index_chunks()?;

        let mut state = State {
            bytes,
            chunks,
            classes: Vec::new(),
            class_by_name_offset: HashMap::new(),
            class_by_type_id: HashMap::new(),
            strings: StringTable::new(Vec::new()),
        };

        // String table is the first chunk after BETH.
        let strt_bytes = state.consume_chunk(ChunkType::Strt)?;
        state.strings = StringTable::new(strt_bytes.to_vec());

        // TYPE chunk: a single u32 type count, followed by N CLAS chunks.
        let type_chunk = state.consume_chunk(ChunkType::Type)?;
        if type_chunk.len() != 4 {
            return Err(Error::BadTypeChunkSize {
                got: type_chunk.len(),
            });
        }
        let type_count = read_u32_le(type_chunk, 0)?;

        for _ in 0..type_count {
            let class = parse_class(&mut state)?;
            let idx = state.classes.len();
            state.class_by_name_offset.insert(class.name_offset, idx);
            state.class_by_type_id.insert(class.type_id, idx);
            state.classes.push(class);
        }

        // Remaining chunks are object/list/map instances. Each one
        // dispatches by its declared chunk type.
        let mut instances = Vec::new();
        while !state.chunks.is_empty() {
            let kind = state.peek_kind()?;
            let value = match kind {
                ChunkType::Objt | ChunkType::User | ChunkType::Diff | ChunkType::Usrd => {
                    consume_object(&mut state)?
                }
                ChunkType::Mapc => consume_map(&mut state, /* is_diff = */ false)?,
                ChunkType::List => consume_list(&mut state, /* is_diff = */ false)?,
                _ => {
                    return Err(Error::WrongChunkType {
                        wanted: ChunkType::Objt,
                        got: kind,
                    });
                }
            };
            instances.push(value);
        }

        Ok(ComponentDatabaseFile {
            classes: state.classes,
            class_by_name_offset: state.class_by_name_offset,
            instances,
            strings: state.strings,
        })
    }

    /// Quick header probe — succeeds when the first 16 bytes look like
    /// a CDB. Useful as a magic-byte check before invoking the full
    /// parser (mirrors `BgsmFile::peek_magic` over in the bgsm crate).
    pub fn peek_magic(bytes: &[u8]) -> bool {
        if bytes.len() < 4 {
            return false;
        }
        let mag = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        mag == SIGNATURE_BETH
    }
}

// ── parser internals ─────────────────────────────────────────────────

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn parse_header(&mut self) -> Result<()> {
        let magic = self.read_u32()?;
        if magic == SIGNATURE_BETH.swap_bytes() {
            // Gibbed's reference accepts both endiannesses defensively,
            // but vanilla Starfield (and all known content) is little-
            // endian. Reject BE rather than silently mis-decode.
            return Err(Error::BigEndianUnsupported);
        }
        if magic != SIGNATURE_BETH {
            return Err(Error::BadMagic {
                got: magic,
                expected: SIGNATURE_BETH,
            });
        }
        let header_size = self.read_u32()?;
        if header_size != HEADER_SIZE {
            return Err(Error::BadHeaderSize { got: header_size });
        }
        let file_version = self.read_u32()?;
        if file_version != FILE_VERSION {
            return Err(Error::UnsupportedVersion { got: file_version });
        }
        Ok(())
    }

    fn index_chunks(&mut self) -> Result<VecDeque<Chunk>> {
        let chunk_count_incl_beth = self.read_u32()?;
        if chunk_count_incl_beth < 1 {
            return Err(Error::EmptyChunkList);
        }
        let chunk_count = (chunk_count_incl_beth - 1) as usize;

        let mut chunks = VecDeque::with_capacity(chunk_count);
        for index in 0..chunk_count {
            let raw = self.read_u32()?;
            let kind = ChunkType::from_raw(raw, index)?;
            let size = self.read_u32()?;
            let start = self.pos;
            let remaining = self.bytes.len().saturating_sub(start);
            if (size as usize) > remaining {
                return Err(Error::ChunkOverflow {
                    index,
                    chunk_type: kind,
                    size,
                    remaining,
                });
            }
            self.pos += size as usize;
            chunks.push_back(Chunk {
                kind,
                start,
                size: size as usize,
            });
        }
        Ok(chunks)
    }

    fn read_u32(&mut self) -> Result<u32> {
        let v = read_u32_le(self.bytes, self.pos)?;
        self.pos += 4;
        Ok(v)
    }
}

struct State<'a> {
    bytes: &'a [u8],
    chunks: VecDeque<Chunk>,
    classes: Vec<Class>,
    class_by_name_offset: HashMap<i32, usize>,
    class_by_type_id: HashMap<u32, usize>,
    strings: StringTable,
}

impl<'a> State<'a> {
    fn peek_kind(&self) -> Result<ChunkType> {
        self.chunks
            .front()
            .map(|c| c.kind)
            .ok_or(Error::ChunkQueueEmpty {
                context: "peek_kind",
            })
    }

    fn consume_chunk(&mut self, wanted: ChunkType) -> Result<&'a [u8]> {
        let chunk = self
            .chunks
            .pop_front()
            .ok_or(Error::ChunkQueueEmpty { context: "consume" })?;
        if chunk.kind != wanted {
            return Err(Error::WrongChunkType {
                wanted,
                got: chunk.kind,
            });
        }
        Ok(&self.bytes[chunk.start..chunk.start + chunk.size])
    }

    fn class_for(&self, type_ref: TypeReference) -> Result<&Class> {
        if type_ref.is_builtin() {
            return Err(Error::UnknownTypeRef { id: type_ref.id });
        }
        // Gibbed indexes `typeMap` by `nameOffset` (which is what's
        // serialized in `TypeReference.id` for declared classes —
        // confirmed by reading `Class.NameOffset` and using it as the
        // map key).
        self.class_by_name_offset
            .get(&type_ref.id)
            .and_then(|&idx| self.classes.get(idx))
            .ok_or(Error::UnknownTypeRef { id: type_ref.id })
    }

    fn is_chunk_type(&self, type_ref: TypeReference) -> bool {
        if type_ref.is_builtin() {
            BuiltinType::from_u32(type_ref.id as u32)
                .map(|b| b.is_chunk())
                .unwrap_or(false)
        } else {
            // User-flagged classes are spilled to OBJT side-chunks.
            self.class_by_name_offset
                .get(&type_ref.id)
                .and_then(|&idx| self.classes.get(idx))
                .map(|c| c.flags.is_user())
                .unwrap_or(false)
        }
    }
}

fn parse_class(state: &mut State) -> Result<Class> {
    let payload = state.consume_chunk(ChunkType::Clas)?;
    let mut cur = Cursor::new(payload);

    let name_offset = cur.read_i32()?;
    let name = state.strings.get(name_offset)?;
    let type_id = cur.read_u32()?;
    let flags_raw = cur.read_u16()?;
    let field_count = cur.read_u16()? as usize;

    let unknown = flags_raw & !ClassFlags::KNOWN;
    if unknown != 0 {
        return Err(Error::UnknownClassFlags { raw: flags_raw });
    }

    let mut fields = Vec::with_capacity(field_count);
    for _ in 0..field_count {
        let name_off = cur.read_i32()?;
        let name = state.strings.get(name_off)?;
        let type_ref = TypeReference::new(cur.read_i32()?);
        let offset = cur.read_u16()?;
        let size = cur.read_u16()?;
        fields.push(Field {
            name,
            type_ref,
            offset,
            size,
        });
    }

    let leftover = payload.len() - cur.pos;
    if leftover != 0 {
        return Err(Error::ClassTrailingBytes { leftover });
    }

    Ok(Class {
        name_offset,
        name,
        type_id,
        flags: ClassFlags(flags_raw),
        fields,
    })
}

fn consume_object(state: &mut State) -> Result<Value> {
    let kind = state.peek_kind()?;
    let (is_cast, is_diff) = match kind {
        ChunkType::Objt => (false, false),
        ChunkType::User => (true, false),
        ChunkType::Diff => (false, true),
        ChunkType::Usrd => (true, true),
        _ => {
            return Err(Error::WrongChunkType {
                wanted: ChunkType::Objt,
                got: kind,
            })
        }
    };
    let payload = state.consume_chunk(kind)?;
    let mut cur = Cursor::new(payload);

    // Cast objects (`USER` / `USRD`) prepend a target-type id before the
    // actual type id; the target is unused by the decoder but consumes
    // bytes so the cursor offsets line up.
    let _target_ref = if is_cast {
        Some(TypeReference::new(cur.read_i32()?))
    } else {
        None
    };
    let type_ref = TypeReference::new(cur.read_i32()?);

    let value = read_value(state, type_ref, &mut cur, is_diff)?;

    if is_cast {
        // Trailing u32 on USER/USRD — purpose undocumented in Gibbed;
        // consume it so the trailing-bytes assertion below holds.
        let _unknown = cur.read_u32()?;
    }

    let leftover = payload.len() - cur.pos;
    if leftover != 0 {
        return Err(Error::ObjectTrailingBytes { leftover });
    }
    Ok(value)
}

fn consume_list(state: &mut State, is_diff: bool) -> Result<Value> {
    let payload = state.consume_chunk(ChunkType::List)?;
    let mut cur = Cursor::new(payload);
    let elem_ref = TypeReference::new(cur.read_i32()?);
    let count = cur.read_i32()? as usize;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        items.push(read_value(state, elem_ref, &mut cur, is_diff)?);
    }
    let leftover = payload.len() - cur.pos;
    if leftover != 0 {
        return Err(Error::ObjectTrailingBytes { leftover });
    }
    Ok(Value::List(items))
}

fn consume_map(state: &mut State, is_diff: bool) -> Result<Value> {
    let payload = state.consume_chunk(ChunkType::Mapc)?;
    let mut cur = Cursor::new(payload);
    let key_ref = TypeReference::new(cur.read_i32()?);
    let val_ref = TypeReference::new(cur.read_i32()?);
    let count = cur.read_i32()? as usize;
    let mut pairs = Vec::with_capacity(count);
    for _ in 0..count {
        let k = read_value(state, key_ref, &mut cur, is_diff)?;
        let v = read_value(state, val_ref, &mut cur, is_diff)?;
        pairs.push((k, v));
    }
    let leftover = payload.len() - cur.pos;
    if leftover != 0 {
        return Err(Error::ObjectTrailingBytes { leftover });
    }
    Ok(Value::Map(pairs))
}

fn read_value(
    state: &mut State,
    type_ref: TypeReference,
    cur: &mut Cursor<'_>,
    is_diff: bool,
) -> Result<Value> {
    if type_ref.is_builtin() {
        let bt = type_ref.as_builtin()?;
        return if bt.is_chunk() {
            // Per Gibbed: builtin-chunk reads at the top level happen
            // through the value reader on a field whose type is List/
            // Map. Those fields aren't read inline — they're spilled
            // to side chunks. This branch is reached only via the
            // chunk-spill path (already handled in
            // `read_user_class` below), so hitting it from inline
            // value-read indicates a malformed CDB.
            Err(Error::UnsupportedBuiltin {
                raw: type_ref.id as u32,
            })
        } else {
            read_primitive(state, bt, cur, is_diff)
        };
    }

    read_user_class(state, type_ref, cur, is_diff)
}

fn read_user_class(
    state: &mut State,
    type_ref: TypeReference,
    cur: &mut Cursor<'_>,
    is_diff: bool,
) -> Result<Value> {
    let (class_name, class_type_id, field_layout) = {
        let class = state.class_for(type_ref)?;
        // Clone the field list once so the iterator below doesn't
        // hold a `&Class` while we mutate `state` reading nested
        // objects.
        (class.name.clone(), class.type_id, class.fields.clone())
    };

    let mut fields: BTreeMap<String, Value> = BTreeMap::new();
    let mut chunk_fields: Vec<Field> = Vec::new();

    if !is_diff {
        for field in &field_layout {
            if state.is_chunk_type(field.type_ref) {
                chunk_fields.push(field.clone());
            } else {
                let v = read_value(state, field.type_ref, cur, is_diff)?;
                fields.insert(field.name.clone(), v);
            }
        }
    } else {
        loop {
            let idx = cur.read_u16()?;
            if idx == 0xFFFF {
                break;
            }
            let field = field_layout
                .get(idx as usize)
                .ok_or(Error::DiffFieldOutOfRange {
                    idx,
                    count: field_layout.len(),
                })?
                .clone();
            if state.is_chunk_type(field.type_ref) {
                chunk_fields.push(field);
            } else {
                let v = read_value(state, field.type_ref, cur, is_diff)?;
                fields.insert(field.name.clone(), v);
            }
        }
    }

    for field in chunk_fields {
        let value = if field.type_ref.is_builtin() {
            match field.type_ref.as_builtin()? {
                BuiltinType::List => consume_list(state, is_diff)?,
                BuiltinType::Map => consume_map(state, is_diff)?,
                _ => {
                    return Err(Error::UnsupportedBuiltin {
                        raw: field.type_ref.id as u32,
                    });
                }
            }
        } else {
            // User class — read its body from the next OBJT/USER/DIFF/USRD chunk.
            consume_object(state)?
        };
        fields.insert(field.name, value);
    }

    Ok(Value::Object(ObjectInstance {
        class_name,
        type_id: class_type_id,
        fields,
    }))
}

fn read_primitive(
    state: &mut State,
    bt: BuiltinType,
    cur: &mut Cursor<'_>,
    is_diff: bool,
) -> Result<Value> {
    Ok(match bt {
        BuiltinType::Null => Value::Null,
        BuiltinType::String => Value::String(read_primitive_string(cur)?),
        BuiltinType::List | BuiltinType::Map => {
            // Should be unreachable — caller short-circuits via
            // `is_chunk()` before getting here. Mirror Gibbed which
            // also returns null for these in the primitive path.
            Value::Null
        }
        BuiltinType::Ref => read_primitive_ref(state, cur, is_diff)?,
        BuiltinType::Int8 => Value::I8(cur.read_i8()?),
        BuiltinType::UInt8 => Value::U8(cur.read_u8()?),
        BuiltinType::Int16 => Value::I16(cur.read_i16()?),
        BuiltinType::UInt16 => Value::U16(cur.read_u16()?),
        BuiltinType::Int32 => Value::I32(cur.read_i32()?),
        BuiltinType::UInt32 => Value::U32(cur.read_u32()?),
        BuiltinType::Int64 => Value::I64(cur.read_i64()?),
        BuiltinType::UInt64 => Value::U64(cur.read_u64()?),
        BuiltinType::Bool => Value::Bool(cur.read_u8()? != 0),
        BuiltinType::Float => Value::Float(f32::from_bits(cur.read_u32()?)),
        BuiltinType::Double => Value::Double(f64::from_bits(cur.read_u64()?)),
    })
}

fn read_primitive_ref(state: &mut State, cur: &mut Cursor<'_>, is_diff: bool) -> Result<Value> {
    let type_id = cur.read_i32()?;
    let type_ref = TypeReference::new(type_id);

    if type_ref.is_builtin() {
        let bt = type_ref.as_builtin()?;
        let inner = read_primitive(state, bt, cur, is_diff)?;
        return Ok(Value::Ref(Ref {
            type_ref,
            inner: Box::new(inner),
        }));
    }

    // Class referent. If the class is flagged IsUser, the body lives in
    // the next OBJT-family chunk; otherwise it's an inline struct read.
    let is_user = state
        .class_by_name_offset
        .get(&type_ref.id)
        .and_then(|&idx| state.classes.get(idx))
        .map(|c| c.flags.is_user())
        .unwrap_or(false);

    let inner = if is_user {
        consume_object(state)?
    } else {
        read_user_class(state, type_ref, cur, is_diff)?
    };
    Ok(Value::Ref(Ref {
        type_ref,
        inner: Box::new(inner),
    }))
}

fn read_primitive_string(cur: &mut Cursor<'_>) -> Result<String> {
    let len = cur.read_u16()? as usize;
    let bytes = cur.read_bytes(len)?;
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

// ── tiny byte-slice cursor (avoids `std::io::Cursor`'s Result-only API) ──

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.pos + n > self.bytes.len() {
            return Err(Error::UnexpectedEof {
                offset: self.pos as u64,
                need: n,
                have: self.bytes.len() - self.pos,
            });
        }
        let out = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn read_u8(&mut self) -> Result<u8> {
        let b = self.read_bytes(1)?;
        Ok(b[0])
    }
    fn read_i8(&mut self) -> Result<i8> {
        self.read_u8().map(|v| v as i8)
    }
    fn read_u16(&mut self) -> Result<u16> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn read_i16(&mut self) -> Result<i16> {
        self.read_u16().map(|v| v as i16)
    }
    fn read_u32(&mut self) -> Result<u32> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn read_i32(&mut self) -> Result<i32> {
        self.read_u32().map(|v| v as i32)
    }
    fn read_u64(&mut self) -> Result<u64> {
        let b = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
    fn read_i64(&mut self) -> Result<i64> {
        self.read_u64().map(|v| v as i64)
    }
}

fn read_u32_le(bytes: &[u8], pos: usize) -> Result<u32> {
    if pos + 4 > bytes.len() {
        return Err(Error::UnexpectedEof {
            offset: pos as u64,
            need: 4,
            have: bytes.len().saturating_sub(pos),
        });
    }
    Ok(u32::from_le_bytes([
        bytes[pos],
        bytes[pos + 1],
        bytes[pos + 2],
        bytes[pos + 3],
    ]))
}
