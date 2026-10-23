//! WTHS instance-value decode (#5627) — the value half of the BGS
//! reflection container that [`super::reflection`] walks the
//! schema of. The grammar below was recovered from `Starfield.esm`
//! itself (xEdit has no shipped reader for the instance chunks), by
//! decoding `WeatherClearTemplate`'s REFL by hand against its own CLAS
//! schema and then validating every chunk of all 181 vanilla WTHS
//! records against it — the same standard the schema half was held to
//! (#5364/#5514).
//!
//! ```text
//! root instance  := OBJT  type_ref: i32, class_body
//! class_body     := (inline fields in field-offset order, no prefixes;
//!                    then side-chunk values for the chunk-typed fields,
//!                    in walk-encounter order)
//! inline value   := builtin → fixed bytes / u16-length string /
//!                    Ref → (i32 target_ref, target value) /
//!                    non-user class → nested class_body (struct)
//! LIST           := elem_ref: i32, count: i32, count × value(elem_ref)
//! MAPC           := key_ref: i32, val_ref: i32, count: i32,
//!                   count × (value, value)
//! USER / USRD    := class_ref: i32, per-leaf typed stream over the
//!                   user class's fields in offset order, trailing u32
//! typed stream   := every LEAF value is prefixed with its type ref:
//!                   builtin leaf → (i32 type == field type, value);
//!                   Ref leaf → (i32 target_ref, target value) — the
//!                   `Ref` node itself carries no prefix; a struct field
//!                   expands in place (no prefix of its own)
//! ```
//!
//! The USER/USRD typed stream differs from the CDB file reader's
//! prefix-free class body (`crates/sfmaterial/src/reader.rs`
//! `read_user_class_body`) — the WTHS writer self-describes leaf types.
//! `WeatherClearTemplate`'s `WeatherSound.SoundHook` side chunk pins
//! this byte-for-byte: `BGSAudio::WwiseSoundHook` = 4 × `UInt64` GUID
//! halves + 2 × null `UInt32` Ref targets + trailing 0 = exactly its
//! 72 bytes, and every other shape that fits 72 bytes misparses the
//! GUIDs. A chunk whose walk disagrees with the schema (a prefix that
//! isn't the field's type, trailing bytes, an unexpected side-chunk
//! signature, truncation) declines the whole instance decode rather
//! than guess — `BgsReflection::root_instance` stays `None` and the
//! record carries the chunk census only, exactly the pre-#5627 state.

use super::reflection::{
    builtin, builtin_entry, ReflectionClass, ReflectionField, InlineValue, CLASS_FLAG_USER,
    MAX_NESTING_DEPTH,
};
use crate::esm::reader::FormIdRemap;

/// The `0xFFFF` field-index terminator shared with the DIFF walk.
const DIFF_TERMINATOR: u16 = 0xFFFF;

/// One decoded WTHS value — the dynamic tree the renderer-facing
/// synthesis walks by class/field name. `Ref` keeps its target type so
/// a FormID leaf can be told from a plain `UInt32`: only `Ref` leaves
/// are load-order remapped.
#[derive(Debug, Clone, PartialEq)]
pub enum ReflectionValue {
    Null,
    Bool(bool),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    F32(f32),
    F64(f64),
    String(String),
    /// The `Ref` builtin: a target type ref (negative = builtin,
    /// non-negative = class name offset) plus the referent's value.
    /// Numeric targets carry the raw FormID in `value`, already
    /// remapped into the record's load-order space.
    Ref {
        target_type: i32,
        value: Box<ReflectionValue>,
    },
    List(Vec<ReflectionValue>),
    Map(Vec<(ReflectionValue, ReflectionValue)>),
    Object(ReflectionObject),
}

/// One class-instance value — fields bind by name, in walk order.
#[derive(Debug, Clone, PartialEq)]
pub struct ReflectionObject {
    pub class: String,
    pub fields: Vec<(String, ReflectionValue)>,
}

impl ReflectionValue {
    pub fn as_object(&self) -> Option<&ReflectionObject> {
        match self {
            ReflectionValue::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        match self {
            ReflectionValue::F32(v) => Some(*v),
            _ => None,
        }
    }

    /// A `Ref` leaf's FormID (numeric target only).
    pub fn as_form_id(&self) -> Option<u32> {
        match self {
            ReflectionValue::Ref { value, .. } => match value.as_ref() {
                ReflectionValue::U32(id) => Some(*id),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            ReflectionValue::String(s) => Some(s),
            _ => None,
        }
    }
}

impl ReflectionObject {
    pub fn field(&self, name: &str) -> Option<&ReflectionValue> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
}

/// One decoded DIFF override: the field it names (index into the
/// diff's own class, kept beside the resolved name so merge sites don't
/// need the schema) and its decoded value.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffOverride {
    pub field_index: u16,
    pub field_name: String,
    pub value: ReflectionValue,
}

/// Byte cursor over one chunk payload — every read is bounds-checked;
/// a truncation ends the decode (declines, never guesses).
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let out = self.data.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(out)
    }


    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }





    fn at_end(&self) -> bool {
        self.pos == self.data.len()
    }
}

/// The instance decoder: the complete schema (every CLAS precedes the
/// instance chunks in authored order) plus the side-chunk queue every
/// chunk-typed field consumes from, in authored order.
struct InstanceReader<'a> {
    classes: &'a [ReflectionClass],
    side: std::collections::VecDeque<([u8; 4], &'a [u8])>,
    depth: usize,
    remap: Option<&'a FormIdRemap>,
}

impl<'a> InstanceReader<'a> {
    fn class_for(&self, name_offset: i32) -> Option<&'a ReflectionClass> {
        self.classes.iter().find(|c| c.name_offset == name_offset)
    }

    /// `sfmaterial`'s `is_chunk_type`: builtin List/Map, or a
    /// user-flagged class — the field kinds whose values ride side
    /// chunks.
    fn is_chunk_type(&self, type_ref: i32) -> bool {
        if type_ref < 0 {
            matches!(type_ref, builtin::LIST | builtin::MAP)
        } else {
            self.class_for(type_ref)
                .is_some_and(|c| c.flags & CLASS_FLAG_USER != 0)
        }
    }

    fn nested<T>(&mut self, body: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        if self.depth >= MAX_NESTING_DEPTH {
            return None;
        }
        self.depth += 1;
        let out = body(self);
        self.depth -= 1;
        out
    }

    /// A `Ref` target's value: builtin target → primitive; class target
    /// → its own value walk. The recursion edge is depth-capped like
    /// every other cycle path (#5687 — a `Ref`-to-`Ref` chain recursed
    /// per 4 payload bytes pre-cap).
    fn read_ref_target(&mut self, target: i32, cur: &mut Cursor) -> Option<ReflectionValue> {
        self.nested(|reader| match target {
            t if t < 0 => read_builtin(t, cur, reader),
            t => reader.read_value(t, cur, false),
        })
    }

    /// Read one value of `type_ref` at `cur` (the CDB reader's
    /// `read_value`). `is_diff` only changes class-body layout (pair
    /// stream vs offset order).
    fn read_value(
        &mut self,
        type_ref: i32,
        cur: &mut Cursor,
        is_diff: bool,
    ) -> Option<ReflectionValue> {
        self.nested(|reader| {
            if type_ref < 0 {
                return read_builtin(type_ref, cur, reader);
            }
            let class = reader.class_for(type_ref)?;
            if class.flags & CLASS_FLAG_USER != 0 {
                read_user_side_chunk(reader, class)
            } else {
                read_class_body(class, cur, is_diff, reader)
                    .map(ReflectionValue::Object)
            }
        })
    }

    /// A chunk-typed field's value: builtin `List`/`Map` consume a
    /// `LIST`/`MAPC` side chunk; a user class consumes its `USER`/`USRD`
    /// cast chunk.
    fn read_chunk_field(&mut self, type_ref: i32, is_diff: bool) -> Option<ReflectionValue> {
        if type_ref < 0 {
            let (sig, payload) = self.side.pop_front()?;
            let mut cur = Cursor::new(payload);
            match (type_ref, sig.as_slice()) {
                (builtin::LIST, b"LIST") => {
                    let elem_ref = cur.i32()?;
                    let raw_count = cur.i32()?;
                    if raw_count < 0 {
                        return None;
                    }
                    // No chunk can hold more elements than it has bytes
                    // for (the CDB reader's #2623 clamp) — a hostile
                    // count truncates instead of looping forever.
                    let count = (raw_count as usize).min(payload.len());
                    let mut items = Vec::with_capacity(count);
                    for _ in 0..count {
                        // The element encoding follows the enclosing
                        // context (the CDB reader's `consume_list(
                        // state, is_diff)`): a root template's list
                        // elements are prefix-free class bodies, a
                        // DIFF-context list element is a pair stream
                        // with its own deferred chunk fields (pinned by
                        // WeatherThunderstormC01H100D075's `Sounds`
                        // rows, whose `WeatherSound` elements carry
                        // (idx, value) pairs and a deferred `SoundHook`
                        // USRD each).
                        items.push(self.read_value(elem_ref, &mut cur, is_diff)?);
                    }
                    cur.at_end().then_some(ReflectionValue::List(items))
                }
                (builtin::MAP, b"MAPC") => {
                    let key_ref = cur.i32()?;
                    let val_ref = cur.i32()?;
                    let raw_count = cur.i32()?;
                    if raw_count < 0 {
                        return None;
                    }
                    let count = (raw_count as usize).min(payload.len());
                    let mut pairs = Vec::with_capacity(count);
                    for _ in 0..count {
                        let k = self.read_value(key_ref, &mut cur, is_diff)?;
                        let v = self.read_value(val_ref, &mut cur, is_diff)?;
                        pairs.push((k, v));
                    }
                    cur.at_end().then_some(ReflectionValue::Map(pairs))
                }
                _ => None,
            }
        } else {
            let class = self.class_for(type_ref)?;
            read_user_side_chunk(self, class)
        }
    }
}

/// A user-class field's `USER`/`USRD` side chunk: the leading i32 must
/// name the field's own class (a mismatch declines — the stream's
/// meaning would be unknown), then the per-leaf typed stream, then the
/// trailing u32 the CDB reader's `consume_object` also skips.
fn read_user_side_chunk(
    reader: &mut InstanceReader,
    class: &ReflectionClass,
) -> Option<ReflectionValue> {
    reader.nested(|reader| {
        let (sig, payload) = reader.side.pop_front()?;
        if !matches!(sig.as_slice(), b"USER" | b"USRD") {
            return None;
        }
        let mut cur = Cursor::new(payload);
        let class_ref = cur.i32()?;
        if class_ref != class.name_offset {
            return None;
        }
        let mut fields = Vec::new();
        for i in offset_read_order(class) {
            let field = &class.fields[i];
            let value = read_typed_leaf(field, &mut cur, reader)?;
            fields.push((field.name.clone(), value));
        }
        // Trailing u32 — purpose undocumented (Gibbed); consumed so a
        // stray tail can't leak into the next field's prefix.
        cur.u32()?;
        cur.at_end().then_some(ReflectionValue::Object(ReflectionObject {
            class: class.name.clone(),
            fields,
        }))
    })
}

/// Indices into `fields` sorted by ascending wire offset — the walk
/// order for any sequential reader over a class's inline bytes (the CDB
/// reader's `read_order`; declaration order disagrees with offsets on
/// real classes, sfmaterial #3398).
fn offset_read_order(class: &ReflectionClass) -> Vec<usize> {
    let mut order: Vec<usize> = (0..class.fields.len()).collect();
    order.sort_by_key(|&i| class.fields[i].offset);
    order
}

/// A non-user class's inline body (`read_class_body`): fields in offset
/// order, chunk-typed fields deferred and consumed from the side queue
/// right after the inline ones, in encounter order.
fn read_class_body(
    class: &ReflectionClass,
    cur: &mut Cursor,
    is_diff: bool,
    reader: &mut InstanceReader,
) -> Option<ReflectionObject> {
    reader.nested(|reader| {
        let mut fields: Vec<(String, ReflectionValue)> = Vec::new();
        let mut chunk_fields: Vec<&ReflectionField> = Vec::new();

        if !is_diff {
            for i in offset_read_order(class) {
                let field = &class.fields[i];
                if reader.is_chunk_type(field.type_ref) {
                    chunk_fields.push(field);
                } else {
                    let v = reader.read_value(field.type_ref, cur, false)?;
                    fields.push((field.name.clone(), v));
                }
            }
        } else {
            loop {
                let idx = cur.u16()?;
                if idx == DIFF_TERMINATOR {
                    break;
                }
                let field = class.fields.get(idx as usize)?;
                if reader.is_chunk_type(field.type_ref) {
                    chunk_fields.push(field);
                } else {
                    let v = reader.read_value(field.type_ref, cur, true)?;
                    fields.push((field.name.clone(), v));
                }
            }
        }

        for field in chunk_fields {
            let value = reader.read_chunk_field(field.type_ref, is_diff)?;
            fields.push((field.name.clone(), value));
        }

        Some(ReflectionObject {
            class: class.name.clone(),
            fields,
        })
    })
}

/// Inline builtin value (the CDB reader's `read_primitive` + its `Ref`
/// walk). An inline `List`/`Map` type reads as `Null` — the value rides
/// a side chunk, so meeting the type inline means unauthored. `Ref`
/// targets that resolve to numbers are FormIDs and get the record's
/// load-order remap here, once, at construction.
fn read_builtin(
    type_ref: i32,
    cur: &mut Cursor,
    reader: &mut InstanceReader,
) -> Option<ReflectionValue> {
    let Some(&(_, _, inline)) = builtin_entry(type_ref) else {
        return None; // unknown built-in — refuse to guess
    };
    Some(match inline {
        InlineValue::Nothing => ReflectionValue::Null,
        InlineValue::LengthPrefixedString => {
            let len = cur.u16()? as usize;
            let bytes = cur.take(len)?;
            // The CDB reader trims an embedded NUL inside the length
            // window (#2101) — same rule here.
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            ReflectionValue::String(String::from_utf8_lossy(&bytes[..end]).into_owned())
        }
        InlineValue::Ref => {
            let target = cur.i32()?;
            let value = reader.read_ref_target(target, cur)?;
            ReflectionValue::Ref {
                target_type: target,
                value: Box::new(remap_ref_value(value, reader.remap)),
            }
        }
        InlineValue::Fixed(n) => {
            let bytes = cur.take(n)?;
            match type_ref {
                builtin::INT8 => ReflectionValue::I8(bytes[0] as i8),
                builtin::UINT8 => ReflectionValue::U8(bytes[0]),
                builtin::INT16 => {
                    ReflectionValue::I16(i16::from_le_bytes(bytes.try_into().unwrap()))
                }
                builtin::UINT16 => {
                    ReflectionValue::U16(u16::from_le_bytes(bytes.try_into().unwrap()))
                }
                builtin::INT32 => {
                    ReflectionValue::I32(i32::from_le_bytes(bytes.try_into().unwrap()))
                }
                builtin::UINT32 => {
                    ReflectionValue::U32(u32::from_le_bytes(bytes.try_into().unwrap()))
                }
                builtin::INT64 => {
                    ReflectionValue::I64(i64::from_le_bytes(bytes.try_into().unwrap()))
                }
                builtin::UINT64 => {
                    ReflectionValue::U64(u64::from_le_bytes(bytes.try_into().unwrap()))
                }
                builtin::BOOL => ReflectionValue::Bool(bytes[0] != 0),
                builtin::FLOAT => ReflectionValue::F32(f32::from_bits(u32::from_le_bytes(
                    bytes.try_into().unwrap(),
                ))),
                builtin::DOUBLE => ReflectionValue::F64(f64::from_bits(u64::from_le_bytes(
                    bytes.try_into().unwrap(),
                ))),
                _ => return None, // a Fixed builtin with no arm — table drift
            }
        }
    })
}

/// One field's leaf (or subtree) in a USER/USRD typed stream. Every
/// leaf is self-describing: the stream prefix must BE the field's type;
/// a `Ref` leaf carries its target type instead (the `Ref` node itself
/// is elided, which is why `0xFFFFFF0D` opens a null form ref in
/// `WeatherClearTemplate`'s hook chunk). Anything else declines.
fn read_typed_leaf(
    field: &ReflectionField,
    cur: &mut Cursor,
    reader: &mut InstanceReader,
) -> Option<ReflectionValue> {
    if field.type_ref < 0 {
        if reader.is_chunk_type(field.type_ref) {
            // A List/Map field inside a user class cannot ride the
            // typed stream — the writer emits side chunks per FIELD of
            // the class the chunk names, not per leaf. Vanilla user
            // classes have none; decline rather than guess.
            return None;
        }
        if field.type_ref == builtin::REF {
            let target = cur.i32()?;
            let value = reader.read_ref_target(target, cur)?;
            return Some(ReflectionValue::Ref {
                target_type: target,
                value: Box::new(remap_ref_value(value, reader.remap)),
            });
        }
        let prefix = cur.i32()?;
        if prefix != field.type_ref {
            return None; // self-description disagrees with the schema
        }
        return read_builtin(prefix, cur, reader);
    }
    let class = reader.class_for(field.type_ref)?;
    if class.flags & CLASS_FLAG_USER != 0 {
        return None; // nested user class — would need its own side chunk
    }
    // Struct: expands in place, no prefix of its own.
    reader.nested(|reader| {
        let mut fields = Vec::new();
        for i in offset_read_order(class) {
            let field = &class.fields[i];
            let value = read_typed_leaf(field, cur, reader)?;
            fields.push((field.name.clone(), value));
        }
        Some(ReflectionValue::Object(ReflectionObject {
            class: class.name.clone(),
            fields,
        }))
    })
}

/// Remap a numeric `Ref` target through the record's load order.
/// Non-numeric targets (struct/class referents) pass through untouched.
/// Zero stays zero (the null form), matching `remap_fid`.
fn remap_ref_value(value: ReflectionValue, remap: Option<&FormIdRemap>) -> ReflectionValue {
    match value {
        ReflectionValue::U32(id) => ReflectionValue::U32(match id {
            0 => 0,
            _ => remap.map_or(id, |r| r.remap(id)),
        }),
        other => other,
    }
}

/// Outcome of [`decode_instances`] — the decoded pieces plus how many
/// side chunks went unconsumed (0 on a clean walk; any other count is
/// the diagnostic that the grammar drifted from the data).
pub(super) struct DecodedInstances {
    pub root: Option<ReflectionObject>,
    pub diff: Option<Vec<DiffOverride>>,
    pub unconsumed_chunks: usize,
}

/// Decode a whole blob's instance half: the root `OBJT` (always first
/// in authored order) plus the `DIFF`'s value pairs, sharing one
/// side-chunk queue. The `diff_chunk` carries the raw DIFF payload and
/// the leading type ref's class name offset (resolved at chunk-walk
/// time). Any failure declines that piece (`None`) and leaves the chunk
/// census as the record's evidence; vanilla decodes clean.
pub(super) fn decode_instances(
    instance_payloads: &[([u8; 4], &[u8])],
    diff_chunk: Option<(&[u8], i32)>,
    classes: &[ReflectionClass],
    remap: Option<&FormIdRemap>,
) -> DecodedInstances {
    let mut reader = InstanceReader {
        classes,
        side: instance_payloads.iter().copied().collect(),
        depth: 0,
        remap,
    };

    // Root instance: the front of the queue, only when it is an OBJT
    // (an RDIF blob has no root instance — just DIFF + side chunks).
    let root = if reader.side.front().is_some_and(|(sig, _)| *sig == *b"OBJT") {
        let (_, payload) = reader.side.pop_front().expect("front checked above");
        let mut cur = Cursor::new(payload);
        let decoded = reader.nested(|reader| {
            let type_ref = cur.i32()?;
            let class = reader.class_for(type_ref)?;
            if class.flags & CLASS_FLAG_USER != 0 {
                return None; // the root is a plain class, not a cast
            }
            read_class_body(class, &mut cur, false, reader)
        });
        // Trailing bytes decline the root: the walk and the schema must
        // account for every byte.
        decoded.filter(|_| cur.at_end())
    } else {
        None
    };

    let mut diff = None;
    if let Some((diff_payload, class_ref)) = diff_chunk {
        if let Some(class) = classes.iter().find(|c| c.name_offset == class_ref) {
            let mut cur = Cursor::new(diff_payload);
            let mut overrides = Vec::new();
            let ok = reader.nested(|reader| {
                cur.i32()?; // leading type ref, resolved at walk time
                loop {
                    let idx = cur.u16()?;
                    if idx == DIFF_TERMINATOR {
                        return Some(());
                    }
                    let field = class.fields.get(idx as usize)?;
                    let value = if reader.is_chunk_type(field.type_ref) {
                        reader.read_chunk_field(field.type_ref, true)?
                    } else {
                        reader.read_value(field.type_ref, &mut cur, true)?
                    };
                    overrides.push(DiffOverride {
                        field_index: idx,
                        field_name: field.name.clone(),
                        value,
                    });
                }
            });
            if ok.is_some() && cur.at_end() {
                diff = Some(overrides);
            }
        }
    }

    DecodedInstances {
        root,
        diff,
        unconsumed_chunks: reader.side.len(),
    }
}
