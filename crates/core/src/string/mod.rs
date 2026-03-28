//! String interning for the engine.
//!
//! All entity names, asset paths, and shader identifiers go through
//! [`StringPool`]. Equality checks on [`FixedString`] are integer
//! comparisons — O(1), zero allocation after first intern.

use crate::ecs::resource::Resource;

/// An interned string handle. Equality is integer comparison, O(1).
pub type FixedString = string_interner::DefaultSymbol;

/// Thread-safe string interner, registered as a global [`Resource`].
///
/// Access via `world.resource::<StringPool>()` (read) or
/// `world.resource_mut::<StringPool>()` (intern new strings).
pub struct StringPool(string_interner::StringInterner<string_interner::backend::StringBackend>);

impl StringPool {
    pub fn new() -> Self {
        Self(string_interner::StringInterner::<string_interner::backend::StringBackend>::new())
    }

    /// Intern a string, returning its symbol. If the string was already
    /// interned, returns the existing symbol with no allocation.
    pub fn intern(&mut self, s: &str) -> FixedString {
        self.0.get_or_intern(s)
    }

    /// Resolve a symbol back to its string slice.
    pub fn resolve(&self, sym: FixedString) -> Option<&str> {
        self.0.resolve(sym)
    }

    /// Look up a string without interning it. Returns `None` if the
    /// string has never been interned.
    pub fn get(&self, s: &str) -> Option<FixedString> {
        self.0.get(s)
    }
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
}
