//! Serializes a `serde_json::Value` as a `return { ... }` Lua source, for
//! files a human is expected to read and edit by hand (`ccform init`'s
//! bootstrap `ccform.lua` and `ccform import`'s generated `import.lua`).
use std::fmt::Write as _;

use serde_json::{Map, Value};

const INDENT: &str = "  ";

const LUA_RESERVED_WORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// Renders `value` as a `return { ... }` Lua source. When `header` is
/// `Some`, each of its lines is emitted as a `-- `-prefixed comment line
/// before `return`. The result always ends with a trailing newline.
pub fn to_lua_literal(value: &Value, header: Option<&str>) -> String {
    let mut out = String::new();
    if let Some(header) = header {
        for line in header.lines() {
            let _ = writeln!(out, "-- {line}");
        }
    }
    out.push_str("return ");
    write_value(&mut out, value, 0);
    out.push('\n');
    out
}

fn write_value(out: &mut String, value: &Value, depth: usize) {
    match value {
        Value::Null => out.push_str("nil"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            let _ = write!(out, "{n}");
        }
        Value::String(s) => write_string(out, s),
        Value::Array(items) => write_array(out, items, depth),
        Value::Object(map) => write_object(out, map, depth),
    }
}

/// Single-quotes `s`, escaping the characters that would otherwise break out
/// of the Lua string literal or corrupt its content: backslash, single
/// quote, and line breaks. An unescaped `\r` inside a Lua short string is as
/// fatal as an unescaped `\n` — the lexer treats it as an unterminated
/// string — so both are escaped.
fn write_string(out: &mut String, s: &str) {
    out.push('\'');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('\'');
}

fn write_array(out: &mut String, items: &[Value], depth: usize) {
    write_braced(out, depth, items.is_empty(), |out| {
        for item in items {
            out.push_str(&INDENT.repeat(depth + 1));
            write_value(out, item, depth + 1);
            out.push_str(",\n");
        }
    });
}

fn write_object(out: &mut String, map: &Map<String, Value>, depth: usize) {
    write_braced(out, depth, map.is_empty(), |out| {
        for (key, value) in map {
            out.push_str(&INDENT.repeat(depth + 1));
            write_key(out, key);
            out.push_str(" = ");
            write_value(out, value, depth + 1);
            out.push_str(",\n");
        }
    });
}

/// Shared shape behind `write_array`/`write_object`: an empty table
/// collapses to `{}`, otherwise `write_items` fills the body between an
/// opening `{\n` and a closing `}` indented back to `depth`.
fn write_braced(
    out: &mut String,
    depth: usize,
    is_empty: bool,
    write_items: impl FnOnce(&mut String),
) {
    if is_empty {
        out.push_str("{}");
        return;
    }

    out.push_str("{\n");
    write_items(out);
    out.push_str(&INDENT.repeat(depth));
    out.push('}');
}

/// Writes `key` bare (`foo = ...`) when it is a valid Lua identifier that
/// isn't a reserved word, and as an indexing expression (`["foo-bar"] =
/// ...`) otherwise.
fn write_key(out: &mut String, key: &str) {
    if is_lua_identifier(key) {
        out.push_str(key);
    } else {
        out.push('[');
        write_string(out, key);
        out.push(']');
    }
}

fn is_lua_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.clone().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !LUA_RESERVED_WORDS.contains(&s)
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use rstest::rstest;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_to_lua_literal_renders_nested_object() {
        let value = json!({
            "settings": {
                "permissions": {
                    "allow": ["Bash(brew:*)"],
                },
            },
        });

        assert_eq!(
            to_lua_literal(&value, None),
            indoc! {"
                return {
                  settings = {
                    permissions = {
                      allow = {
                        'Bash(brew:*)',
                      },
                    },
                  },
                }
            "}
        );
    }

    #[test]
    fn test_to_lua_literal_quotes_non_identifier_keys() {
        let value = json!({
            "foo-bar": 1,
            "1leading": 2,
            "end": 3,
            "valid_key": 4,
        });

        assert_eq!(
            to_lua_literal(&value, None),
            indoc! {"
                return {
                  ['foo-bar'] = 1,
                  ['1leading'] = 2,
                  ['end'] = 3,
                  valid_key = 4,
                }
            "}
        );
    }

    #[test]
    fn test_to_lua_literal_escapes_special_characters_in_strings() {
        let value = json!({"value": "back\\slash 'quote'\nnewline\rcarriage return"});

        assert_eq!(
            to_lua_literal(&value, None),
            indoc! {r"
                return {
                  value = 'back\\slash \'quote\'\nnewline\rcarriage return',
                }
            "}
        );
    }

    #[test]
    fn test_to_lua_literal_renders_scalars_and_empty_containers() {
        let value = json!({
            "number": 42,
            "float": 3.5,
            "bool_true": true,
            "bool_false": false,
            "null": null,
            "empty_array": [],
            "empty_object": {},
        });

        assert_eq!(
            to_lua_literal(&value, None),
            indoc! {"
                return {
                  number = 42,
                  float = 3.5,
                  bool_true = true,
                  bool_false = false,
                  null = nil,
                  empty_array = {},
                  empty_object = {},
                }
            "}
        );
    }

    #[test]
    fn test_to_lua_literal_renders_array_of_scalars() {
        let value = json!([1, 2, 3]);

        assert_eq!(
            to_lua_literal(&value, None),
            indoc! {"
                return {
                  1,
                  2,
                  3,
                }
            "}
        );
    }

    #[rstest]
    #[case::without_header(
        None,
        indoc! {"
            return {
              a = 1,
            }
        "}
    )]
    #[case::with_header(
        Some("AUTO-GENERATED by 'ccform import'. Edit at your own risk.\nPromote entries to base.lua / profiles/ and re-run 'ccform import' to clean up."),
        indoc! {"
            -- AUTO-GENERATED by 'ccform import'. Edit at your own risk.
            -- Promote entries to base.lua / profiles/ and re-run 'ccform import' to clean up.
            return {
              a = 1,
            }
        "}
    )]
    fn test_to_lua_literal_header_handling(#[case] header: Option<&str>, #[case] expected: &str) {
        let value = json!({"a": 1});

        assert_eq!(to_lua_literal(&value, header), expected);
    }
}
