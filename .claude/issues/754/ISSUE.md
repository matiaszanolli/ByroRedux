# Issue #754: BSWeakReferenceNode undispatched

Starfield block with no parser dispatch. Hits 7552 NIFs in Meshes02.ba2 (100% truncation).
Wire layout sourced from nifly Nodes.hpp / Nodes.cpp.

## Fix
Added BsWeakReferenceNode parser to node.rs, dispatch in mod.rs, as_ni_node unwrap in walk.rs.
Parser reads and discards the packin/water-ref payload so alignment is maintained.
