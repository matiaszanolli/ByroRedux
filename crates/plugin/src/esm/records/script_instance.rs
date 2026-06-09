//! `VMAD` (Virtual Machine ADapter) decode — Skyrim+/FO4 per-instance
//! Papyrus script attachments + their authored property values.
//!
//! Pre-M47.2 the parser surfaced only a `has_script: bool` presence
//! flag. M47.2's canonical scripting-translation layer needs the actual
//! attached-script names + property bindings (which quest, which object
//! a script's `ObjectReference Property` resolves to) to populate the
//! canonical behavior components — so this decodes the structure.
//!
//! ## Layout (empirically derived + cross-validated)
//!
//! No public byte-spec was on hand (OpenMW skips VMAD; xEdit not
//! available), so the layout was derived by decoding real `Skyrim.esm`
//! VMAD records and confirming every field cross-validates: version in
//! range, readable ASCII script/property names, property types in the
//! known enum, and `Object` values resolving to sane FormIDs / alias
//! indices. All sampled ACTI VMADs consumed their byte count exactly
//! (e.g. `WinterholdJailTriggerScript` 136/136, `DA16Erandur…` 98/140).
//!
//! ```text
//! i16 version        (5 on Skyrim SE)
//! i16 objectFormat   (2 on Skyrim SE)
//! u16 scriptCount
//! per script:
//!   u16 nameLen + name (ASCII, no null)
//!   u8  status         (only if version >= 4)
//!   u16 propCount
//!   per property:
//!     u16 nameLen + name
//!     u8  type
//!     u8  status       (only if version >= 4)
//!     value, by type:
//!       1  Object  → objectFormat 2: {u16 unused, i16 alias, u32 formId}
//!                    objectFormat 1: {u32 formId, i16 alias, u16 unused}
//!       2  String  → u16 len + chars
//!       3  Int32   → i32
//!       4  Float   → f32
//!       5  Bool    → u8
//!       11-15 Array → u32 count + `count` elements of the base type
//! ```
//!
//! QUST / INFO / PACK / SCEN carry a trailing *fragment* section after
//! the scripts; ACTI / REFR / STAT / CONT (the records the M47.2
//! recognizers consume) do not — the scripts section is the whole VMAD
//! for them. The decoder reads the scripts section and ignores any
//! trailing bytes (graceful — fragment decode is a later phase).
//!
//! Parsing is bounds-checked and *graceful*: a truncated VMAD yields the
//! scripts decoded so far rather than panicking, matching the engine's
//! recover-don't-crash parse philosophy.

/// An authored Papyrus property value attached to a script instance.
/// Only the scalar + array core types are decoded; `Object` carries the
/// raw plugin-local FormID (the consumer applies any FormID remap).
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    /// Type 1 — a form reference. `alias` is the quest-alias index when
    /// the property is alias-bound (`-1` = none).
    Object { form_id: u32, alias: i16 },
    /// Type 2.
    String(String),
    /// Type 3.
    Int32(i32),
    /// Type 4.
    Float(f32),
    /// Type 5.
    Bool(bool),
    /// Types 11-15 — homogeneous array of the base type.
    Array(Vec<PropertyValue>),
    /// A property type outside the decoded set (e.g. a struct/var on
    /// FO4, or a malformed type). Carries the raw type tag; the rest of
    /// this script's properties are abandoned (we can't know the width).
    Unknown(u8),
}

/// One authored property: name + value.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptProperty {
    pub name: String,
    /// VMAD per-property status byte (version >= 4); `1` = edited, the
    /// common case. `0` on older content where the field is absent.
    pub status: u8,
    pub value: PropertyValue,
}

/// One attached Papyrus script + its authored properties.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptInstance {
    pub name: String,
    pub status: u8,
    pub properties: Vec<ScriptProperty>,
}

impl ScriptInstance {
    /// First property matching `name` (case-insensitive — Papyrus
    /// identifiers are case-insensitive).
    pub fn property(&self, name: &str) -> Option<&ScriptProperty> {
        self.properties
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Convenience: the FormID of an `Object`-typed property by name.
    pub fn object_form_id(&self, name: &str) -> Option<u32> {
        match self.property(name)?.value {
            PropertyValue::Object { form_id, .. } => Some(form_id),
            _ => None,
        }
    }
}

/// All script instances attached to a record/reference via its VMAD.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptInstanceData {
    pub version: i16,
    pub object_format: i16,
    pub scripts: Vec<ScriptInstance>,
}

impl ScriptInstanceData {
    /// True when at least one script is attached. The drop-in
    /// replacement for the old `has_script: bool` flag.
    pub fn has_script(&self) -> bool {
        !self.scripts.is_empty()
    }

    /// First attached script matching `name` (case-insensitive).
    pub fn script(&self, name: &str) -> Option<&ScriptInstance> {
        self.scripts
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Decode a `VMAD` sub-record payload. Graceful: returns whatever
    /// decoded cleanly before any truncation/unknown-type.
    pub fn parse(data: &[u8]) -> Self {
        let mut c = Cursor::new(data);
        let version = c.i16().unwrap_or(0);
        let object_format = c.i16().unwrap_or(0);
        let mut out = ScriptInstanceData {
            version,
            object_format,
            scripts: Vec::new(),
        };
        let Some(script_count) = c.u16() else {
            return out;
        };
        let has_status = version >= 4;
        for _ in 0..script_count {
            let Some(name) = c.wstring() else { break };
            let status = if has_status { c.u8().unwrap_or(0) } else { 0 };
            let Some(prop_count) = c.u16() else { break };
            let mut properties = Vec::with_capacity(prop_count as usize);
            let mut script_ok = true;
            for _ in 0..prop_count {
                let Some(pname) = c.wstring() else {
                    script_ok = false;
                    break;
                };
                let Some(ptype) = c.u8() else {
                    script_ok = false;
                    break;
                };
                let pstatus = if has_status { c.u8().unwrap_or(0) } else { 0 };
                match c.property_value(ptype, object_format) {
                    Some(value @ PropertyValue::Unknown(_)) => {
                        // Width of an unknown type is unknowable — keep
                        // what we have and stop this script's props.
                        properties.push(ScriptProperty {
                            name: pname,
                            status: pstatus,
                            value,
                        });
                        script_ok = false;
                        break;
                    }
                    Some(value) => properties.push(ScriptProperty {
                        name: pname,
                        status: pstatus,
                        value,
                    }),
                    None => {
                        script_ok = false;
                        break;
                    }
                }
            }
            out.scripts.push(ScriptInstance {
                name,
                status,
                properties,
            });
            if !script_ok {
                break;
            }
        }
        out
    }
}

/// Minimal bounds-checked little-endian cursor over a VMAD payload.
struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    fn i16(&mut self) -> Option<i16> {
        self.take(2).map(|b| i16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i32(&mut self) -> Option<i32> {
        self.take(4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f32(&mut self) -> Option<f32> {
        self.take(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// u16-length-prefixed string (no null terminator).
    fn wstring(&mut self) -> Option<String> {
        let len = self.u16()? as usize;
        let bytes = self.take(len)?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    /// Decode one property value by type tag + object format.
    fn property_value(&mut self, ptype: u8, object_format: i16) -> Option<PropertyValue> {
        Some(match ptype {
            1 => {
                // Object: byte order flips with objectFormat.
                let (form_id, alias) = if object_format == 1 {
                    let f = self.u32()?;
                    let a = self.i16()?;
                    let _unused = self.u16()?;
                    (f, a)
                } else {
                    let _unused = self.u16()?;
                    let a = self.i16()?;
                    let f = self.u32()?;
                    (f, a)
                };
                PropertyValue::Object { form_id, alias }
            }
            2 => PropertyValue::String(self.wstring()?),
            3 => PropertyValue::Int32(self.i32()?),
            4 => PropertyValue::Float(self.f32()?),
            5 => PropertyValue::Bool(self.u8()? != 0),
            11..=15 => {
                // Array of the base type (11 → 1, 12 → 2, …).
                let base = ptype - 10;
                let count = self.u32()?;
                let mut items = Vec::with_capacity(count.min(4096) as usize);
                for _ in 0..count {
                    match self.property_value(base, object_format)? {
                        v @ PropertyValue::Unknown(_) => return Some(v),
                        v => items.push(v),
                    }
                }
                PropertyValue::Array(items)
            }
            other => PropertyValue::Unknown(other),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic Skyrim-shape VMAD: version 5, objectFormat 2, one script
    // "TestScript" with one Object property "MyQuest" → FormID 0x000242af
    // alias -1. Mirrors the real `Skyrim.esm` ACTI layout the decoder was
    // derived from.
    fn synthetic_vmad() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&5i16.to_le_bytes()); // version
        v.extend_from_slice(&2i16.to_le_bytes()); // objectFormat
        v.extend_from_slice(&1u16.to_le_bytes()); // scriptCount
                                                  // script
        let name = b"TestScript";
        v.extend_from_slice(&(name.len() as u16).to_le_bytes());
        v.extend_from_slice(name);
        v.push(0); // script status (version >= 4)
        v.extend_from_slice(&1u16.to_le_bytes()); // propCount
                                                  // property: Object "MyQuest"
        let pname = b"MyQuest";
        v.extend_from_slice(&(pname.len() as u16).to_le_bytes());
        v.extend_from_slice(pname);
        v.push(1); // type = Object
        v.push(1); // prop status
        v.extend_from_slice(&0u16.to_le_bytes()); // unused (objectFormat 2)
        v.extend_from_slice(&(-1i16).to_le_bytes()); // alias
        v.extend_from_slice(&0x0002_42afu32.to_le_bytes()); // formId
        v
    }

    #[test]
    fn decodes_synthetic_skyrim_vmad() {
        let v = synthetic_vmad();
        let d = ScriptInstanceData::parse(&v);
        assert_eq!(d.version, 5);
        assert_eq!(d.object_format, 2);
        assert!(d.has_script());
        assert_eq!(d.scripts.len(), 1);
        let s = d
            .script("testscript")
            .expect("case-insensitive script lookup");
        assert_eq!(s.name, "TestScript");
        assert_eq!(s.properties.len(), 1);
        assert_eq!(s.object_form_id("MyQuest"), Some(0x0002_42af));
        match &s.property("myquest").unwrap().value {
            PropertyValue::Object { form_id, alias } => {
                assert_eq!(*form_id, 0x0002_42af);
                assert_eq!(*alias, -1);
            }
            other => panic!("expected Object, got {other:?}"),
        }
    }

    #[test]
    fn empty_or_truncated_vmad_is_graceful() {
        assert!(!ScriptInstanceData::parse(&[]).has_script());
        // version + objectFormat present, scriptCount says 1 but the
        // script body is truncated → no script, no panic.
        let mut v = Vec::new();
        v.extend_from_slice(&5i16.to_le_bytes());
        v.extend_from_slice(&2i16.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        let d = ScriptInstanceData::parse(&v);
        assert_eq!(d.version, 5);
        assert!(!d.has_script());
    }

    /// Real-data regression fixture: the `WinterholdJailTriggerScript`
    /// ACTI VMAD from vanilla `Skyrim.esm` (136 bytes, captured verbatim).
    /// This is the empirical record the layout was derived from — pinning
    /// it here makes the real-data validation a permanent guard rather
    /// than a one-off external decode. version 5 / objectFormat 2, one
    /// script, three Object (faction/ref) properties.
    const WINTERHOLD_JAIL_VMAD_HEX: &str = "0500020001001b0057696e746572686f6c644a61696c547269676765725363726970740003000d00506c6179657246616374696f6e01010000ffffb10d00001c0057696e746572686f6c644a61696c4578744174726f6e61636852656601010000ffff4a620900150057696e746572686f6c644a61696c46616374696f6e01010000ffffa80e0000";

    fn from_hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn decodes_real_skyrim_acti_vmad_fixture() {
        let bytes = from_hex(WINTERHOLD_JAIL_VMAD_HEX);
        assert_eq!(bytes.len(), 136);
        let d = ScriptInstanceData::parse(&bytes);
        assert_eq!(d.version, 5);
        assert_eq!(d.object_format, 2);
        assert_eq!(d.scripts.len(), 1);
        let s = &d.scripts[0];
        assert_eq!(s.name, "WinterholdJailTriggerScript");
        assert_eq!(s.properties.len(), 3);
        assert_eq!(s.object_form_id("PlayerFaction"), Some(0x0000_0db1));
        assert_eq!(
            s.object_form_id("WinterholdJailExtAtronachRef"),
            Some(0x0009_624a)
        );
        assert_eq!(s.object_form_id("WinterholdJailFaction"), Some(0x0000_0ea8));
        // All three are non-alias-bound (alias = -1).
        for p in &s.properties {
            match p.value {
                PropertyValue::Object { alias, .. } => assert_eq!(alias, -1),
                ref other => panic!("expected Object, got {other:?}"),
            }
        }
    }

    #[test]
    fn scalar_property_types_decode() {
        // version 2 (no status bytes), objectFormat 2, one script with
        // Int32 / Float / Bool properties.
        let mut v = Vec::new();
        v.extend_from_slice(&2i16.to_le_bytes()); // version (< 4 → no status)
        v.extend_from_slice(&2i16.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        let name = b"S";
        v.extend_from_slice(&(name.len() as u16).to_le_bytes());
        v.extend_from_slice(name);
        // no status byte (version < 4)
        v.extend_from_slice(&3u16.to_le_bytes()); // propCount
        for (pn, ty, bytes) in [
            (&b"I"[..], 3u8, 42i32.to_le_bytes().to_vec()),
            (&b"F"[..], 4u8, 1.5f32.to_le_bytes().to_vec()),
            (&b"B"[..], 5u8, vec![1u8]),
        ] {
            v.extend_from_slice(&(pn.len() as u16).to_le_bytes());
            v.extend_from_slice(pn);
            v.push(ty);
            // no per-prop status (version < 4)
            v.extend_from_slice(&bytes);
        }
        let d = ScriptInstanceData::parse(&v);
        let s = &d.scripts[0];
        assert_eq!(s.property("I").unwrap().value, PropertyValue::Int32(42));
        assert_eq!(s.property("F").unwrap().value, PropertyValue::Float(1.5));
        assert_eq!(s.property("B").unwrap().value, PropertyValue::Bool(true));
    }
}
