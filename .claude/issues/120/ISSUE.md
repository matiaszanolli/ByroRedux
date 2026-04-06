# NIF-303: BsTriShape vertex size underflow wraps to huge skip on malformed data

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: MEDIUM | **Dimension**: Stream Position

**Location**: `crates/nif/src/blocks/tri_shape.rs:347-349`
**Game Affected**: All (with malformed data)

### Description

```rust
let consumed = (stream.position() - vert_start) as usize;
if consumed != vertex_size_bytes {
    stream.skip((vertex_size_bytes - consumed) as u64);
}
```

If `consumed > vertex_size_bytes` (malformed vertex descriptor), the subtraction wraps to a huge `usize` in release mode, which converts to `u64` and causes `stream.skip()` to seek far past end-of-data. The error message will be a confusing EOF rather than a clear vertex size mismatch.

### Suggested Fix

```rust
if consumed < vertex_size_bytes {
    stream.skip((vertex_size_bytes - consumed) as u64);
} else if consumed > vertex_size_bytes {
    return Err(io::Error::new(io::ErrorKind::InvalidData,
        format!("vertex consumed {} bytes but descriptor says {}", consumed, vertex_size_bytes)));
}
```

### Completeness Checks

- [ ] **TESTS**: Test with vertex_size smaller than decoded attributes
- [ ] **UNSAFE**: N/A

🤖 Generated with [Claude Code](https://claude.com/claude-code)
