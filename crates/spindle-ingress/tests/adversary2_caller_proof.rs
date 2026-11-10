//! Adversary pass 2 on `story:http-caller-proves-actor` (wave 19 U2): `Caller::proven` refuses a
//! proof whose objects name a member twice (`unique_members`, a byte scanner run beside
//! serde_json). Attacked here for a scanner/parser disagreement: a duplicate let through, or a
//! valid proof refused, under escaped names, surrogate pairs, `\/`, strings full of `"`, `\\`
//! and structural characters, arbitrary nesting and whitespace, deep nesting and large inputs.

use std::time::{Duration, Instant};

use serde_json::{Map, Value};
use spindle_ingress::Caller;

/// A fixed-seed xorshift, so a red case names the seed and the iteration that made it.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }
}

const NAMES: [&str; 13] = [
    "a", "b", "actor", "caller", "\"", "\\", "/", "é", "😀", "a\"b", "x/y", "\n", "\\\"",
];
const TEXTS: [&str; 10] = [
    "actor",
    "\"actor\": {\"a\": 1, \"a\": 2}",
    "{",
    "}",
    "[",
    "]",
    ",",
    ":",
    "\\",
    "\\\"\\\\\"",
];

fn hex(rng: &mut Rng, unit: u32) -> String {
    let text = format!("{unit:04x}");
    if rng.chance(2) {
        text.to_uppercase()
    } else {
        text
    }
}

/// `text` as a JSON string literal, each character spelled one of the ways JSON allows.
fn literal(rng: &mut Rng, text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        let escaped = |rng: &mut Rng| {
            let mut units = [0u16; 2];
            c.encode_utf16(&mut units)
                .iter()
                .map(|&unit| format!("\\u{}", hex(rng, u32::from(unit))))
                .collect::<String>()
        };
        let spelled = match c {
            '"' => {
                if rng.chance(2) {
                    "\\\"".to_owned()
                } else {
                    escaped(rng)
                }
            }
            '\\' => {
                if rng.chance(2) {
                    "\\\\".to_owned()
                } else {
                    escaped(rng)
                }
            }
            '\n' => {
                if rng.chance(2) {
                    "\\n".to_owned()
                } else {
                    escaped(rng)
                }
            }
            '/' => match rng.below(3) {
                0 => "/".to_owned(),
                1 => "\\/".to_owned(),
                _ => escaped(rng),
            },
            _ => {
                if rng.chance(3) {
                    escaped(rng)
                } else {
                    c.to_string()
                }
            }
        };
        out.push_str(&spelled);
    }
    out.push('"');
    out
}

fn space(rng: &mut Rng) -> &'static str {
    [" ", "", "\t", "\n", "\r\n ", ""][rng.below(6)]
}

/// A random JSON value as text; `twice` is set where any object in it names a member twice.
fn value(rng: &mut Rng, depth: usize, twice: &mut bool) -> String {
    let kind = if depth == 0 { rng.below(4) } else { rng.below(7) };
    match kind {
        0 => {
            let text = TEXTS[rng.below(TEXTS.len())];
            literal(rng, text)
        }
        1 => ["0", "-1.5e3", "true", "false", "null"][rng.below(5)].to_owned(),
        2 => {
            let name = NAMES[rng.below(NAMES.len())];
            literal(rng, name)
        }
        3 => "\"\"".to_owned(),
        4 | 5 => object(rng, depth - 1, twice, &[]),
        _ => {
            let items = (0..rng.below(4))
                .map(|_| value(rng, depth - 1, twice))
                .collect::<Vec<_>>();
            format!("[{}{}]", space(rng), items.join(&format!(",{}", space(rng))))
        }
    }
}

/// A random object as text, holding `fixed` members first (shuffled in), each name drawn from
/// [`NAMES`]; `twice` is set where it names a member twice.
fn object(rng: &mut Rng, depth: usize, twice: &mut bool, fixed: &[(String, String)]) -> String {
    let mut members: Vec<(String, String)> = fixed.to_vec();
    for _ in 0..rng.below(5) {
        let name = NAMES[rng.below(NAMES.len())].to_owned();
        let text = value(rng, depth, twice);
        members.push((name, text));
    }
    for at in (1..members.len()).rev() {
        let other = rng.below(at + 1);
        members.swap(at, other);
    }
    let mut seen = std::collections::BTreeSet::new();
    for (name, _) in &members {
        if !seen.insert(name.clone()) {
            *twice = true;
        }
    }
    let spelled = members
        .iter()
        .map(|(name, text)| {
            format!(
                "{}{}{}:{}{}",
                space(rng),
                literal(rng, name),
                space(rng),
                space(rng),
                text
            )
        })
        .collect::<Vec<_>>();
    format!("{{{}{}}}", spelled.join(","), space(rng))
}

/// A proof `{"actor": <string>, "caller": {...}}` whose caller holds random attributes at any depth,
/// with, now and then, a second `actor` or `caller`; and whether any object in it names a member
/// twice.
fn proof(rng: &mut Rng) -> (String, bool, String) {
    let mut twice = false;
    let actor = TEXTS[rng.below(TEXTS.len())].to_owned();
    let mut fixed = vec![("actor".to_owned(), literal(rng, &actor))];
    if !rng.chance(4) {
        let caller = object(rng, 4, &mut twice, &[]);
        fixed.push(("caller".to_owned(), caller));
    }
    if rng.chance(6) {
        let again = literal(rng, "porch.visit.Auditor");
        fixed.push(("actor".to_owned(), again));
    }
    if rng.chance(8) {
        fixed.push(("caller".to_owned(), "{}".to_owned()));
    }
    let mut members = fixed.clone();
    for at in (1..members.len()).rev() {
        let other = rng.below(at + 1);
        members.swap(at, other);
    }
    let mut seen = std::collections::BTreeSet::new();
    for (name, _) in &members {
        if !seen.insert(name.clone()) {
            twice = true;
        }
    }
    let spelled = members
        .iter()
        .map(|(name, text)| format!("{}{}:{}", space(rng), literal(rng, name), text))
        .collect::<Vec<_>>();
    (
        format!("{}{{{}}}{}", space(rng), spelled.join(","), space(rng)),
        twice,
        actor,
    )
}

/// The property the fix claims, over generated proofs: a proof proves a caller exactly when no
/// object in it names a member twice, and then proves the actor and attributes it spells.
#[test]
fn a_proof_proves_a_caller_exactly_when_no_object_names_a_member_twice() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut duplicates = 0;
    let mut proofs = 0;
    for iteration in 0..20_000 {
        let (text, twice, actor) = proof(&mut rng);
        let parsed: Value = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("#{iteration} generated invalid JSON {text:?}: {error}"));
        let proven = Caller::proven(text.as_bytes());
        if twice {
            duplicates += 1;
            assert_eq!(
                proven, None,
                "#{iteration}: a proof naming a member twice proved a caller: {text}"
            );
        } else {
            proofs += 1;
            let attributes = match parsed.get("caller") {
                Some(Value::Object(attributes)) => attributes.clone(),
                _ => Map::new(),
            };
            assert_eq!(
                proven,
                Some(Caller { actor, attributes }),
                "#{iteration}: a valid proof was refused or misread: {text}"
            );
        }
    }
    assert!(duplicates > 1_000 && proofs > 1_000, "{duplicates} {proofs}");
}

/// Each spelling of `actor` JSON allows is the one member `actor`: none of them lets a second
/// `actor` through, whichever comes first.
#[test]
fn every_spelling_of_a_name_is_that_name() {
    let spellings = [
        r#""actor""#,
        r#""actor""#,
        r#""actor""#,
        r#""actor""#,
        r#""actor""#,
    ];
    for first in &spellings {
        for second in &spellings {
            let text =
                format!(r#"{{{first}: "porch.visit.Auditor", {second}: "porch.visit.Member"}}"#);
            let parsed: Value = serde_json::from_str(&text).expect("valid JSON");
            assert_eq!(parsed.as_object().map(Map::len), Some(1), "{text}");
            assert_eq!(Caller::proven(text.as_bytes()), None, "{text}");
        }
    }
    let slash = r#"{"actor": "A", "caller": {"x/y": 1, "x\/y": 2}}"#;
    assert_eq!(Caller::proven(slash.as_bytes()), None, "{slash}");
    let pair = r#"{"actor": "A", "caller": {"😀": 1, "😀": 2}}"#;
    assert_eq!(Caller::proven(pair.as_bytes()), None, "{pair}");
    let upper = r#"{"actor": "A", "caller": {"😀": 1, "😀": 2}}"#;
    assert_eq!(Caller::proven(upper.as_bytes()), None, "{upper}");
    let lone = r#"{"actor": "A", "caller": {"\ud83d": 1}}"#;
    assert_eq!(Caller::proven(lone.as_bytes()), None, "a lone surrogate: {lone}");
    // Unicode normalisation: NFC and NFD é are two members to JSON, not one.
    let forms = "{\"actor\": \"A\", \"caller\": {\"caf\u{e9}\": 1, \"cafe\u{301}\": 2}}";
    let proven = Caller::proven(forms.as_bytes()).expect("two distinct names prove a caller");
    assert_eq!(proven.attributes.len(), 2, "{forms}");
}

/// Nesting at serde_json's limit and far past it: the first proves a caller, the second none,
/// and neither overflows the stack.
#[test]
fn deep_nesting_is_answered_without_overflow() {
    let deep = |depth: usize| {
        format!(
            r#"{{"actor": "A", "caller": {{"x": {}1{}}}}}"#,
            "[".repeat(depth),
            "]".repeat(depth)
        )
    };
    assert!(Caller::proven(deep(100).as_bytes()).is_some(), "depth 100");
    assert_eq!(Caller::proven(deep(200_000).as_bytes()), None, "depth 200000");
    let objects = |depth: usize| {
        format!(
            r#"{{"actor": "A", "caller": {}1{}}}"#,
            r#"{"x": "#.repeat(depth),
            "}".repeat(depth)
        )
    };
    assert!(Caller::proven(objects(100).as_bytes()).is_some(), "object depth 100");
    assert_eq!(Caller::proven(objects(200_000).as_bytes()), None, "object depth 200000");
}

/// A proof the size of the largest head hyper reads (about 400 kB) is answered in linear-ish time,
/// with many members, long escaped names, and long runs of backslashes.
#[test]
fn large_proofs_are_answered_quickly() {
    let members = (0..20_000)
        .map(|n| format!(r#""k{n:05}": {n}"#))
        .collect::<Vec<_>>()
        .join(",");
    let many = format!(r#"{{"actor": "A", "caller": {{{members}}}}}"#);
    let again = format!(r#"{{"actor": "A", "caller": {{{members}, "k00000": 0}}}}"#);
    let long = "\\u0061".repeat(30_000);
    let escaped = format!(r#"{{"actor": "A", "caller": {{"{long}": 1, "{long}b": 2}}}}"#);
    let slashes = format!(r#"{{"actor": "{}", "caller": {{}}}}"#, "\\\\".repeat(150_000));
    let started = Instant::now();
    assert!(Caller::proven(many.as_bytes()).is_some());
    assert_eq!(Caller::proven(again.as_bytes()), None);
    assert!(Caller::proven(escaped.as_bytes()).is_some());
    assert!(Caller::proven(slashes.as_bytes()).is_some());
    let took = started.elapsed();
    assert!(took < Duration::from_secs(5), "took {took:?}");
}
