//! String interning for the engine.
//!
//! All entity names, asset paths, and shader identifiers go through
//! [`StringPool`]. Equality checks on [`FixedString`] are integer
//! comparisons — O(1).
//!
//! [`StringPool::intern`] and [`StringPool::get`] case-fold via a
//! stack buffer for inputs ≤ [`LOWERCASE_STACK_BUF`] bytes (covers
//! every NIF / ESM / BSA path observed in vanilla content), so the
//! per-call cost is the case-fold copy plus one hash lookup — no
//! `String` allocation. Longer inputs fall back to an allocated
//! lowercase copy. The interner backend itself allocates once per
//! unique string the first time it is seen.

use crate::ecs::resource::Resource;

/// An interned string handle. Equality is integer comparison, O(1).
pub type FixedString = string_interner::DefaultSymbol;

/// Stack buffer size used by the case-fold fast path in
/// [`StringPool::intern`] / [`StringPool::get`]. Sized to cover every
/// string observed in vanilla Bethesda content (longest BSA asset
/// path ~120 bytes; longest NIF node name ~64 bytes); inputs that
/// exceed this fall back to a heap-allocated lowercase copy.
const LOWERCASE_STACK_BUF: usize = 256;

/// Thread-safe string interner, registered as a global [`Resource`].
///
/// Access via `world.resource::<StringPool>()` (read) or
/// `world.resource_mut::<StringPool>()` (intern new strings).
pub struct StringPool(string_interner::StringInterner<string_interner::backend::StringBackend>);

impl StringPool {
    pub fn new() -> Self {
        Self(string_interner::StringInterner::<
            string_interner::backend::StringBackend,
        >::new())
    }

    /// Intern a string, returning its symbol. The interner backend
    /// allocates exactly once for each unique string; subsequent
    /// `intern` calls for the same string (case-insensitive) reuse
    /// the existing entry.
    ///
    /// **Case-insensitive**: strings are lowercased before interning to
    /// match Gamebryo's GlobalStringTable behavior. "Bip01 Head" and
    /// "bip01 head" produce the same symbol.
    ///
    /// The case-fold itself is allocation-free for inputs ≤
    /// [`LOWERCASE_STACK_BUF`] bytes (≥ 99 % of engine call sites);
    /// longer inputs allocate a `String` for the lowercased copy.
    pub fn intern(&mut self, s: &str) -> FixedString {
        let mut buf = [0u8; LOWERCASE_STACK_BUF];
        match ascii_lowercase_into_buf(s, &mut buf) {
            Some(lower) => self.0.get_or_intern(lower),
            None => self.0.get_or_intern(&s.to_ascii_lowercase()),
        }
    }

    /// Resolve a symbol back to its string slice.
    /// Returns the lowercased canonical form — the original case is
    /// not preserved (#895).
    pub fn resolve(&self, sym: FixedString) -> Option<&str> {
        self.0.resolve(sym)
    }

    /// Look up a string without interning it. Returns `None` if the
    /// string has never been interned.
    ///
    /// Case-insensitive: lowercases before lookup. Same fast/slow path
    /// split as [`StringPool::intern`] — no allocation for inputs ≤
    /// [`LOWERCASE_STACK_BUF`] bytes.
    pub fn get(&self, s: &str) -> Option<FixedString> {
        let mut buf = [0u8; LOWERCASE_STACK_BUF];
        match ascii_lowercase_into_buf(s, &mut buf) {
            Some(lower) => self.0.get(lower),
            None => self.0.get(&s.to_ascii_lowercase()),
        }
    }
}

/// Copy `s` into `buf`, ASCII-lowercasing in place, and return the
/// resulting `&str`. Returns `None` when `s` doesn't fit.
///
/// `<[u8]>::make_ascii_lowercase` only flips bit 0x20 on bytes in
/// `0x41..=0x5A`; multi-byte UTF-8 sequences have the high bit set on
/// every byte and are therefore untouched. The output is the same
/// codepoint sequence as the input with ASCII upper-case letters
/// folded — guaranteed valid UTF-8 since the input was a `&str`.
#[inline]
fn ascii_lowercase_into_buf<'a>(s: &str, buf: &'a mut [u8]) -> Option<&'a str> {
    if s.len() > buf.len() {
        return None;
    }
    let dst = &mut buf[..s.len()];
    dst.copy_from_slice(s.as_bytes());
    dst.make_ascii_lowercase();
    // SAFETY: `dst` was filled from a valid `&str`; `make_ascii_lowercase`
    // only touches ASCII single-byte codepoints, so the bytes still form
    // valid UTF-8.
    Some(unsafe { std::str::from_utf8_unchecked(dst) })
}

impl Default for StringPool {
    fn default() -> Self {
        Self::new()
    }
}

impl Resource for StringPool {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_same_string_returns_same_symbol() {
        let mut pool = StringPool::new();
        let a = pool.intern("player");
        let b = pool.intern("player");
        assert_eq!(a, b);
    }

    #[test]
    fn different_strings_different_symbols() {
        let mut pool = StringPool::new();
        let a = pool.intern("player");
        let b = pool.intern("enemy");
        assert_ne!(a, b);
    }

    #[test]
    fn resolve_round_trips() {
        let mut pool = StringPool::new();
        let sym = pool.intern("hello");
        assert_eq!(pool.resolve(sym), Some("hello"));
    }

    #[test]
    fn get_without_interning() {
        let mut pool = StringPool::new();
        assert!(pool.get("missing").is_none());

        let sym = pool.intern("present");
        assert_eq!(pool.get("present"), Some(sym));
        assert!(pool.get("still_missing").is_none());
    }

    #[test]
    fn case_insensitive_interning() {
        let mut pool = StringPool::new();
        let a = pool.intern("Bip01 Head");
        let b = pool.intern("bip01 head");
        let c = pool.intern("BIP01 HEAD");
        assert_eq!(a, b, "mixed case must produce same symbol");
        assert_eq!(b, c, "upper case must produce same symbol");
    }

    #[test]
    fn case_insensitive_get() {
        let mut pool = StringPool::new();
        let sym = pool.intern("Scene Root");
        assert_eq!(pool.get("scene root"), Some(sym));
        assert_eq!(pool.get("SCENE ROOT"), Some(sym));
        assert_eq!(pool.get("Scene Root"), Some(sym));
    }

    #[test]
    fn resolve_returns_lowercase() {
        let mut pool = StringPool::new();
        let sym = pool.intern("Bip01 Spine");
        assert_eq!(pool.resolve(sym), Some("bip01 spine"));
    }

    /// Regression for #893 / LC-D6-NEW-01 — strings that exceed the
    /// 256-byte stack-buffer fast path must still round-trip through
    /// the heap-allocated fallback. The boundary is exclusive: 256
    /// bytes fits, 257 falls back.
    #[test]
    fn long_string_falls_back_correctly() {
        let mut pool = StringPool::new();

        // Exactly LOWERCASE_STACK_BUF bytes — fast path.
        let at_boundary: String = "A".repeat(LOWERCASE_STACK_BUF);
        let sym_boundary = pool.intern(&at_boundary);
        assert_eq!(
            pool.resolve(sym_boundary).map(|s| s.len()),
            Some(LOWERCASE_STACK_BUF)
        );
        assert_eq!(pool.resolve(sym_boundary).unwrap().chars().next(), Some('a'));

        // One byte over — fallback path.
        let over_boundary: String = "B".repeat(LOWERCASE_STACK_BUF + 1);
        let sym_over = pool.intern(&over_boundary);
        assert_eq!(
            pool.resolve(sym_over).map(|s| s.len()),
            Some(LOWERCASE_STACK_BUF + 1)
        );
        assert_eq!(pool.resolve(sym_over).unwrap().chars().next(), Some('b'));

        // Both paths share the pool — distinct strings get distinct symbols.
        assert_ne!(sym_boundary, sym_over);
    }

    /// Regression for #893 / LC-D6-NEW-01 — case-insensitive interning
    /// must produce the same symbol on both the fast and slow paths.
    #[test]
    fn case_insensitive_across_fast_and_slow_paths() {
        let mut pool = StringPool::new();

        // Short string — fast path on both calls.
        let short_a = pool.intern("Bip01");
        let short_b = pool.intern("BIP01");
        assert_eq!(short_a, short_b);

        // Long string — slow path on both calls.
        let long_upper = "X".repeat(LOWERCASE_STACK_BUF + 10);
        let long_mixed = {
            let mut s = long_upper.clone();
            // Force a couple of lowercase letters so case-fold is observable.
            unsafe {
                let bytes = s.as_bytes_mut();
                bytes[0] = b'x';
                bytes[5] = b'x';
            }
            s
        };
        let sym_upper = pool.intern(&long_upper);
        let sym_mixed = pool.intern(&long_mixed);
        assert_eq!(sym_upper, sym_mixed);
    }

    /// Regression for #893 / LC-D6-NEW-01 — non-ASCII (multi-byte UTF-8)
    /// input round-trips through the fast path. `make_ascii_lowercase`
    /// must leave non-ASCII bytes untouched, so the resulting `&str`
    /// stays valid UTF-8.
    #[test]
    fn non_ascii_input_round_trips() {
        let mut pool = StringPool::new();
        // "Naïve Café — résumé" mixes ASCII + Latin-1-supplement codepoints.
        let s = "Naïve Café — Résumé";
        let sym = pool.intern(s);
        let resolved = pool.resolve(sym).unwrap();
        // The Latin-supplement uppercase letters (É, …) are NOT ASCII so
        // `make_ascii_lowercase` leaves them as-is. The ASCII letters
        // (N, C, R) get folded.
        assert_eq!(resolved, "naïve café — résumé");
    }

    /// Regression for #893 / LC-D6-NEW-01 — repeated intern of the same
    /// string returns the same symbol and never grows the pool past one
    /// entry. (Indirectly: if every call were allocating a new entry,
    /// `pool.0.len()` would grow on each call.)
    #[test]
    fn repeated_intern_does_not_grow_pool() {
        let mut pool = StringPool::new();
        let sym = pool.intern("BSFadeNode");
        for _ in 0..100 {
            assert_eq!(pool.intern("BSFadeNode"), sym);
            assert_eq!(pool.intern("bsfadenode"), sym);
            assert_eq!(pool.intern("BSFADENODE"), sym);
        }
        assert_eq!(pool.0.len(), 1, "pool must contain exactly one entry");
    }
}
