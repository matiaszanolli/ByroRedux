use crate::chunk::{Chunk, ChunkType};
use crate::string_table::StringTable;
use crate::types::{BuiltinType, Class, ClassFlags, Field, TypeReference};
use crate::value::{ObjectInstance, Ref, Value};
use crate::{Error, Result};
use std::collections::{BTreeMap, HashMap, VecDeque};

const SIGNATURE_BETH: u32 = 0x48544542;
const HEADER_SIZE: u32 = 8;
const FILE_VERSION: u32 = 4;

/// #4657 — ceiling on struct / reference nesting while reading or skipping a
/// value. The value readers recurse through `read_user_class` and
/// `read_primitive_ref` (and their `skip_*` twins) with no other bound: a
/// `CLAS` whose inline field has its own class as type recurses without
/// consuming a byte, and an 84-byte file overflowed the stack and aborted
/// the process. [`ParseLimits::max_instances`] counts top-level chunks only
/// and cannot catch it. Vanilla nesting is shallow — the full vanilla
/// `materialsbeta.cdb` validates under this cap — so 64 is ample.
const MAX_NESTING_DEPTH: usize = 64;

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
        Self::parse_with_limits(bytes, ParseLimits::unlimited())
    }

    /// Parse with a caller-supplied ceiling on top-level object instances.
    /// Vanilla Starfield contains ~1.44M instances and expands dramatically
    /// when materialised as a generic `Value` tree; callers loading untrusted
    /// or memory-constrained content should choose a finite limit. The
    /// production presence path uses [`Self::probe_header`] and never needs
    /// this tree at all. #3055.
    pub fn parse_with_limits(bytes: &[u8], limits: ParseLimits) -> Result<Self> {
        let mut state = parse_schema(bytes, limits)?;

        // Remaining chunks are object/list/map instances. Each one
        // dispatches by its declared chunk type.
        let mut instances = Vec::new();
        while !state.chunks.is_empty() {
            let value = consume_top_level_value(&mut state)?;
            instances.push(value);
        }

        Ok(ComponentDatabaseFile {
            classes: state.classes,
            class_by_name_offset: state.class_by_name_offset,
            instances,
            strings: state.strings,
        })
    }

    /// Visit top-level CDB values in on-disk order without retaining values
    /// from earlier top-level chunks. This is useful when each top-level
    /// chunk is independently sized. For a fully bounded corpus walk use
    /// [`Self::validate_instances_with_limits`]: a single `LIST` or `MAPC`
    /// can itself contain a very large value tree. Intended for Starfield CDB
    /// Phase 2's selective material indexing.
    pub fn visit_instances_with_limits(
        bytes: &[u8],
        limits: ParseLimits,
        mut visitor: impl FnMut(&Value),
    ) -> Result<CdbVisitInfo> {
        let mut state = parse_schema(bytes, limits)?;
        let class_count = state.classes.len();
        let mut value_count = 0usize;
        while !state.chunks.is_empty() {
            let value = consume_top_level_value(&mut state)?;
            visitor(&value);
            value_count += 1;
        }
        Ok(CdbVisitInfo {
            class_count,
            value_count,
        })
    }

    /// Decode and validate every instance without materialising any dynamic
    /// [`Value`] tree. The schema and string table remain resident, while
    /// objects, lists, maps, and strings are consumed directly from the CDB
    /// byte stream. This is suitable for vanilla `materialsbeta.cdb`, whose
    /// full generic value tree has measured at ~9.19 GiB RSS (#4274).
    pub fn validate_instances_with_limits(
        bytes: &[u8],
        limits: ParseLimits,
    ) -> Result<CdbVisitInfo> {
        let mut state = parse_schema(bytes, limits)?;
        let class_count = state.classes.len();
        let mut value_count = 0usize;
        while !state.chunks.is_empty() {
            skip_top_level_value(&mut state)?;
            value_count += 1;
        }
        Ok(CdbVisitInfo {
            class_count,
            value_count,
        })
    }

    /// Quick magic-byte probe — succeeds when the first 4 bytes are the
    /// `BETH` signature. The cheapest possible reject for a mis-named /
    /// non-CDB file before any header or chunk-table work (mirrors
    /// `BgsmFile::peek_magic` over in the bgsm crate). Wired into the
    /// material provider's discovery path. SF-D3-AUDIT-03 / #2102.
    pub fn peek_magic(bytes: &[u8]) -> bool {
        if bytes.len() < 4 {
            return false;
        }
        let mag = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        mag == SIGNATURE_BETH
    }

    /// Lightweight validity/presence probe: validate the 16-byte header
    /// and index the chunk table WITHOUT walking the (vanilla: ~1.44M
    /// entry) instance tree. Returns a [`CdbHeaderInfo`] summary.
    ///
    /// Callers that only need to confirm a well-formed CDB is present —
    /// e.g. the material provider's `has_starfield_cdb` gate — should use
    /// this instead of [`ComponentDatabaseFile::parse`], which
    /// materialises the full typed tree (a multi-second, multi-hundred-MB
    /// operation whose result Phase-1 presence checks never read). When
    /// Phase 2's per-field material index is built it re-runs the full
    /// `parse` on demand. SF-D3-AUDIT-01 / #2100.
    pub fn probe_header(bytes: &[u8]) -> Result<CdbHeaderInfo> {
        let mut p = Parser::new(bytes);
        p.parse_header()?;
        // #4273 (SF-D3-2026-09-11-02) — tolerant walk, not `index_chunks`.
        // The probe only needs the chunk count/presence, never chunk
        // semantics, so it must not abort on a chunk-type FourCC outside
        // the current 10-entry `ChunkType` vocabulary the way the real
        // object-tree parse correctly does (that path DOES need every
        // chunk typed to dispatch on it).
        let chunk_count = p.count_chunks_tolerant()?;
        Ok(CdbHeaderInfo { chunk_count })
    }
}

/// Resource ceiling for the owned instance tree produced by the full parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseLimits {
    pub max_instances: usize,
}

impl ParseLimits {
    pub const fn unlimited() -> Self {
        Self {
            max_instances: usize::MAX,
        }
    }
}

/// Header-only summary produced by [`ComponentDatabaseFile::probe_header`]
/// — the cheap presence-check counterpart to a full
/// [`ComponentDatabaseFile::parse`] that never touches the instance tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdbHeaderInfo {
    /// Number of chunks declared in the index (excludes the BETH marker).
    pub chunk_count: usize,
}

/// Summary returned by [`ComponentDatabaseFile::visit_instances_with_limits`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CdbVisitInfo {
    /// Number of class schemas decoded before instance visitation.
    pub class_count: usize,
    /// Number of top-level values delivered to the visitor.
    pub value_count: usize,
}

pub(crate) fn parse_schema(bytes: &[u8], limits: ParseLimits) -> Result<State<'_>> {
    let mut p = Parser::new(bytes);
    p.parse_header()?;
    let chunks = p.index_chunks()?;
    let object_chunks = chunks
        .iter()
        .filter(|chunk| {
            matches!(
                chunk.kind,
                ChunkType::Objt | ChunkType::User | ChunkType::Diff | ChunkType::Usrd
            )
        })
        .count();
    if object_chunks > limits.max_instances {
        return Err(Error::ParseBudgetExceeded {
            requested: object_chunks,
            limit: limits.max_instances,
        });
    }

    let mut state = State {
        bytes,
        chunks,
        classes: Vec::new(),
        class_by_name_offset: HashMap::new(),
        strings: StringTable::new(Vec::new()),
        depth: 0,
    };
    let strt_bytes = state.consume_chunk(ChunkType::Strt)?;
    state.strings = StringTable::new(strt_bytes.to_vec());
    let type_chunk = state.consume_chunk(ChunkType::Type)?;
    if type_chunk.len() != 4 {
        return Err(Error::BadTypeChunkSize {
            got: type_chunk.len(),
        });
    }
    let type_count = read_u32_le(type_chunk, 0)?;
    for class_index in 0..type_count as usize {
        let class = parse_class(&mut state, class_index)?;
        let idx = state.classes.len();
        insert_class_name_offset(&mut state.class_by_name_offset, &state.classes, &class, idx)?;
        state.classes.push(class);
    }
    Ok(state)
}

pub(crate) fn consume_top_level_value(state: &mut State<'_>) -> Result<Value> {
    let kind = state.peek_kind()?;
    match kind {
        ChunkType::Objt | ChunkType::User | ChunkType::Diff | ChunkType::Usrd => {
            consume_object(state)
        }
        ChunkType::Mapc => consume_map(state, /* is_diff = */ false),
        ChunkType::List => consume_list(state, /* is_diff = */ false),
        _ => Err(Error::WrongChunkType {
            wanted: ChunkType::Objt,
            got: kind,
        }),
    }
}

/// Consume the same CDB structure as `consume_top_level_value` but retain no
/// dynamic values. Kept separate from the materialising reader so its memory
/// bound is evident at the call site and future selective visitors can build
/// only the objects they need.
pub(crate) fn skip_top_level_value(state: &mut State<'_>) -> Result<()> {
    match state.peek_kind()? {
        ChunkType::Objt | ChunkType::User | ChunkType::Diff | ChunkType::Usrd => skip_object(state),
        ChunkType::Mapc => skip_map(state, false),
        ChunkType::List => skip_list(state, false),
        kind => Err(Error::WrongChunkType {
            wanted: ChunkType::Objt,
            got: kind,
        }),
    }
}

/// #3398 Phase 2 — true while the instance stream still has chunks to
/// consume. `State`'s fields are module-private, so the index builder in
/// `index.rs` drives its own loop through this.
pub(crate) fn state_has_more_chunks(state: &State<'_>) -> bool {
    !state.chunks.is_empty()
}

/// #3398 Phase 2 — class name of the NEXT top-level instance chunk,
/// read without consuming anything. `Ok(None)` when the front of the
/// queue is a `LIST`/`MAPC` (either a stray or a side chunk owned by a
/// preceding object's collection fields — the caller's walk decides).
pub(crate) fn peek_top_level_class_name(state: &State<'_>) -> Result<Option<String>> {
    use crate::reader::read_u32_le;
    let Some(chunk) = state.chunks.front() else {
        return Ok(None);
    };
    if !matches!(
        chunk.kind,
        ChunkType::Objt | ChunkType::User | ChunkType::Diff | ChunkType::Usrd
    ) {
        return Ok(None);
    }
    let mut off = chunk.start;
    if matches!(chunk.kind, ChunkType::User | ChunkType::Usrd) {
        off += 4;
    }
    let raw = read_u32_le(state.bytes, off)? as i32;
    let type_ref = TypeReference::new(raw);
    if type_ref.is_builtin() {
        return Ok(None);
    }
    let class = state.class_for(type_ref)?;
    Ok(Some(class.name.clone()))
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

        // #2614 / SF-D3-01 — `chunk_count` is an unvalidated on-disk u32
        // (up to ~4 billion for a corrupt/truncated file); pre-reserving
        // directly from it let `VecDeque::with_capacity` request ~103 GB
        // and panic/abort *before* the per-chunk `ChunkOverflow` guard
        // below ever runs, on the live cell-load path. Each chunk costs
        // at least 8 bytes on disk (the `kind`/`size` header read every
        // iteration), so cap the reservation at that lower bound — a
        // hostile count still gets validated chunk-by-chunk into a
        // proper `Err`, it just doesn't over-allocate first. Matches
        // Gibbed's reference (`ComponentDatabaseFile.cs`), which has no
        // pre-reserve at all.
        let mut chunks = VecDeque::with_capacity(chunk_count.min(self.bytes.len() / 8));
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

    /// Tolerant counterpart to [`Self::index_chunks`] for
    /// [`ComponentDatabaseFile::probe_header`] (#4273 / SF-D3-2026-09-11-02):
    /// walks the chunk table counting entries and validating each declared
    /// `size` against the remaining stream, exactly like `index_chunks`,
    /// but does NOT require every chunk's FourCC to resolve via
    /// [`ChunkType::from_raw`] — the probe never dispatches on chunk type,
    /// only counts chunks, so a FourCC outside the current 10-entry
    /// vocabulary (a future format revision, a mod-authored or corrupted
    /// CDB) must not abort it. The buffer-overflow check still runs
    /// unconditionally: an oversized chunk means the stream itself is
    /// unsafe to walk further, which is orthogonal to whether its type is
    /// recognized.
    fn count_chunks_tolerant(&mut self) -> Result<usize> {
        let chunk_count_incl_beth = self.read_u32()?;
        if chunk_count_incl_beth < 1 {
            return Err(Error::EmptyChunkList);
        }
        let chunk_count = (chunk_count_incl_beth - 1) as usize;
        for index in 0..chunk_count {
            let raw = self.read_u32()?;
            let size = self.read_u32()?;
            let remaining = self.bytes.len().saturating_sub(self.pos);
            if (size as usize) > remaining {
                return Err(match ChunkType::from_raw(raw, index) {
                    Ok(kind) => Error::ChunkOverflow {
                        index,
                        chunk_type: kind,
                        size,
                        remaining,
                    },
                    Err(_) => Error::ChunkOverflowUnknownType {
                        index,
                        raw,
                        size,
                        remaining,
                    },
                });
            }
            self.pos += size as usize;
        }
        Ok(chunk_count)
    }

    fn read_u32(&mut self) -> Result<u32> {
        let v = read_u32_le(self.bytes, self.pos)?;
        self.pos += 4;
        Ok(v)
    }
}

pub(crate) struct State<'a> {
    bytes: &'a [u8],
    chunks: VecDeque<Chunk>,
    classes: Vec<Class>,
    class_by_name_offset: HashMap<i32, usize>,
    strings: StringTable,
    /// Current value-nesting depth; see [`MAX_NESTING_DEPTH`].
    depth: usize,
}

/// Run one level of value nesting under the [`MAX_NESTING_DEPTH`] cap.
fn nested<'a, T>(
    state: &mut State<'a>,
    body: impl FnOnce(&mut State<'a>) -> Result<T>,
) -> Result<T> {
    if state.depth >= MAX_NESTING_DEPTH {
        return Err(Error::NestingTooDeep {
            limit: MAX_NESTING_DEPTH,
        });
    }
    state.depth += 1;
    let result = body(state);
    state.depth -= 1;
    result
}

impl<'a> State<'a> {
    pub(crate) fn peek_kind(&self) -> Result<ChunkType> {
        self.chunks
            .front()
            .map(|c| c.kind)
            .ok_or(Error::ChunkQueueEmpty {
                context: "peek_kind",
            })
    }

    pub(crate) fn consume_chunk(&mut self, wanted: ChunkType) -> Result<&'a [u8]> {
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

    pub(crate) fn class_for(&self, type_ref: TypeReference) -> Result<&Class> {
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

    pub(crate) fn is_chunk_type(&self, type_ref: TypeReference) -> bool {
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

fn parse_class(state: &mut State, class_index: usize) -> Result<Class> {
    let payload = state.consume_chunk(ChunkType::Clas)?;
    let mut cur = Cursor::new(payload);

    let name_offset = cur.read_i32()?;
    let name = state.strings.get(name_offset)?;
    let type_id = cur.read_u32()?;
    let flags_raw = cur.read_u16()?;
    let field_count = cur.read_u16()? as usize;

    let unknown = flags_raw & !ClassFlags::KNOWN;
    if unknown != 0 {
        // #1569 — name the offending class (index + editor name) instead
        // of just the raw flag value, so a future patch/DLC CDB that adds
        // a reflection class-flag bit is diagnosable from the single warn
        // line rather than leaving the operator guessing which of 1.44M
        // materials' classes aborted the whole load.
        return Err(Error::UnknownClassFlags {
            raw: flags_raw,
            class_index,
            class_name: name,
        });
    }

    // #2614 / SF-D3-01 sibling — `field_count` is an unvalidated on-disk
    // u16 read from `payload`; each field costs at least 12 bytes
    // (name_off + type_ref + offset + size), so cap the reservation at
    // that lower bound rather than trusting the count directly. See the
    // `index_chunks` doc for the full rationale — same class of bug.
    let mut fields = Vec::with_capacity(field_count.min(payload.len() / 12));
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

    // #4275 (SF-D3-2026-09-11-04) → fixed by #3398 — inline field bytes
    // are laid out by `Field::offset`, and declaration order disagrees
    // with offset order on real classes (`XMCOLOR` declares `r,g,b,a` at
    // offsets `2,1,0,3`). `read_order` below is the offset-sorted
    // iteration every sequential reader must use; `read_user_class_body`
    // walks it, so a divergence between the two orders can no longer
    // bind a value to the wrong field name. Diff chunks index fields by
    // declaration slot, so `fields` stays in declaration order.
    let read_order = offset_read_order(&fields);

    Ok(Class {
        name_offset,
        name,
        type_id,
        flags: ClassFlags(flags_raw),
        fields,
        read_order,
    })
}

/// #3398 — indices into `fields` sorted by ascending wire `offset`. This
/// is the iteration order for any reader that consumes a class's inline
/// bytes sequentially (see [`Class::read_order`]).
fn offset_read_order(fields: &[Field]) -> Vec<u32> {
    let mut order: Vec<u32> = (0..fields.len() as u32).collect();
    order.sort_by_key(|&i| fields[i as usize].offset);
    order
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
            });
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
    let raw_count = cur.read_i32()?;
    // #2623 / SF-D3-02 — `raw_count` is unvalidated on-disk data. Pre-fix
    // this sign-extended a negative value through `as usize` into
    // ~1.8e19; #2614 already dropped the `with_capacity` panic that used
    // to follow, but the raw (possibly astronomical) count still drove
    // `for _ in 0..count` directly. That's not purely theoretical even
    // post-#2614: a user-class element type with no non-chunk fields is a
    // structurally valid class shape whose `read_value` returns `Ok`
    // while consuming ZERO bytes from `cur` — so a huge count paired with
    // that element type wouldn't error out on the first iteration, it
    // would loop until the process exhausts memory accumulating `items`.
    // Reject a negative count outright (a clean, attributable error beats
    // a confusing downstream one), then clamp to `payload.len()` — no
    // chunk can contain more elements than it has bytes for the common
    // (non-zero-byte-element) case, matching the issue's own suggested
    // fix. Caveat, left as a documented edge case rather than solved
    // here: a list whose element type is a genuinely zero-field class
    // could in principle legitimately encode more elements than
    // `payload.len()`; this clamp would truncate that (extremely
    // unusual, unobserved in real content) shape rather than reject it
    // outright. Closing that fully needs a stuck-cursor progress check
    // instead of a count clamp, which is a larger change than this fix.
    let count = usize::try_from(raw_count)
        .map_err(|_| Error::NegativeCount {
            what: "LIST element",
            raw: raw_count,
        })?
        .min(payload.len());
    let mut items = Vec::new();
    for _ in 0..count {
        items.push(read_value(state, elem_ref, &mut cur, is_diff)?);
    }
    let leftover = payload.len() - cur.pos;
    if leftover != 0 {
        return Err(Error::ObjectTrailingBytes { leftover });
    }
    Ok(Value::List(items))
}

pub(crate) fn consume_map(state: &mut State, is_diff: bool) -> Result<Value> {
    let payload = state.consume_chunk(ChunkType::Mapc)?;
    let mut cur = Cursor::new(payload);
    let key_ref = TypeReference::new(cur.read_i32()?);
    let val_ref = TypeReference::new(cur.read_i32()?);
    let raw_count = cur.read_i32()?;
    // #2623 / SF-D3-02 — same rationale and caveat as `consume_list`.
    let count = usize::try_from(raw_count)
        .map_err(|_| Error::NegativeCount {
            what: "MAPC pair",
            raw: raw_count,
        })?
        .min(payload.len());
    let mut pairs = Vec::new();
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

fn skip_object(state: &mut State) -> Result<()> {
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
            });
        }
    };
    let payload = state.consume_chunk(kind)?;
    let mut cur = Cursor::new(payload);
    if is_cast {
        let _target_ref = TypeReference::new(cur.read_i32()?);
    }
    let type_ref = TypeReference::new(cur.read_i32()?);
    skip_value(state, type_ref, &mut cur, is_diff)?;
    if is_cast {
        let _unknown = cur.read_u32()?;
    }
    let leftover = payload.len() - cur.pos;
    if leftover != 0 {
        return Err(Error::ObjectTrailingBytes { leftover });
    }
    Ok(())
}

fn skip_list(state: &mut State, is_diff: bool) -> Result<()> {
    let payload = state.consume_chunk(ChunkType::List)?;
    let mut cur = Cursor::new(payload);
    let elem_ref = TypeReference::new(cur.read_i32()?);
    let count = checked_container_count(cur.read_i32()?, payload.len(), "LIST element")?;
    for _ in 0..count {
        skip_value(state, elem_ref, &mut cur, is_diff)?;
    }
    let leftover = payload.len() - cur.pos;
    if leftover != 0 {
        return Err(Error::ObjectTrailingBytes { leftover });
    }
    Ok(())
}

fn skip_map(state: &mut State, is_diff: bool) -> Result<()> {
    let payload = state.consume_chunk(ChunkType::Mapc)?;
    let mut cur = Cursor::new(payload);
    let key_ref = TypeReference::new(cur.read_i32()?);
    let val_ref = TypeReference::new(cur.read_i32()?);
    let count = checked_container_count(cur.read_i32()?, payload.len(), "MAPC pair")?;
    for _ in 0..count {
        skip_value(state, key_ref, &mut cur, is_diff)?;
        skip_value(state, val_ref, &mut cur, is_diff)?;
    }
    let leftover = payload.len() - cur.pos;
    if leftover != 0 {
        return Err(Error::ObjectTrailingBytes { leftover });
    }
    Ok(())
}

fn checked_container_count(
    raw_count: i32,
    payload_len: usize,
    what: &'static str,
) -> Result<usize> {
    usize::try_from(raw_count)
        .map_err(|_| Error::NegativeCount {
            what,
            raw: raw_count,
        })
        .map(|count| count.min(payload_len))
}

pub(crate) fn skip_value(
    state: &mut State,
    type_ref: TypeReference,
    cur: &mut Cursor<'_>,
    is_diff: bool,
) -> Result<()> {
    if type_ref.is_builtin() {
        return skip_primitive(state, type_ref.as_builtin()?, cur, is_diff);
    }
    skip_user_class(state, type_ref, cur, is_diff)
}

fn skip_user_class(
    state: &mut State,
    type_ref: TypeReference,
    cur: &mut Cursor<'_>,
    is_diff: bool,
) -> Result<()> {
    nested(state, |state| {
        skip_user_class_body(state, type_ref, cur, is_diff)
    })
}

fn skip_user_class_body(
    state: &mut State,
    type_ref: TypeReference,
    cur: &mut Cursor<'_>,
    is_diff: bool,
) -> Result<()> {
    let (field_layout, read_order) = {
        let class = state.class_for(type_ref)?;
        (class.fields.clone(), class.read_order.clone())
    };
    let mut chunk_fields = Vec::new();
    if !is_diff {
        // #5323 (PAR-D3-2026-10-05-01) — walk the offset-sorted
        // `read_order`, exactly like `read_user_class_body` (#3398:
        // "any sequential reader MUST walk this order"). This path used
        // declaration order, so on a class whose declaration order
        // differs from offset order AND that carries a variable-size
        // inline field (`String`) or a chunk field (`List`/`Map`) among
        // the reordered ones, skip and read disagree on how many bytes
        // a field consumes or which side chunk belongs to which field.
        // Vanilla hides this (XMCOLOR is the only divergent class and
        // its four u8s consume identically in both orders); a mod or
        // Creation CDB desynced into ObjectTrailingBytes / a skipped
        // instance consuming the wrong side chunks.
        for idx in &read_order {
            let field = &field_layout[*idx as usize];
            if state.is_chunk_type(field.type_ref) {
                chunk_fields.push(field.type_ref);
            } else {
                skip_value(state, field.type_ref, cur, is_diff)?;
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
                })?;
            if state.is_chunk_type(field.type_ref) {
                chunk_fields.push(field.type_ref);
            } else {
                skip_value(state, field.type_ref, cur, is_diff)?;
            }
        }
    }
    for type_ref in chunk_fields {
        skip_chunk_value(state, type_ref, is_diff)?;
    }
    Ok(())
}

fn skip_chunk_value(state: &mut State, type_ref: TypeReference, is_diff: bool) -> Result<()> {
    if type_ref.is_builtin() {
        match type_ref.as_builtin()? {
            BuiltinType::List => skip_list(state, is_diff),
            BuiltinType::Map => skip_map(state, is_diff),
            _ => Err(Error::UnsupportedBuiltin {
                raw: type_ref.id as u32,
            }),
        }
    } else {
        skip_object(state)
    }
}

fn skip_primitive(
    state: &mut State,
    bt: BuiltinType,
    cur: &mut Cursor<'_>,
    is_diff: bool,
) -> Result<()> {
    match bt {
        BuiltinType::Null => Ok(()),
        BuiltinType::String => {
            let len = cur.read_u16()? as usize;
            let _bytes = cur.read_bytes(len)?;
            Ok(())
        }
        BuiltinType::List | BuiltinType::Map => Err(Error::UnsupportedBuiltin { raw: bt as u32 }),
        BuiltinType::Ref => skip_primitive_ref(state, cur, is_diff),
        BuiltinType::Int8 | BuiltinType::UInt8 | BuiltinType::Bool => {
            let _ = cur.read_u8()?;
            Ok(())
        }
        BuiltinType::Int16 | BuiltinType::UInt16 => {
            let _ = cur.read_u16()?;
            Ok(())
        }
        BuiltinType::Int32 | BuiltinType::UInt32 | BuiltinType::Float => {
            let _ = cur.read_u32()?;
            Ok(())
        }
        BuiltinType::Int64 | BuiltinType::UInt64 | BuiltinType::Double => {
            let _ = cur.read_u64()?;
            Ok(())
        }
    }
}

fn skip_primitive_ref(state: &mut State, cur: &mut Cursor<'_>, is_diff: bool) -> Result<()> {
    nested(state, |state| skip_primitive_ref_body(state, cur, is_diff))
}

fn skip_primitive_ref_body(state: &mut State, cur: &mut Cursor<'_>, is_diff: bool) -> Result<()> {
    let type_ref = TypeReference::new(cur.read_i32()?);
    if type_ref.is_builtin() {
        // Mirrors `read_primitive`: list/map references resolve to the
        // null sentinel rather than a side chunk.
        return match type_ref.as_builtin()? {
            BuiltinType::List | BuiltinType::Map => Ok(()),
            bt => skip_primitive(state, bt, cur, is_diff),
        };
    }
    let is_user = state.class_for(type_ref)?.flags.is_user();
    if is_user {
        skip_object(state)
    } else {
        skip_user_class(state, type_ref, cur, is_diff)
    }
}

pub(crate) fn read_value(
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

/// #4272 (SF-D3-2026-09-11-01) — insert a class into the `name_offset`
/// index, rejecting a duplicate `name_offset` instead of the silent
/// `HashMap::insert` overwrite (last-wins). Sibling of #2633's
/// [`insert_field`] below, at the `CLAS` level instead of the field
/// level: the Gibbed reference implementation's `typeMap.Add` throws on
/// a duplicate key, silently discarding the earlier class's definition
/// under whatever code later resolves that `name_offset`.
fn insert_class_name_offset(
    class_by_name_offset: &mut HashMap<i32, usize>,
    classes: &[Class],
    class: &Class,
    idx: usize,
) -> Result<()> {
    if let Some(&prev_idx) = class_by_name_offset.get(&class.name_offset) {
        return Err(Error::DuplicateClassNameOffset {
            name_offset: class.name_offset,
            first_class_index: prev_idx,
            first_class_name: classes[prev_idx].name.clone(),
            duplicate_class_index: idx,
            duplicate_class_name: class.name.clone(),
        });
    }
    class_by_name_offset.insert(class.name_offset, idx);
    Ok(())
}

/// #2633 (SF-D3-05) — insert a field value, rejecting a duplicate field
/// name instead of the silent `BTreeMap::insert` overwrite (last-wins).
/// The Gibbed reference implementation uses `Dictionary.Add`, which
/// throws on a duplicate key; a `CLAS` declaring the same field name
/// twice, or a `DIFF` naming the same field index twice, silently kept
/// the second value pre-fix — a wrong-value risk, not a parse-error
/// risk, for a class shape this parser should instead reject outright.
fn insert_field(
    fields: &mut BTreeMap<String, Value>,
    class_name: &str,
    field_name: String,
    value: Value,
) -> Result<()> {
    if fields.contains_key(&field_name) {
        return Err(Error::DuplicateFieldName {
            class_name: class_name.to_string(),
            field_name,
        });
    }
    fields.insert(field_name, value);
    Ok(())
}

fn read_user_class(
    state: &mut State,
    type_ref: TypeReference,
    cur: &mut Cursor<'_>,
    is_diff: bool,
) -> Result<Value> {
    nested(state, |state| {
        read_user_class_body(state, type_ref, cur, is_diff)
    })
}

fn read_user_class_body(
    state: &mut State,
    type_ref: TypeReference,
    cur: &mut Cursor<'_>,
    is_diff: bool,
) -> Result<Value> {
    let (class_name, class_type_id, field_layout, read_order) = {
        let class = state.class_for(type_ref)?;
        // Clone the field list once so the iterator below doesn't
        // hold a `&Class` while we mutate `state` reading nested
        // objects.
        (
            class.name.clone(),
            class.type_id,
            class.fields.clone(),
            class.read_order.clone(),
        )
    };

    let mut fields: BTreeMap<String, Value> = BTreeMap::new();
    let mut chunk_fields: Vec<Field> = Vec::new();

    if !is_diff {
        // #3398 — inline bytes are laid out by `Field::offset`, which is
        // NOT always declaration order (XMCOLOR). Walk the offset-sorted
        // `read_order`, never `field_layout` directly.
        for idx in &read_order {
            let field = &field_layout[*idx as usize];
            if state.is_chunk_type(field.type_ref) {
                chunk_fields.push(field.clone());
            } else {
                let v = read_value(state, field.type_ref, cur, is_diff)?;
                insert_field(&mut fields, &class_name, field.name.clone(), v)?;
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
                insert_field(&mut fields, &class_name, field.name.clone(), v)?;
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
        insert_field(&mut fields, &class_name, field.name, value)?;
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

/// Every recursion cycle in the value reader passes through either
/// [`read_user_class`] (struct nesting) or this function (a `Ref` chain,
/// which can recurse through `read_primitive` without touching a class), so
/// guarding both bounds all of them. The `skip_*` walker mirrors it.
fn read_primitive_ref(state: &mut State, cur: &mut Cursor<'_>, is_diff: bool) -> Result<Value> {
    nested(state, |state| read_primitive_ref_body(state, cur, is_diff))
}

fn read_primitive_ref_body(
    state: &mut State,
    cur: &mut Cursor<'_>,
    is_diff: bool,
) -> Result<Value> {
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

pub(crate) fn read_primitive_string(cur: &mut Cursor<'_>) -> Result<String> {
    let len = cur.read_u16()? as usize;
    let bytes = cur.read_bytes(len)?;
    // Gibbed reads inline CDB strings with `trimNull=true`. Truncate at the
    // first NUL inside the length-prefixed window before the lossy UTF-8
    // decode so a terminating / embedded `\0` doesn't survive into the
    // `String` and diverge from the reference. SF-D3-AUDIT-02 / #2101.
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

// ── tiny byte-slice cursor (avoids `std::io::Cursor`'s Result-only API) ──

pub(crate) struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub(crate) fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
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

    pub(crate) fn read_u8(&mut self) -> Result<u8> {
        let b = self.read_bytes(1)?;
        Ok(b[0])
    }
    pub(crate) fn read_i8(&mut self) -> Result<i8> {
        self.read_u8().map(|v| v as i8)
    }
    pub(crate) fn read_u16(&mut self) -> Result<u16> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    pub(crate) fn read_i16(&mut self) -> Result<i16> {
        self.read_u16().map(|v| v as i16)
    }
    pub(crate) fn read_u32(&mut self) -> Result<u32> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub(crate) fn read_i32(&mut self) -> Result<i32> {
        self.read_u32().map(|v| v as i32)
    }
    pub(crate) fn read_u64(&mut self) -> Result<u64> {
        let b = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }
    pub(crate) fn read_i64(&mut self) -> Result<i64> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// #5323 (PAR-D3-2026-10-05-01) — skip ≡ read on a reordered class
    /// with a variable-size inline field and a chunk field. Declaration
    /// order (`b: Int32`, `a: String`, `items: List`) disagrees with
    /// offset order (`a@0`, `b@6`, `items@14`); the read path walks
    /// `read_order` (#3398) and the skip path used to walk
    /// `field_layout`, so on exactly this shape the two paths consumed
    /// different byte counts and could bind side chunks to the wrong
    /// field. Vanilla hides it (XMCOLOR's four u8s consume identically
    /// in both orders); the pin is byte-exact cursor agreement plus the
    /// offset-correct field bindings.
    #[test]
    fn skip_and_read_consume_identically_on_a_reordered_class() {
        let int32 = TypeReference::new(BuiltinType::Int32 as i32);
        let string = TypeReference::new(BuiltinType::String as i32);
        let list = TypeReference::new(BuiltinType::List as i32);
        let class = Class {
            name_offset: 7,
            name: "Reordered".to_string(),
            type_id: 99,
            flags: ClassFlags(0),
            fields: vec![
                Field {
                    name: "b".to_string(),
                    type_ref: int32,
                    offset: 6,
                    size: 4,
                },
                Field {
                    name: "a".to_string(),
                    type_ref: string,
                    offset: 0,
                    size: 6,
                },
                Field {
                    name: "items".to_string(),
                    type_ref: list,
                    offset: 14,
                    size: 0,
                },
            ],
            read_order: vec![1, 0, 2], // a@0, b@6, then the List chunk
        };
        // Inline bytes laid out by OFFSET: "abcd" (u16 len + 4 bytes),
        // then the u32 — NOT declaration order (u32 first).
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(b"abcd");
        bytes.extend_from_slice(&0x1122_3344u32.to_le_bytes());
        // The LIST side chunk: elem type Int32, count 1, one u32 element.
        let mut list_payload = Vec::new();
        list_payload.extend_from_slice(&int32.id.to_le_bytes());
        list_payload.extend_from_slice(&1i32.to_le_bytes());
        list_payload.extend_from_slice(&0xCAFE_BABEu32.to_le_bytes());
        let list_start = bytes.len();
        bytes.extend_from_slice(&list_payload);

        let inline_len = 2 + 4 + 4;
        let file_bytes: &[u8] = &bytes;
        let fresh_chunks = || {
            VecDeque::from([Chunk {
                kind: ChunkType::List,
                start: list_start,
                size: list_payload.len(),
            }])
        };

        // READ path — binds values to names and consumes the side chunk.
        let mut read_state = State {
            bytes: file_bytes,
            chunks: fresh_chunks(),
            classes: vec![class.clone()],
            class_by_name_offset: HashMap::from([(7, 0)]),
            strings: StringTable::new(Vec::new()),
            depth: 0,
        };
        let mut read_cur = Cursor::new(file_bytes);
        let value = read_user_class_body(
            &mut read_state,
            TypeReference::new(7),
            &mut read_cur,
            false,
        )
        .expect("read path");

        // SKIP path — identical inputs, fresh chunk queue.
        let mut skip_state = State {
            bytes: file_bytes,
            chunks: fresh_chunks(),
            classes: vec![class],
            class_by_name_offset: HashMap::from([(7, 0)]),
            strings: StringTable::new(Vec::new()),
            depth: 0,
        };
        let mut skip_cur = Cursor::new(file_bytes);
        skip_user_class_body(&mut skip_state, TypeReference::new(7), &mut skip_cur, false)
            .expect("skip path");

        assert_eq!(read_cur.pos, skip_cur.pos, "skip must consume exactly the bytes read does");
        assert_eq!(read_cur.pos, inline_len, "the offset-ordered inline region only");
        assert!(
            read_state.chunks.is_empty() && skip_state.chunks.is_empty(),
            "both paths must consume the LIST side chunk"
        );
        let Value::Object(obj) = value else {
            panic!("expected an object value");
        };
        match (
            obj.fields.get("a"),
            obj.fields.get("b"),
            obj.fields.get("items"),
        ) {
            (
                Some(Value::String(s)),
                Some(Value::I32(v)),
                Some(Value::List(items)),
            ) => {
                assert_eq!(s, "abcd", "offset-order binding: 'abcd' belongs to 'a'");
                assert_eq!(*v, 0x1122_3344u32 as i32, "the u32 belongs to 'b'");
                assert_eq!(items.len(), 1, "the side chunk belongs to 'items'");
            }
            other => panic!("wrong field bindings: {other:?}"),
        }
    }

    /// #2633 (SF-D3-05) — `insert_field` must reject a duplicate field
    /// name instead of silently keeping the second value
    /// (`BTreeMap::insert`'s own overwrite-on-collision behaviour), the
    /// exact "silent wrong value" gap the issue names. Mirrors the
    /// Gibbed reference's `Dictionary.Add`, which throws on a duplicate
    /// key — a `CLAS` declaring the same field name twice, or a `DIFF`
    /// naming the same field index twice, must now be a real parse
    /// error, not a last-wins overwrite.
    #[test]
    fn insert_field_rejects_a_duplicate_field_name() {
        let mut fields: BTreeMap<String, Value> = BTreeMap::new();
        insert_field(&mut fields, "TestClass", "first".to_string(), Value::I32(1))
            .expect("first insert of a fresh field name must succeed");
        insert_field(
            &mut fields,
            "TestClass",
            "second".to_string(),
            Value::I32(2),
        )
        .expect("a distinct field name must not collide with the first");

        let err = insert_field(
            &mut fields,
            "TestClass",
            "first".to_string(),
            Value::I32(99),
        )
        .expect_err("re-declaring an already-present field name must fail");
        match err {
            Error::DuplicateFieldName {
                class_name,
                field_name,
            } => {
                assert_eq!(class_name, "TestClass");
                assert_eq!(field_name, "first");
            }
            other => panic!("expected DuplicateFieldName, got {other:?}"),
        }

        // The pre-existing value must survive untouched — no partial
        // overwrite on the rejected insert. `Value` has no `PartialEq`,
        // so match the variant directly rather than `assert_eq!`.
        match fields.get("first") {
            Some(Value::I32(1)) => {}
            other => panic!("expected the original Value::I32(1) untouched, got {other:?}"),
        }
        assert_eq!(
            fields.len(),
            2,
            "the rejected insert must not add a new entry"
        );
    }

    /// #4272 (SF-D3-2026-09-11-01) — `insert_class_name_offset` must
    /// reject a duplicate `name_offset` instead of the silent
    /// `HashMap::insert` overwrite (last-wins). Sibling of
    /// `insert_field_rejects_a_duplicate_field_name` above, at the `CLAS`
    /// level instead of the field level.
    #[test]
    fn insert_class_name_offset_rejects_a_duplicate_name_offset() {
        let mut class_by_name_offset: HashMap<i32, usize> = HashMap::new();
        let mut classes: Vec<Class> = Vec::new();

        let first = Class {
            name_offset: 42,
            name: "FirstClass".to_string(),
            type_id: 1,
            flags: ClassFlags(0),
            fields: Vec::new(),
            read_order: Vec::new(),
        };
        insert_class_name_offset(&mut class_by_name_offset, &classes, &first, 0)
            .expect("first insert of a fresh name_offset must succeed");
        classes.push(first);

        let distinct = Class {
            name_offset: 43,
            name: "SecondClass".to_string(),
            type_id: 2,
            flags: ClassFlags(0),
            fields: Vec::new(),
            read_order: Vec::new(),
        };
        insert_class_name_offset(&mut class_by_name_offset, &classes, &distinct, 1)
            .expect("a distinct name_offset must not collide with the first");
        classes.push(distinct);

        let duplicate = Class {
            name_offset: 42, // reuses FirstClass's name_offset
            name: "DuplicateClass".to_string(),
            type_id: 3,
            flags: ClassFlags(0),
            fields: Vec::new(),
            read_order: Vec::new(),
        };
        let err = insert_class_name_offset(&mut class_by_name_offset, &classes, &duplicate, 2)
            .expect_err("re-declaring an already-claimed name_offset must fail");
        match err {
            Error::DuplicateClassNameOffset {
                name_offset,
                first_class_index,
                first_class_name,
                duplicate_class_index,
                duplicate_class_name,
            } => {
                assert_eq!(name_offset, 42);
                assert_eq!(first_class_index, 0);
                assert_eq!(first_class_name, "FirstClass");
                assert_eq!(duplicate_class_index, 2);
                assert_eq!(duplicate_class_name, "DuplicateClass");
            }
            other => panic!("expected DuplicateClassNameOffset, got {other:?}"),
        }

        // The pre-existing mapping must survive untouched — no partial
        // overwrite on the rejected insert.
        assert_eq!(class_by_name_offset.get(&42), Some(&0));
        assert_eq!(
            class_by_name_offset.len(),
            2,
            "the rejected insert must not add a new entry"
        );
    }

    fn field(name: &str, offset: u16, size: u16) -> Field {
        Field {
            name: name.to_string(),
            type_ref: TypeReference::new(-1),
            offset,
            size,
        }
    }

    /// #4275 (SF-D3-2026-09-11-04) → #3398 fix — the common-case shape
    /// (96 of 97 vanilla classes): declaration order already ascends by
    /// offset, so `read_order` is the identity permutation.
    #[test]
    fn read_order_identity_for_common_shape() {
        let fields = vec![
            field("r", 0, 1),
            field("g", 1, 1),
            field("b", 2, 1),
            field("a", 3, 1),
        ];
        let order = offset_read_order(&fields);
        assert_eq!(order, vec![0, 1, 2, 3]);
    }

    /// #3398 — `XMCOLOR`'s actual measured wire shape: `r,g,b,a`
    /// declared, but offsets `2,1,0,3` (a straight R<->B transposition).
    /// `read_order` must reorder to wire-offset order (b, g, r, a), which
    /// is what makes the sequential reader bind the right bytes to the
    /// right channels.
    #[test]
    fn read_order_reorders_xmcolor_shape() {
        let fields = vec![
            field("r", 2, 1),
            field("g", 1, 1),
            field("b", 0, 1),
            field("a", 3, 1),
        ];
        let order = offset_read_order(&fields);
        assert_eq!(order, vec![2, 1, 0, 3]);
    }

    /// Zero or one field: the (trivially sorted) permutation of nothing.
    #[test]
    fn read_order_trivial_cases() {
        assert!(offset_read_order(&[]).is_empty());
        assert_eq!(offset_read_order(&[field("only", 5, 1)]), vec![0]);
    }

    /// SF-D3-AUDIT-01 / #2100 — `probe_header` must validate the header +
    /// chunk index WITHOUT walking (or even semantically checking) the
    /// instance chunks. Proof: a file whose chunk *table* is well-formed
    /// but whose chunk *order* is invalid for a real parse (an `OBJT`
    /// where the `STRT`/`TYPE` chunks belong) probes clean yet fails the
    /// full parse — so the probe cannot have touched the instance walk.
    #[test]
    fn probe_header_skips_instance_walk() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&SIGNATURE_BETH.to_le_bytes());
        buf.extend_from_slice(&HEADER_SIZE.to_le_bytes());
        buf.extend_from_slice(&FILE_VERSION.to_le_bytes());
        buf.extend_from_slice(&3u32.to_le_bytes()); // chunkCount incl BETH → 2 chunks
        buf.extend_from_slice(b"OBJT"); // instance chunk where STRT/TYPE belong
        buf.extend_from_slice(&0u32.to_le_bytes()); // size 0
        buf.extend_from_slice(b"STRT");
        buf.extend_from_slice(&0u32.to_le_bytes()); // size 0

        let info = ComponentDatabaseFile::probe_header(&buf)
            .expect("well-formed header + chunk table must probe clean");
        assert_eq!(info.chunk_count, 2);
        // The full parser walks chunk semantics and rejects the misordered
        // OBJT-before-STRT — work the probe demonstrably skipped.
        assert!(ComponentDatabaseFile::parse(&buf).is_err());
    }

    /// #4273 (SF-D3-2026-09-11-02) — `probe_header` only needs chunk
    /// count/presence, not chunk semantics, so an unrecognized chunk-type
    /// FourCC (a future format revision, mod-authored or corrupted CDB)
    /// must not abort it the way the strict `index_chunks` used by the
    /// real object-tree parse correctly does. Sibling of
    /// `chunk_type_recognized_set_is_pinned` below, which pins the STRICT
    /// path's hard-fail on the same input — this pins the TOLERANT path's
    /// pass-through.
    #[test]
    fn probe_header_tolerates_an_unrecognized_chunk_type() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&SIGNATURE_BETH.to_le_bytes());
        buf.extend_from_slice(&HEADER_SIZE.to_le_bytes());
        buf.extend_from_slice(&FILE_VERSION.to_le_bytes());
        buf.extend_from_slice(&3u32.to_le_bytes()); // chunkCount incl BETH → 2 chunks
        buf.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // unrecognized FourCC
        buf.extend_from_slice(&0u32.to_le_bytes()); // size 0
        buf.extend_from_slice(b"STRT");
        buf.extend_from_slice(&0u32.to_le_bytes()); // size 0

        let info = ComponentDatabaseFile::probe_header(&buf)
            .expect("probe_header must tolerate an unrecognized chunk FourCC");
        assert_eq!(info.chunk_count, 2);

        // The strict full parse still correctly rejects the unrecognized
        // type — this fix is scoped to the probe only, per #1569's
        // deliberate all-or-nothing baseline for the real parse.
        match ComponentDatabaseFile::parse(&buf) {
            Err(Error::UnknownChunkType { raw, index }) => {
                assert_eq!(raw, 0xDEAD_BEEF);
                assert_eq!(index, 0);
            }
            other => panic!(
                "expected the strict parse to still reject the unknown FourCC, got {other:?}"
            ),
        }
    }

    /// #4273 — the tolerant probe path must still refuse to walk past the
    /// buffer: an unrecognized FourCC does not exempt an oversized chunk
    /// from the same overflow guard `index_chunks` applies, since that
    /// check is about stream safety, not chunk semantics.
    #[test]
    fn probe_header_still_rejects_an_oversized_unrecognized_chunk() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&SIGNATURE_BETH.to_le_bytes());
        buf.extend_from_slice(&HEADER_SIZE.to_le_bytes());
        buf.extend_from_slice(&FILE_VERSION.to_le_bytes());
        buf.extend_from_slice(&2u32.to_le_bytes()); // chunkCount incl BETH → 1 chunk
        buf.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // unrecognized FourCC
        buf.extend_from_slice(&1000u32.to_le_bytes()); // size far exceeds remaining bytes
        // No payload bytes follow.

        match ComponentDatabaseFile::probe_header(&buf) {
            Err(Error::ChunkOverflowUnknownType {
                index,
                raw,
                size,
                remaining,
            }) => {
                assert_eq!(index, 0);
                assert_eq!(raw, 0xDEAD_BEEF);
                assert_eq!(size, 1000);
                assert_eq!(remaining, 0);
            }
            other => panic!("expected ChunkOverflowUnknownType, got {other:?}"),
        }
    }

    /// SF-D3-AUDIT-03 / #2102 — the discovery cheap-reject. `peek_magic`
    /// accepts a BETH-signature buffer and rejects a mis-named non-CDB one
    /// (and a too-short buffer) on the first 4 bytes alone.
    #[test]
    fn peek_magic_gates_discovery() {
        assert!(ComponentDatabaseFile::peek_magic(
            &SIGNATURE_BETH.to_le_bytes()
        ));
        assert!(!ComponentDatabaseFile::peek_magic(b"not a cdb"));
        assert!(!ComponentDatabaseFile::peek_magic(b"ab")); // < 4 bytes
    }

    /// SF-D3-AUDIT-02 / #2101 — inline strings follow Gibbed's `trimNull`
    /// semantics: a length-prefixed window containing an embedded or
    /// trailing NUL decodes without the NUL.
    #[test]
    fn read_primitive_string_trims_nul() {
        // Embedded NUL mid-window: "abc\0de" (len 6) → "abc".
        let mut buf = 6u16.to_le_bytes().to_vec();
        buf.extend_from_slice(b"abc\0de");
        assert_eq!(
            read_primitive_string(&mut Cursor::new(&buf)).unwrap(),
            "abc"
        );

        // Trailing NUL padding: "hi\0\0" (len 4) → "hi".
        let mut buf = 4u16.to_le_bytes().to_vec();
        buf.extend_from_slice(b"hi\0\0");
        assert_eq!(read_primitive_string(&mut Cursor::new(&buf)).unwrap(), "hi");

        // No NUL — passes through unchanged.
        let mut buf = 3u16.to_le_bytes().to_vec();
        buf.extend_from_slice(b"xyz");
        assert_eq!(
            read_primitive_string(&mut Cursor::new(&buf)).unwrap(),
            "xyz"
        );
    }

    /// #1569 baseline — the CDB is a flat chunk stream with no
    /// per-instance recovery: a single unrecognised chunk FourCC aborts
    /// the whole 1.44M-material parse (`?` on every dispatch). Pin the
    /// recognised chunk-type set so a future format tag becomes a
    /// deliberate baseline edit here, not a silent runtime drop — and
    /// confirm an unknown FourCC fails loud and diagnosable (raw + index).
    #[test]
    fn chunk_type_recognized_set_is_pinned() {
        let known: &[(u32, ChunkType)] = &[
            (0x54525453, ChunkType::Strt),
            (0x45505954, ChunkType::Type),
            (0x53414C43, ChunkType::Clas),
            (0x544A424F, ChunkType::Objt),
            (0x46464944, ChunkType::Diff),
            (0x52455355, ChunkType::User),
            (0x44525355, ChunkType::Usrd),
            (0x4350414D, ChunkType::Mapc),
            (0x5453494C, ChunkType::List),
        ];
        for (raw, want) in known {
            assert_eq!(
                ChunkType::from_raw(*raw, 0).unwrap(),
                *want,
                "chunk FourCC {raw:#010x} must decode"
            );
        }
        match ChunkType::from_raw(0xDEAD_BEEF, 7) {
            Err(Error::UnknownChunkType { raw, index }) => {
                assert_eq!(raw, 0xDEAD_BEEF);
                assert_eq!(
                    index, 7,
                    "the failing chunk index must be carried for diagnostics"
                );
            }
            other => panic!("expected UnknownChunkType, got {other:?}"),
        }
    }

    /// #1569 baseline — same all-or-nothing brittleness for the
    /// `BuiltinType` low-byte tag. Pin the recognised primitive set; an
    /// undocumented tag must fail with `UnsupportedBuiltin` (carries raw).
    #[test]
    fn builtin_type_recognized_set_is_pinned() {
        let known: &[u32] = &[
            0xFFFFFF01, 0xFFFFFF02, 0xFFFFFF03, 0xFFFFFF04, 0xFFFFFF05, 0xFFFFFF08, 0xFFFFFF09,
            0xFFFFFF0A, 0xFFFFFF0B, 0xFFFFFF0C, 0xFFFFFF0D, 0xFFFFFF0E, 0xFFFFFF0F, 0xFFFFFF10,
            0xFFFFFF11, 0xFFFFFF12,
        ];
        for raw in known {
            assert!(
                BuiltinType::from_u32(*raw).is_ok(),
                "builtin tag {raw:#010x} must decode"
            );
        }
        // 0xFFFFFF07 sits in the gap between Ref (…05) and Int8 (…08).
        match BuiltinType::from_u32(0xFFFF_FF07) {
            Err(Error::UnsupportedBuiltin { raw }) => assert_eq!(raw, 0xFFFF_FF07),
            other => panic!("expected UnsupportedBuiltin, got {other:?}"),
        }
    }

    /// #1569 baseline — only `IsUser | IsStruct` are recognised
    /// class-flag bits; any other bit aborts the whole parse. Pin the
    /// mask so a new reflection flag is a conscious edit.
    #[test]
    fn class_flags_known_mask_is_pinned() {
        assert_eq!(ClassFlags::IS_USER, 1 << 2);
        assert_eq!(ClassFlags::IS_STRUCT, 1 << 3);
        assert_eq!(ClassFlags::KNOWN, 0b1100);
        // A bit outside the mask is detected as unknown by parse_class.
        assert_ne!(0x0001u16 & !ClassFlags::KNOWN, 0);
    }

    /// Build a minimal valid CDB whose single CLAS carries `flags`.
    /// Layout: BETH header + STRT ("TestClass") + TYPE (count=1) + CLAS
    /// (name_offset=0, type_id=0, flags, 0 fields). `chunkCount` includes
    /// the BETH header, so BETH + STRT + TYPE + CLAS = 4.
    fn synthetic_cdb_with_class_flags(flags: u16) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SIGNATURE_BETH.to_le_bytes());
        bytes.extend_from_slice(&HEADER_SIZE.to_le_bytes());
        bytes.extend_from_slice(&FILE_VERSION.to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes()); // chunkCount incl. BETH

        let strt: &[u8] = b"TestClass\0";
        bytes.extend_from_slice(&(ChunkType::Strt as u32).to_le_bytes());
        bytes.extend_from_slice(&(strt.len() as u32).to_le_bytes());
        bytes.extend_from_slice(strt);

        bytes.extend_from_slice(&(ChunkType::Type as u32).to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes()); // type_count

        let mut clas = Vec::new();
        clas.extend_from_slice(&0i32.to_le_bytes()); // name_offset → "TestClass"
        clas.extend_from_slice(&0u32.to_le_bytes()); // type_id
        clas.extend_from_slice(&flags.to_le_bytes()); // flags
        clas.extend_from_slice(&0u16.to_le_bytes()); // field_count
        bytes.extend_from_slice(&(ChunkType::Clas as u32).to_le_bytes());
        bytes.extend_from_slice(&(clas.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&clas);

        bytes
    }

    /// #1569 — an unknown class flag must abort with a diagnostic that
    /// NAMES the offending class (index + editor name), not just the raw
    /// flag value. Pre-fix the operator got "unknown class flags 0xNNNN"
    /// with no anchor into the 1.44M-material set.
    #[test]
    fn unknown_class_flag_names_offending_class() {
        let cdb = synthetic_cdb_with_class_flags(0x0001);
        match ComponentDatabaseFile::parse(&cdb) {
            Err(Error::UnknownClassFlags {
                raw,
                class_index,
                class_name,
            }) => {
                assert_eq!(raw, 0x0001);
                assert_eq!(class_index, 0);
                assert_eq!(class_name, "TestClass");
            }
            other => panic!("expected UnknownClassFlags naming the class, got {other:?}"),
        }
    }

    /// Sanity: the SAME synthetic CDB with a valid flag (IsStruct) parses
    /// cleanly — proves the failure above is the flag check, not a
    /// malformed fixture.
    #[test]
    fn synthetic_cdb_with_valid_flag_parses() {
        let cdb = synthetic_cdb_with_class_flags(ClassFlags::IS_STRUCT);
        let parsed = ComponentDatabaseFile::parse(&cdb).expect("valid-flag CDB parses");
        assert_eq!(parsed.classes.len(), 1);
        assert_eq!(parsed.classes[0].name, "TestClass");
    }

    #[test]
    fn streaming_visitor_delivers_and_drops_each_top_level_value() {
        let mut cdb = synthetic_cdb_with_class_flags(ClassFlags::IS_STRUCT);
        // Add one zero-field object referring to TestClass at STRT offset 0.
        cdb[12..16].copy_from_slice(&5u32.to_le_bytes()); // BETH + 4 chunks
        cdb.extend_from_slice(&(ChunkType::Objt as u32).to_le_bytes());
        cdb.extend_from_slice(&4u32.to_le_bytes());
        cdb.extend_from_slice(&0i32.to_le_bytes());

        let mut class_names = Vec::new();
        let info = ComponentDatabaseFile::visit_instances_with_limits(
            &cdb,
            ParseLimits::unlimited(),
            |value| match value {
                Value::Object(object) => class_names.push(object.class_name.clone()),
                other => panic!("expected object value, got {other:?}"),
            },
        )
        .expect("streaming visitor must decode the same valid CDB object");

        assert_eq!(info.class_count, 1);
        assert_eq!(info.value_count, 1);
        assert_eq!(class_names, ["TestClass"]);
    }

    #[test]
    fn validation_walks_a_nested_list_without_materialising_it() {
        let mut cdb = synthetic_cdb_with_class_flags(ClassFlags::IS_STRUCT);
        // Add a top-level LIST<bool> with two values. Unlike the generic
        // reader, the validator must consume its nested values without
        // building `Value::List` (the real CDB has one very large shape).
        cdb[12..16].copy_from_slice(&5u32.to_le_bytes()); // BETH + 4 chunks
        let mut list = Vec::new();
        list.extend_from_slice(&(BuiltinType::Bool as u32 as i32).to_le_bytes());
        list.extend_from_slice(&2i32.to_le_bytes());
        list.extend_from_slice(&[1, 0]);
        cdb.extend_from_slice(&(ChunkType::List as u32).to_le_bytes());
        cdb.extend_from_slice(&(list.len() as u32).to_le_bytes());
        cdb.extend_from_slice(&list);

        let info =
            ComponentDatabaseFile::validate_instances_with_limits(&cdb, ParseLimits::unlimited())
                .expect("validator must consume nested list values");
        assert_eq!(info.class_count, 1);
        assert_eq!(info.value_count, 1);
    }

    /// Regression: #2614 / SF-D3-01 — a hostile on-disk `chunkCount`
    /// (`0xFFFF_FFFF`, requesting ~103 GB of `VecDeque<Chunk>` capacity
    /// pre-fix) must fail with a proper `Err` from `index_chunks`'s
    /// bounds-checked reads, not panic/abort the process via
    /// `with_capacity`. A well-formed header with no chunk data follows —
    /// pre-fix this test never reached the assertion at all.
    #[test]
    fn hostile_chunk_count_errors_instead_of_aborting() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SIGNATURE_BETH.to_le_bytes());
        bytes.extend_from_slice(&HEADER_SIZE.to_le_bytes());
        bytes.extend_from_slice(&FILE_VERSION.to_le_bytes());
        bytes.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // chunkCount incl. BETH

        match ComponentDatabaseFile::parse(&bytes) {
            Err(_) => {} // any Err is acceptable — the point is "not a panic/abort"
            Ok(_) => panic!("a hostile chunk_count with no backing chunk data must not parse Ok"),
        }
    }

    /// Sibling to the above for the CDB reflection-payload allocation
    /// sites (`parse_class`'s `fields`, `consume_list`'s `items`,
    /// `consume_map`'s `pairs`) — same unvalidated-on-disk-count shape.
    /// A hostile `field_count` on an otherwise well-formed chunk table
    /// must fail bounds-checked (`ClassTrailingBytes`/`UnexpectedEof`),
    /// not over-allocate.
    #[test]
    fn hostile_class_field_count_errors_instead_of_aborting() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SIGNATURE_BETH.to_le_bytes());
        bytes.extend_from_slice(&HEADER_SIZE.to_le_bytes());
        bytes.extend_from_slice(&FILE_VERSION.to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes()); // chunkCount incl. BETH

        let strt: &[u8] = b"TestClass\0";
        bytes.extend_from_slice(&(ChunkType::Strt as u32).to_le_bytes());
        bytes.extend_from_slice(&(strt.len() as u32).to_le_bytes());
        bytes.extend_from_slice(strt);

        bytes.extend_from_slice(&(ChunkType::Type as u32).to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes()); // type_count

        // CLAS with a hostile field_count but no field data behind it.
        let mut clas = Vec::new();
        clas.extend_from_slice(&0i32.to_le_bytes()); // name_offset
        clas.extend_from_slice(&0u32.to_le_bytes()); // type_id
        clas.extend_from_slice(&(ClassFlags::IS_STRUCT).to_le_bytes()); // flags
        clas.extend_from_slice(&0xFFFFu16.to_le_bytes()); // field_count HOSTILE
        bytes.extend_from_slice(&(ChunkType::Clas as u32).to_le_bytes());
        bytes.extend_from_slice(&(clas.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&clas);

        match ComponentDatabaseFile::parse(&bytes) {
            Err(_) => {}
            Ok(_) => panic!("a hostile field_count with no backing field data must not parse Ok"),
        }
    }

    #[test]
    fn parse_with_limits_rejects_object_tree_before_materialising_it() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SIGNATURE_BETH.to_le_bytes());
        bytes.extend_from_slice(&HEADER_SIZE.to_le_bytes());
        bytes.extend_from_slice(&FILE_VERSION.to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes()); // BETH + STRT + TYPE + OBJT
        bytes.extend_from_slice(&(ChunkType::Strt as u32).to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&(ChunkType::Type as u32).to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&(ChunkType::Objt as u32).to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());

        let err =
            ComponentDatabaseFile::parse_with_limits(&bytes, ParseLimits { max_instances: 0 })
                .expect_err("the object chunk must be rejected before semantic decoding");
        assert!(matches!(
            err,
            Error::ParseBudgetExceeded {
                requested: 1,
                limit: 0
            }
        ));
    }

    // ── #2623 (SF-D3-02) — LIST/MAPC negative/oversized count ───────
    //
    // `consume_list`/`consume_map` are exercised directly against a
    // hand-built `State` rather than a full top-level CDB file: they're
    // the smallest unit that reproduces the bug (a LIST/MAPC chunk's own
    // payload bytes), and a full file needs a STRT/TYPE/OBJT chunk
    // scaffold just to reach the same code path the other tests in this
    // file already cover that scaffold for.

    fn state_with_single_chunk(payload: &[u8], kind: ChunkType) -> State<'_> {
        State {
            bytes: payload,
            chunks: VecDeque::from([Chunk {
                kind,
                start: 0,
                size: payload.len(),
            }]),
            classes: Vec::new(),
            class_by_name_offset: HashMap::new(),
            strings: StringTable::new(Vec::new()),
            depth: 0,
        }
    }

    /// The issue's headline case: a negative on-disk LIST count must be
    /// rejected explicitly, not sign-extend through `as usize` into
    /// ~1.8e19. Pre-#2614 this reached `Vec::with_capacity` and panicked;
    /// post-#2614 (pre-this-fix) it no longer panicked but still drove
    /// `for _ in 0..count` with the huge value directly.
    #[test]
    fn negative_list_count_is_rejected() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&(0xFFFFFF10u32 as i32).to_le_bytes()); // elem_ref: Bool
        payload.extend_from_slice(&(-1i32).to_le_bytes()); // count: negative
        let mut state = state_with_single_chunk(&payload, ChunkType::List);
        let err = consume_list(&mut state, false).expect_err("negative count must be rejected");
        assert!(matches!(err, Error::NegativeCount { raw: -1, .. }));
    }

    /// Same case for MAPC's count.
    #[test]
    fn negative_map_count_is_rejected() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&(0xFFFFFF10u32 as i32).to_le_bytes()); // key_ref: Bool
        payload.extend_from_slice(&(0xFFFFFF10u32 as i32).to_le_bytes()); // val_ref: Bool
        payload.extend_from_slice(&i32::MIN.to_le_bytes()); // count: most-negative
        let mut state = state_with_single_chunk(&payload, ChunkType::Mapc);
        let err = consume_map(&mut state, false).expect_err("negative count must be rejected");
        assert!(matches!(err, Error::NegativeCount { raw: i32::MIN, .. }));
    }

    /// The issue's second required case: an oversized but non-negative
    /// count (no sign-extension involved) must also fail cleanly rather
    /// than allocate/loop far past what the payload actually contains.
    /// The payload after the 8-byte header has room for exactly 3 Bool
    /// elements (1 byte each); a count of 1000 must error once those 3
    /// bytes are exhausted, not succeed or hang.
    #[test]
    fn oversized_positive_list_count_errors_on_exhaustion() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&(0xFFFFFF10u32 as i32).to_le_bytes()); // elem_ref: Bool
        payload.extend_from_slice(&1000i32.to_le_bytes()); // count: oversized
        payload.extend_from_slice(&[1u8, 0u8, 1u8]); // only 3 elements' worth of data
        let mut state = state_with_single_chunk(&payload, ChunkType::List);
        assert!(
            consume_list(&mut state, false).is_err(),
            "an oversized count must error once the backing payload is exhausted, not hang"
        );
    }

    /// A count that's oversized but still `<= payload.len()` (so the
    /// `.min(payload.len())` clamp doesn't reject it outright) must
    /// still terminate with an `Err` once the real data runs out, not
    /// read past the payload or panic.
    #[test]
    fn count_within_clamp_but_past_real_data_errors_cleanly() {
        // count=5 survives the `.min(payload.len())` clamp (the clamp
        // only bounds against the WHOLE payload, header included) but
        // still overruns the single real element that follows.
        let mut payload = Vec::new();
        payload.extend_from_slice(&(0xFFFFFF10u32 as i32).to_le_bytes()); // elem_ref: Bool
        payload.extend_from_slice(&5i32.to_le_bytes()); // count: 5
        payload.push(1u8); // one real element
        let mut state = state_with_single_chunk(&payload, ChunkType::List);
        assert!(
            consume_list(&mut state, false).is_err(),
            "reading past the last real element must error, not panic"
        );
    }

    /// Sanity check alongside the rejection tests above: a genuinely
    /// well-formed count must still parse normally — the new guards
    /// reject only corrupt/adversarial data.
    #[test]
    fn well_formed_list_count_still_parses() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&(0xFFFFFF10u32 as i32).to_le_bytes()); // elem_ref: Bool
        payload.extend_from_slice(&2i32.to_le_bytes()); // count: 2, matches the data below
        payload.extend_from_slice(&[1u8, 0u8]);
        let mut state = state_with_single_chunk(&payload, ChunkType::List);
        let value = consume_list(&mut state, false).expect("well-formed LIST must parse");
        match value {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert!(matches!(items[0], Value::Bool(true)));
                assert!(matches!(items[1], Value::Bool(false)));
            }
            other => panic!("expected Value::List, got {other:?}"),
        }
    }

    /// Append one `[kind][size][payload]` chunk.
    fn push_chunk(bytes: &mut Vec<u8>, kind: ChunkType, payload: &[u8]) {
        bytes.extend_from_slice(&(kind as u32).to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(payload);
    }

    /// BETH header + STRT ("A") + TYPE (1) + one CLAS + one OBJT chunk.
    fn synthetic_cdb_with_one_object(clas: &[u8], objt: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&SIGNATURE_BETH.to_le_bytes());
        bytes.extend_from_slice(&HEADER_SIZE.to_le_bytes());
        bytes.extend_from_slice(&FILE_VERSION.to_le_bytes());
        bytes.extend_from_slice(&5u32.to_le_bytes()); // chunkCount incl. BETH
        push_chunk(&mut bytes, ChunkType::Strt, b"A\0");
        push_chunk(&mut bytes, ChunkType::Type, &1u32.to_le_bytes());
        push_chunk(&mut bytes, ChunkType::Clas, clas);
        push_chunk(&mut bytes, ChunkType::Objt, objt);
        bytes
    }

    /// #4657 — the audit's self-referential file: an IS_STRUCT class whose
    /// single inline field has the class itself as its type, plus one OBJT of
    /// that class. Every level recurses without consuming a byte; pre-fix
    /// both walkers overflowed the stack and aborted the process.
    fn self_referential_struct_cdb() -> Vec<u8> {
        let mut clas = Vec::new();
        clas.extend_from_slice(&0i32.to_le_bytes()); // name_offset → "A"
        clas.extend_from_slice(&0u32.to_le_bytes()); // type_id
        clas.extend_from_slice(&ClassFlags::IS_STRUCT.to_le_bytes());
        clas.extend_from_slice(&1u16.to_le_bytes()); // field_count
        clas.extend_from_slice(&0i32.to_le_bytes()); // field name → "A"
        clas.extend_from_slice(&0i32.to_le_bytes()); // field type → class "A" itself
        clas.extend_from_slice(&0u16.to_le_bytes()); // offset
        clas.extend_from_slice(&0u16.to_le_bytes()); // size
        synthetic_cdb_with_one_object(&clas, &0i32.to_le_bytes())
    }

    #[test]
    fn self_referential_struct_is_rejected_instead_of_overflowing_the_stack() {
        let cdb = self_referential_struct_cdb();
        let limits = ParseLimits {
            max_instances: 1_000_000,
        };
        let err = ComponentDatabaseFile::parse_with_limits(&cdb, limits)
            .expect_err("unbounded self-nesting must not parse");
        assert!(
            matches!(
                err,
                Error::NestingTooDeep {
                    limit: MAX_NESTING_DEPTH
                }
            ),
            "got {err:?}"
        );
        let err = ComponentDatabaseFile::validate_instances_with_limits(&cdb, limits)
            .expect_err("the skip walker must be bounded too");
        assert!(matches!(err, Error::NestingTooDeep { .. }), "got {err:?}");
    }

    /// #4657 — the other recursion cycle: a chain of builtin `Ref`s recurses
    /// `read_primitive_ref` → `read_primitive` → `read_primitive_ref` without
    /// ever touching a class, 4 bytes per level. 200 levels is far past the
    /// cap yet tiny on disk.
    #[test]
    fn builtin_ref_chain_is_bounded() {
        let mut clas = Vec::new();
        clas.extend_from_slice(&0i32.to_le_bytes());
        clas.extend_from_slice(&0u32.to_le_bytes());
        clas.extend_from_slice(&ClassFlags::IS_STRUCT.to_le_bytes());
        clas.extend_from_slice(&0u16.to_le_bytes()); // no fields
        let mut objt = Vec::new();
        objt.extend_from_slice(&(BuiltinType::Ref as u32).to_le_bytes()); // object type
        for _ in 0..200 {
            objt.extend_from_slice(&(BuiltinType::Ref as u32).to_le_bytes());
        }
        objt.extend_from_slice(&(BuiltinType::Null as u32).to_le_bytes());
        let cdb = synthetic_cdb_with_one_object(&clas, &objt);
        let err = ComponentDatabaseFile::parse(&cdb).expect_err("200-deep Ref chain");
        assert!(matches!(err, Error::NestingTooDeep { .. }), "got {err:?}");
        let err =
            ComponentDatabaseFile::validate_instances_with_limits(&cdb, ParseLimits::unlimited())
                .expect_err("200-deep Ref chain, skip walker");
        assert!(matches!(err, Error::NestingTooDeep { .. }), "got {err:?}");
    }

    /// Nesting under the cap still decodes: a 3-deep `Ref` chain ending in
    /// `Null`, and the depth counter unwinds so a second object parses too.
    #[test]
    fn shallow_ref_chain_still_parses() {
        let mut clas = Vec::new();
        clas.extend_from_slice(&0i32.to_le_bytes());
        clas.extend_from_slice(&0u32.to_le_bytes());
        clas.extend_from_slice(&ClassFlags::IS_STRUCT.to_le_bytes());
        clas.extend_from_slice(&0u16.to_le_bytes());
        let mut objt = Vec::new();
        objt.extend_from_slice(&(BuiltinType::Ref as u32).to_le_bytes());
        for _ in 0..3 {
            objt.extend_from_slice(&(BuiltinType::Ref as u32).to_le_bytes());
        }
        objt.extend_from_slice(&(BuiltinType::Null as u32).to_le_bytes());
        let cdb = synthetic_cdb_with_one_object(&clas, &objt);
        let file = ComponentDatabaseFile::parse(&cdb).expect("shallow chain parses");
        assert_eq!(file.instances.len(), 1);
        ComponentDatabaseFile::validate_instances_with_limits(&cdb, ParseLimits::unlimited())
            .expect("shallow chain validates");
    }
}
