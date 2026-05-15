# FO4-D5-002: Add "NiExtraData" dispatch arm

**Source**: AUDIT_FO4_2026-05-15.md · HIGH  
**Location**: `crates/nif/src/blocks/mod.rs` dispatch table  
100 FO4 FaceGen morph NIFs truncate (99.71% clean → 100% with fix).  
Fix: `"NiExtraData" => Ok(Box::new(NiExtraData::parse(stream, "NiExtraData")?))`.
