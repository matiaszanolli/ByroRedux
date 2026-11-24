//! SKSE's pure natives: `StringUtil`, `Math`'s bit operations and `Log`, and `Utility`'s array creation and
//! resizing; F4SE's `Math` is the same seven functions and `Exp` (LMC-SCR-004 catalog growth,
//! `docs/engine/extender-catalog.md` §8). They read and change nothing in the world. The behaviour is the extenders'
//! own (SKSE64's `PapyrusStringUtil.cpp`, `PapyrusMath.cpp`, `PapyrusUtility.cpp` and F4SE's `PapyrusMath.cpp` at the
//! catalog's pinned commits), read for behaviour only:
//!
//! - SKSE's strings are C byte strings; the engine's are Unicode, decoded from the game's single-byte code page, so a
//!   character here is a byte there. The character classes are the C locale's, which classify ASCII only.
//! - Index and size parameters are `UInt32`: a negative Papyrus `Int` wraps to a huge value, so a negative start finds
//!   nothing, a negative `Substring` length reads to the end, and a negative array size is over any bound.
//! - `Find` and `Split` compare ASCII case-insensitively. `Split`'s delimiter is a set of characters, and empty tokens
//!   are dropped.
//! - `AsOrd` returns the first byte as MSVC's signed `char`, so a byte past 127 is negative; a character no single
//!   byte holds has no answer, and the call stops.

use super::*;

/// The natives: (class, lowercase name, fewest and most arguments, what it returns).
const NATIVES: &[(&str, &str, usize, usize, Kind)] = &[
    ("stringutil", "getlength", 1, 1, Kind::Num),
    ("stringutil", "getnthchar", 2, 2, Kind::Str),
    ("stringutil", "isletter", 1, 1, Kind::Bool),
    ("stringutil", "isdigit", 1, 1, Kind::Bool),
    ("stringutil", "ispunctuation", 1, 1, Kind::Bool),
    ("stringutil", "isprintable", 1, 1, Kind::Bool),
    ("stringutil", "asord", 1, 1, Kind::Num),
    ("stringutil", "aschar", 1, 1, Kind::Str),
    ("stringutil", "find", 2, 3, Kind::Num),
    ("stringutil", "substring", 2, 3, Kind::Str),
    ("stringutil", "split", 2, 2, Kind::Array),
    ("math", "leftshift", 2, 2, Kind::Num),
    ("math", "rightshift", 2, 2, Kind::Num),
    ("math", "logicaland", 2, 2, Kind::Num),
    ("math", "logicalor", 2, 2, Kind::Num),
    ("math", "logicalxor", 2, 2, Kind::Num),
    ("math", "logicalnot", 1, 1, Kind::Num),
    ("math", "log", 1, 1, Kind::Num),
    ("math", "exp", 1, 1, Kind::Num),
    ("utility", "createboolarray", 1, 2, Kind::Array),
    ("utility", "createintarray", 1, 2, Kind::Array),
    ("utility", "createfloatarray", 1, 2, Kind::Array),
    ("utility", "createstringarray", 1, 2, Kind::Array),
    ("utility", "createformarray", 1, 2, Kind::Array),
    ("utility", "createaliasarray", 1, 2, Kind::Array),
    ("utility", "resizeboolarray", 2, 3, Kind::Array),
    ("utility", "resizeintarray", 2, 3, Kind::Array),
    ("utility", "resizefloatarray", 2, 3, Kind::Array),
    ("utility", "resizestringarray", 2, 3, Kind::Array),
    ("utility", "resizeformarray", 2, 3, Kind::Array),
    ("utility", "resizealiasarray", 2, 3, Kind::Array),
];

fn native(class: &str, name: &str) -> Option<&'static (&'static str, &'static str, usize, usize, Kind)> {
    NATIVES.iter().find(|(c, n, ..)| *c == class && *n == name)
}

/// Whether `class.name` is one of these natives.
pub(in crate::fragment::vm) fn is_skse_util(class: &str, name: &str) -> bool {
    native(class, name).is_some()
}

/// An argument as an exact `Int` (an `f32` cannot hold every 32-bit value).
fn int_arg(values: &[Value], index: usize, default: i32) -> i32 {
    values.get(index).map_or(default, |value| value.int().or_else(|| value.number().map(|n| n as i32)).unwrap_or(0))
}

/// An argument as a string.
fn text_arg(values: &[Value], index: usize) -> String {
    match values.get(index) {
        Some(Value::Str(text)) => text.clone(),
        Some(Value::None) | None => String::new(),
        Some(other) => other.text(),
    }
}

/// An index or size parameter as SKSE's `UInt32` reads it.
fn unsigned(value: i32) -> usize {
    value as u32 as usize
}

fn array(values: Vec<Value>) -> Value {
    Value::Array(Rc::new(RefCell::new(values)))
}

/// The first character, classified by the C locale: ASCII only.
fn first_is(text: &str, class: fn(&u8) -> bool) -> bool {
    text.chars().next().is_some_and(|c| c.is_ascii() && class(&(c as u8)))
}

/// `class.name(values)`: SKSE's answer.
pub(in crate::fragment::vm) fn skse_util(class: &str, name: &str, values: &[Value]) -> Run<Value> {
    let int = |index: usize| int_arg(values, index, 0);
    let bits = |index: usize| int(index) as u32;
    Ok(match (class, name) {
        ("stringutil", "getlength") => Value::Int(text_arg(values, 0).chars().count() as i32),
        ("stringutil", "getnthchar") => {
            let text = text_arg(values, 0);
            Value::Str(text.chars().nth(unsigned(int(1))).map(String::from).unwrap_or_default())
        }
        ("stringutil", "isletter") => Value::Bool(first_is(&text_arg(values, 0), u8::is_ascii_alphabetic)),
        ("stringutil", "isdigit") => Value::Bool(first_is(&text_arg(values, 0), u8::is_ascii_digit)),
        ("stringutil", "ispunctuation") => Value::Bool(first_is(&text_arg(values, 0), u8::is_ascii_punctuation)),
        // C's `isprint`: the space and every graphic character.
        ("stringutil", "isprintable") => Value::Bool(first_is(&text_arg(values, 0), |b| *b == b' ' || b.is_ascii_graphic())),
        ("stringutil", "asord") => match text_arg(values, 0).chars().next() {
            None => Value::Int(0),
            Some(c) if (c as u32) < 0x80 => Value::Int(c as i32),
            Some(c) if (c as u32) <= 0xFF => Value::Int(c as u32 as u8 as i8 as i32),
            Some(c) => return stop(format!("StringUtil.AsOrd on {c:?}, which no single byte holds")),
        },
        ("stringutil", "aschar") => {
            let code = bits(0);
            Value::Str(match code {
                1..=255 => char::from(code as u8).to_string(),
                _ => String::new(),
            })
        }
        ("stringutil", "find") => {
            let text: Vec<char> = text_arg(values, 0).chars().map(|c| c.to_ascii_uppercase()).collect();
            let find: Vec<char> = text_arg(values, 1).chars().map(|c| c.to_ascii_uppercase()).collect();
            let start = unsigned(int_arg(values, 2, 0));
            if start >= text.len() {
                return Ok(Value::Int(-1));
            }
            let found = (start..=text.len().saturating_sub(find.len())).find(|&at| text[at..].starts_with(&find));
            Value::Int(found.map_or(-1, |at| at as i32))
        }
        ("stringutil", "substring") => {
            let text: Vec<char> = text_arg(values, 0).chars().collect();
            let start = unsigned(int(1));
            let len = unsigned(int_arg(values, 2, 0));
            if start >= text.len() {
                return Ok(Value::Str(String::new()));
            }
            let end = if len == 0 { text.len() } else { start.saturating_add(len).min(text.len()) };
            Value::Str(text[start..end].iter().collect())
        }
        ("stringutil", "split") => {
            let delimiters: Vec<char> = text_arg(values, 1).chars().map(|c| c.to_ascii_uppercase()).collect();
            let tokens: Vec<Value> = text_arg(values, 0)
                .split(|c: char| delimiters.contains(&c.to_ascii_uppercase()))
                .filter(|token| !token.is_empty())
                .map(|token| Value::Str(token.to_owned()))
                .collect();
            if tokens.len() > MAX_ARRAY_ELEMENTS {
                return stop(format!("StringUtil.Split into {} tokens, over {MAX_ARRAY_ELEMENTS}", tokens.len()));
            }
            array(tokens)
        }
        ("math", "leftshift") => Value::Int(if bits(1) >= 32 { 0 } else { (bits(0) << bits(1)) as i32 }),
        ("math", "rightshift") => Value::Int(if bits(1) >= 32 { 0 } else { (bits(0) >> bits(1)) as i32 }),
        ("math", "logicaland") => Value::Int((bits(0) & bits(1)) as i32),
        ("math", "logicalor") => Value::Int((bits(0) | bits(1)) as i32),
        ("math", "logicalxor") => Value::Int((bits(0) ^ bits(1)) as i32),
        ("math", "logicalnot") => Value::Int(!bits(0) as i32),
        ("math", "log") => Value::Float(values.first().and_then(Value::number).unwrap_or(0.0).ln()),
        ("math", "exp") => Value::Float(values.first().and_then(Value::number).unwrap_or(0.0).exp()),
        ("utility", create) if create.starts_with("create") => {
            let size = unsigned(int(0));
            let fill = values.get(1).cloned().unwrap_or_else(|| fill_default(create));
            if size > MAX_ARRAY_ELEMENTS {
                return stop(format!("Utility.{name} of {size} elements, over {MAX_ARRAY_ELEMENTS}"));
            }
            array(vec![fill; size])
        }
        ("utility", resize) if resize.starts_with("resize") => {
            let size = unsigned(int(1));
            let fill = values.get(2).cloned().unwrap_or_else(|| fill_default(resize));
            if size > MAX_ARRAY_ELEMENTS {
                return stop(format!("Utility.{name} to {size} elements, over {MAX_ARRAY_ELEMENTS}"));
            }
            let mut resized = vec![fill; size];
            if let Some(Value::Array(source)) = values.first() {
                for (slot, value) in resized.iter_mut().zip(source.borrow().iter()) {
                    *slot = value.clone();
                }
            }
            array(resized)
        }
        _ => return stop(format!("{class}.{name}")),
    })
}

/// The declared default fill of a `Create*Array` / `Resize*Array`, by its element type.
fn fill_default(name: &str) -> Value {
    if name.ends_with("boolarray") {
        Value::Bool(false)
    } else if name.ends_with("intarray") {
        Value::Int(0)
    } else if name.ends_with("floatarray") {
        Value::Float(0.0)
    } else if name.ends_with("stringarray") {
        Value::Str(String::new())
    } else {
        Value::None
    }
}

impl Vm<'_> {
    /// A `StringUtil` / SKSE `Math` / SKSE `Utility` call.
    pub(in crate::fragment::vm) fn read_skse_util(
        &mut self,
        frame: &mut Frame,
        class: &str,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        let values = self.eval_args(frame, args)?;
        skse_util(class, name, &values).map(Some)
    }
}

impl Checker<'_> {
    /// Whether the VM runs `class.name` with `args` arguments.
    pub(in crate::fragment::vm) fn check_skse_util(&mut self, class: &str, name: &str, args: usize) -> Option<Kind> {
        match native(class, name) {
            Some((_, _, fewest, most, kind)) if (*fewest..=*most).contains(&args) => Some(*kind),
            _ => self.fail(|| format!("{class}.{name}/{args}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(class: &str, name: &str, values: &[Value]) -> Value {
        skse_util(class, name, values).unwrap_or_else(|_| panic!("{class}.{name} stopped"))
    }
    fn s(text: &str) -> Value {
        Value::Str(text.to_owned())
    }
    fn text(value: Value) -> String {
        match value {
            Value::Str(text) => text,
            other => panic!("not a string: {}", other.text()),
        }
    }
    fn int(value: Value) -> i32 {
        value.int().unwrap_or_else(|| panic!("not an int: {}", value.text()))
    }
    fn items(value: Value) -> Vec<String> {
        match value {
            Value::Array(items) => items.borrow().iter().map(Value::text).collect(),
            other => panic!("not an array: {}", other.text()),
        }
    }

    #[test]
    fn string_reads_follow_skse_including_unsigned_indices() {
        assert_eq!(int(call("stringutil", "getlength", &[s("Whiterun")])), 8);
        assert_eq!(text(call("stringutil", "getnthchar", &[s("Whiterun"), Value::Int(5)])), "r");
        assert_eq!(text(call("stringutil", "getnthchar", &[s("Whiterun"), Value::Int(8)])), "");
        assert_eq!(text(call("stringutil", "getnthchar", &[s("Whiterun"), Value::Int(-1)])), "", "-1 is a huge UInt32");
        // Substring: len 0 (the default) and a negative len both read to the end; a start past the end is "".
        assert_eq!(text(call("stringutil", "substring", &[s("Whiterun"), Value::Int(5)])), "run");
        assert_eq!(text(call("stringutil", "substring", &[s("Whiterun"), Value::Int(0), Value::Int(5)])), "White");
        assert_eq!(text(call("stringutil", "substring", &[s("Whiterun"), Value::Int(5), Value::Int(-1)])), "run");
        assert_eq!(text(call("stringutil", "substring", &[s("Whiterun"), Value::Int(-1)])), "");
        assert_eq!(text(call("stringutil", "substring", &[s("Whiterun"), Value::Int(8)])), "");
    }

    #[test]
    fn find_and_split_ignore_ascii_case() {
        assert_eq!(int(call("stringutil", "find", &[s("Whiterun Hold"), s("HOLD")])), 9);
        assert_eq!(int(call("stringutil", "find", &[s("Whiterun Hold"), s("h"), Value::Int(2)])), 9);
        assert_eq!(int(call("stringutil", "find", &[s("Whiterun"), s("x")])), -1);
        assert_eq!(int(call("stringutil", "find", &[s("Whiterun"), s("w"), Value::Int(8)])), -1, "start past the end");
        assert_eq!(int(call("stringutil", "find", &[s("Whiterun"), s("w"), Value::Int(-1)])), -1, "-1 is a huge UInt32");
        assert_eq!(int(call("stringutil", "find", &[s("Whiterun"), s(""), Value::Int(3)])), 3, "an empty needle is found at the start");
        // The delimiter is a set of characters; empty tokens are dropped; the tokens keep their case.
        assert_eq!(items(call("stringutil", "split", &[s("a,b;;c,"), s(",;")])), ["a", "b", "c"]);
        assert_eq!(items(call("stringutil", "split", &[s("OneXtwoxThree"), s("x")])), ["One", "two", "Three"]);
        assert_eq!(items(call("stringutil", "split", &[s("whole"), s("")])), ["whole"]);
        assert!(items(call("stringutil", "split", &[s(""), s(",")])).is_empty());
    }

    #[test]
    fn character_classes_are_the_c_locales_and_ords_are_signed_bytes() {
        let is = |name: &str, c: &str| call("stringutil", name, &[s(c)]).truthy();
        assert!(is("isletter", "q") && !is("isletter", "1") && !is("isletter", "é") && !is("isletter", ""));
        assert!(is("isdigit", "7") && !is("isdigit", "x"));
        assert!(is("ispunctuation", "!") && !is("ispunctuation", " "));
        assert!(is("isprintable", " ") && is("isprintable", "~") && !is("isprintable", "\t"));
        assert_eq!(int(call("stringutil", "asord", &[s("A")])), 65);
        assert_eq!(int(call("stringutil", "asord", &[s("")])), 0);
        assert_eq!(int(call("stringutil", "asord", &[s("é")])), -23, "0xE9 as MSVC's signed char");
        assert!(skse_util("stringutil", "asord", &[s("€")]).is_err(), "no single byte holds it here");
        assert_eq!(text(call("stringutil", "aschar", &[Value::Int(65)])), "A");
        assert_eq!(text(call("stringutil", "aschar", &[Value::Int(0xE9)])), "é");
        assert_eq!(text(call("stringutil", "aschar", &[Value::Int(256)])), "");
        assert_eq!(text(call("stringutil", "aschar", &[Value::Int(0)])), "");
    }

    #[test]
    fn math_is_unsigned_32_bit() {
        let math = |name: &str, args: &[i32]| int(call("math", name, &args.iter().map(|a| Value::Int(*a)).collect::<Vec<_>>()));
        assert_eq!(math("leftshift", &[1, 4]), 16);
        assert_eq!(math("leftshift", &[1, 31]), i32::MIN);
        assert_eq!(math("leftshift", &[1, 32]), 0);
        assert_eq!(math("rightshift", &[-1, 28]), 0xF, "a logical shift");
        assert_eq!(math("rightshift", &[16, -1]), 0, "-1 is a shift of 2^32 - 1");
        assert_eq!(math("logicaland", &[0b1100, 0b1010]), 0b1000);
        assert_eq!(math("logicalor", &[0b1100, 0b1010]), 0b1110);
        assert_eq!(math("logicalxor", &[0b1100, 0b1010]), 0b0110);
        assert_eq!(math("logicalnot", &[0]), -1);
        let log = call("math", "log", &[Value::Float(std::f32::consts::E)]).number().unwrap();
        assert!((log - 1.0).abs() < 1e-6);
        let exp = call("math", "exp", &[Value::Float(1.0)]).number().unwrap();
        assert!((exp - std::f32::consts::E).abs() < 1e-6);
    }

    #[test]
    fn arrays_are_created_and_resized_within_the_bound() {
        assert_eq!(items(call("utility", "createintarray", &[Value::Int(3)])), ["0", "0", "0"]);
        assert_eq!(items(call("utility", "createstringarray", &[Value::Int(2), s("x")])), ["x", "x"]);
        assert_eq!(items(call("utility", "createformarray", &[Value::Int(1)])), ["None"]);
        let source = array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(items(call("utility", "resizeintarray", &[source.clone(), Value::Int(2)])), ["1", "2"]);
        assert_eq!(items(call("utility", "resizeintarray", &[source.clone(), Value::Int(5), Value::Int(9)])), ["1", "2", "3", "9", "9"]);
        assert_eq!(items(call("utility", "resizeintarray", &[Value::None, Value::Int(1)])), ["0"]);
        if let Value::Array(items) = &source {
            assert_eq!(items.borrow().len(), 3, "a resize returns a new array");
        }
        let over = MAX_ARRAY_ELEMENTS as i32 + 1;
        assert!(skse_util("utility", "createintarray", &[Value::Int(over)]).is_err());
        assert!(skse_util("utility", "createintarray", &[Value::Int(-1)]).is_err(), "-1 wraps past any bound");
        assert!(skse_util("utility", "resizeboolarray", &[Value::None, Value::Int(over)]).is_err());
    }
}
