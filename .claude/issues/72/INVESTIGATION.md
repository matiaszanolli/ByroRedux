# Investigation: Issue #72

## NiStencilProperty Layout
- Oblivion (v <= 20.0.0.5): NiObjectNET + enabled(u8) + function(u32) + ref(u32) + mask(u32) + fail(u32) + zfail(u32) + pass(u32) + draw_mode(u32)
- FO3+ (v >= 20.1.0.3): NiObjectNET + flags(u16) + ref(u32) + mask(u32)
  - draw_mode packed in flags bits 6-7

## NiZBufferProperty Layout
- Oblivion (v <= 20.0.0.5): NiObjectNET + flags(u16) + function(u32)
- FO3+ (v >= 20.1.0.3): NiObjectNET + flags(u16)
  - function packed in flags bits 2-5

## Fix
1. Add parsers in properties.rs (version-aware)
2. Register in mod.rs dispatch
3. Update extract_material_info to use NiStencilProperty directly
4. Remove NiUnknown heuristic for NiStencilProperty

## Scope
2 files: properties.rs (parsers), mod.rs (dispatch), import.rs (remove heuristic).
