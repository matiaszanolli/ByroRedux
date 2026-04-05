# Investigation: Issue #76

## Root Cause
bhk* collision blocks not registered in parse_block dispatch. On FO3+
(v20.2.0.7 with block_sizes), they're skipped as NiUnknown via
block_size fallback. On Oblivion (v20.0.0.5, no block_sizes), the
parser hard-errors.

## Scale of Problem
217 references to bhk/Bhk types in nif.xml. Dozens of deeply nested
Havok physics types. Full parsers would be thousands of lines.

## Fix
Register the ~20 most common bhk* type names as explicit NiUnknown
entries — no parsing, just recognition. On FO3+ they already skip
correctly. On Oblivion, the parser will still fail for these blocks
(no block_size to skip), but now with a clearer error message.

Full Oblivion collision support requires dedicated parsers — defer
to a collision milestone.

## Scope  
1 file: mod.rs (add type names to dispatch as NiUnknown passthrough).
