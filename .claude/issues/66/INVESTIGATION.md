# Investigation: Issue #66

## Root Cause
Four functions iterate shape.av.properties independently:
- find_texture_path (lines 506-568) — downcasts to 4 types
- find_alpha_property (lines 574-590) — downcasts to 1 type
- find_two_sided (lines 597-652) — downcasts to 3 types
- find_decal (lines 663-694) — downcasts to 3 types

Each walks the same list, doing up to 4 downcasts per property.
Called from extract_mesh (lines 324-340) which calls 3 of them,
and extract_colors_and_texture which calls find_texture_path.

## Fix
Create MaterialInfo struct populated by a single-pass function.
Replace the 4 independent searches with one call per shape.

## Call Sites
1. extract_mesh (line 324-340): needs alpha, two_sided, decal
2. extract_colors_and_texture (line 478, 494): needs texture_path
3. extract_mesh calls extract_colors_and_texture internally

So the real entry point is extract_mesh — one MaterialInfo per shape.

## Scope
1 file: import.rs. Replace 4 functions with 1 + struct.
