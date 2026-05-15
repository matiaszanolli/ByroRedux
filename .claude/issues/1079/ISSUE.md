# FO4-D2-009: Validate chunk_hdr_len == 24 in BA2 DX10 parsing

**Source**: AUDIT_FO4_2026-05-15.md · LOW  
**Location**: `crates/bsa/src/ba2.rs:466`  
chunk_hdr_len is read as _chunk_hdr_len (discarded) without validation.
