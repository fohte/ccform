//! Converts between Lua values and `serde_json::Value`, in both directions:
//! `lua_to_json` turns a `ccform.lua` return value into JSON so it can be
//! diffed against actual state, and `json_to_lua` turns JSON (e.g. an
//! existing `settings.json`) into a Lua table for `ccform init` and
//! `ccform import`.

use mlua::{Lua, Result as LuaResult, Table, Value as LuaValue};
use serde_json::{Map, Number, Value as JsonValue};

use crate::config::merge::is_array;

/// Converts a Lua value to JSON. `nil`, booleans, integers, finite floats,
/// and strings convert directly; a table converts to a JSON array when
/// `config::merge::is_array` considers it one (sequential integer keys
/// starting at `1`; an empty table is a map), and to a JSON object
/// otherwise. Any Lua value with no JSON representation — a non-finite
/// float (NaN/infinity), a table with a non-string key, or a function,
/// thread, userdata, etc. — returns an error rather than silently
/// dropping or coercing data.
pub fn lua_to_json(value: &LuaValue) -> LuaResult<JsonValue> {
    match value {
        LuaValue::Nil => Ok(JsonValue::Null),
        LuaValue::Boolean(b) => Ok(JsonValue::Bool(*b)),
        LuaValue::Integer(i) => Ok(JsonValue::Number((*i).into())),
        LuaValue::Number(n) => Number::from_f64(*n).map(JsonValue::Number).ok_or_else(|| {
            mlua::Error::runtime(format!("cannot convert non-finite Lua number {n} to JSON"))
        }),
        LuaValue::String(s) => Ok(JsonValue::String(s.to_str()?.to_string())),
        LuaValue::Table(table) => table_to_json(table),
        other => Err(mlua::Error::runtime(format!(
            "cannot convert Lua {} to JSON",
            other.type_name()
        ))),
    }
}

fn table_to_json(table: &Table) -> LuaResult<JsonValue> {
    if is_array(table)? {
        let mut items = Vec::new();
        for value in table.sequence_values::<LuaValue>() {
            items.push(lua_to_json(&value?)?);
        }
        Ok(JsonValue::Array(items))
    } else {
        let mut map = Map::new();
        for pair in table.pairs::<LuaValue, LuaValue>() {
            let (key, value) = pair?;
            let LuaValue::String(key) = key else {
                return Err(mlua::Error::runtime(format!(
                    "cannot convert Lua {} table key to a JSON object key: expected string",
                    key.type_name()
                )));
            };
            map.insert(key.to_str()?.to_string(), lua_to_json(&value)?);
        }
        Ok(JsonValue::Object(map))
    }
}

/// Converts JSON to a Lua value: the reverse of `lua_to_json`. A JSON array
/// becomes a table with sequential integer keys starting at `1`; a JSON
/// object becomes a table with string keys. `lua` is used to allocate the
/// resulting tables and strings.
pub fn json_to_lua(lua: &Lua, value: &JsonValue) -> LuaResult<LuaValue> {
    match value {
        JsonValue::Null => Ok(LuaValue::Nil),
        JsonValue::Bool(b) => Ok(LuaValue::Boolean(*b)),
        JsonValue::Number(n) => json_number_to_lua(n),
        JsonValue::String(s) => Ok(LuaValue::String(lua.create_string(s)?)),
        JsonValue::Array(items) => {
            let table = lua.create_table()?;
            for (index, item) in items.iter().enumerate() {
                table.raw_set(index + 1, json_to_lua(lua, item)?)?;
            }
            Ok(LuaValue::Table(table))
        }
        JsonValue::Object(map) => {
            let table = lua.create_table()?;
            for (key, item) in map {
                table.raw_set(key.as_str(), json_to_lua(lua, item)?)?;
            }
            Ok(LuaValue::Table(table))
        }
    }
}

fn json_number_to_lua(n: &Number) -> LuaResult<LuaValue> {
    if let Some(i) = n.as_i64() {
        return Ok(LuaValue::Integer(i));
    }
    match n.as_f64() {
        Some(f) => Ok(LuaValue::Number(f)),
        None => Err(mlua::Error::runtime(format!(
            "cannot convert JSON number {n} to Lua"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use rstest::{fixture, rstest};
    use serde_json::json;

    use super::*;

    #[fixture]
    fn lua() -> Lua {
        Lua::new()
    }

    #[rstest]
    #[case::nil("nil", json!(null))]
    #[case::boolean_true("true", json!(true))]
    #[case::boolean_false("false", json!(false))]
    #[case::integer("42", json!(42))]
    #[case::float("3.5", json!(3.5))]
    #[case::string("'hello'", json!("hello"))]
    #[case::empty_table_is_a_map("{}", json!({}))]
    #[case::map("{a = 1, b = 'x'}", json!({"a": 1, "b": "x"}))]
    #[case::array("{1, 2, 3}", json!([1, 2, 3]))]
    #[case::nested(
        "{a = 1, b = {2, 3}, c = {x = 'y'}}",
        json!({"a": 1, "b": [2, 3], "c": {"x": "y"}})
    )]
    fn test_lua_to_json_converts_lua_values_to_json(
        lua: Lua,
        #[case] literal: &str,
        #[case] expected: JsonValue,
    ) {
        let value: LuaValue = lua.load(literal).eval().unwrap();

        assert_eq!(lua_to_json(&value).unwrap(), expected);
    }

    #[rstest]
    #[case::non_finite_number(
        "0/0",
        "runtime error: cannot convert non-finite Lua number NaN to JSON"
    )]
    #[case::table_with_a_non_string_key(
        "{[1] = 'a', foo = 'bar'}",
        "runtime error: cannot convert Lua integer table key to a JSON object key: expected string"
    )]
    fn test_lua_to_json_rejects_values_with_no_json_representation(
        lua: Lua,
        #[case] literal: &str,
        #[case] expected_message: &str,
    ) {
        let value: LuaValue = lua.load(literal).eval().unwrap();

        let err = lua_to_json(&value).unwrap_err();

        assert_eq!(err.to_string(), expected_message);
    }

    #[rstest]
    #[case::string("'hello'")]
    #[case::integer("42")]
    #[case::boolean("true")]
    #[case::null("nil")]
    #[case::object("{a = 1, b = {2, 3}, c = {x = 'y'}}")]
    #[case::array("{1, 2, 3}")]
    fn test_round_trip_through_json_preserves_the_value(lua: Lua, #[case] literal: &str) {
        let original: LuaValue = lua.load(literal).eval().unwrap();
        let json = lua_to_json(&original).unwrap();

        let round_tripped = json_to_lua(&lua, &json).unwrap();

        assert_eq!(lua_to_json(&round_tripped).unwrap(), json);
    }
}
