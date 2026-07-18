use mlua::{Lua, Result as LuaResult, Table, Value as LuaValue, Variadic};

use crate::config::lua::Replace;

/// Deep-merges any number of Lua values left-to-right: maps merge recursively
/// (matching keys are merged again, with the right side winning on scalar
/// conflicts), 1-indexed sequential-integer tables ("arrays") concatenate,
/// and anything else is replaced outright by the right side. A value wrapped
/// by `ccform.replace` fully replaces whatever occupies its position instead
/// of being merged or concatenated, no matter how deep it is nested. Zero
/// arguments yields an empty table; the result never contains a `Replace`
/// marker, however deep the input nesting. Traverses table contents
/// recursively with no cycle detection, so a self-referential table (e.g.
/// `t.self = t`) overflows the stack rather than returning an error — the
/// same trade-off a `ccform.lua` author accepts by writing an infinite Lua
/// loop in their own config.
pub fn deep_merge(lua: &Lua, values: Variadic<LuaValue>) -> LuaResult<LuaValue> {
    let mut acc: Option<LuaValue> = None;
    for value in values {
        acc = Some(merge_value(lua, acc, value)?);
    }
    match acc {
        Some(value) => Ok(value),
        None => Ok(LuaValue::Table(lua.create_table()?)),
    }
}

/// Merges `right` into `left` at a single position. `left` is assumed to
/// already be free of `Replace` markers (it is either `None` or the result of
/// a previous merge step); only `right` is inspected for one.
fn merge_value(lua: &Lua, left: Option<LuaValue>, right: LuaValue) -> LuaResult<LuaValue> {
    match (left, right) {
        (Some(LuaValue::Table(left_table)), LuaValue::Table(right_table))
            if is_array(&left_table)? && is_array(&right_table)? =>
        {
            concat_arrays(lua, &left_table, &right_table)
        }
        (Some(LuaValue::Table(left_table)), LuaValue::Table(right_table))
            if !is_array(&left_table)? && !is_array(&right_table)? =>
        {
            merge_maps(lua, &left_table, &right_table)
        }
        // Either there is nothing on the left, both sides are scalars, or the
        // two tables disagree on shape (one is an array, the other a map):
        // none of these have a sensible merge/concat, so `right` wins outright.
        (_, right) => resolve(lua, right),
    }
}

/// Rebuilds `value` with every `Replace` marker unwrapped, without merging it
/// against anything. Used both for values that have no counterpart on the
/// left and for the payload of a `Replace` marker itself.
fn resolve(lua: &Lua, value: LuaValue) -> LuaResult<LuaValue> {
    if let Some(inner) = unwrap_replace(&value)? {
        return resolve(lua, inner);
    }

    let LuaValue::Table(table) = value else {
        return Ok(value);
    };

    let result = lua.create_table()?;
    if is_array(&table)? {
        for (index, item) in table.sequence_values::<LuaValue>().enumerate() {
            result.raw_set(index + 1, resolve(lua, item?)?)?;
        }
    } else {
        for pair in table.pairs::<LuaValue, LuaValue>() {
            let (key, item) = pair?;
            result.raw_set(key, resolve(lua, item)?)?;
        }
    }
    Ok(LuaValue::Table(result))
}

/// Returns the unwrapped payload if `value` is a `ccform.replace` marker.
fn unwrap_replace(value: &LuaValue) -> LuaResult<Option<LuaValue>> {
    let LuaValue::UserData(userdata) = value else {
        return Ok(None);
    };
    if !userdata.is::<Replace>() {
        return Ok(None);
    }
    Ok(Some(userdata.borrow::<Replace>()?.0.clone()))
}

/// Recursively merges two map-shaped tables, key by key, with `right` values
/// winning on conflicts.
fn merge_maps(lua: &Lua, left: &Table, right: &Table) -> LuaResult<LuaValue> {
    let result = lua.create_table()?;
    for pair in left.pairs::<LuaValue, LuaValue>() {
        let (key, value) = pair?;
        result.raw_set(key, value)?;
    }
    for pair in right.pairs::<LuaValue, LuaValue>() {
        let (key, right_value) = pair?;
        let existing: LuaValue = result.raw_get(key.clone())?;
        let left_value = match existing {
            LuaValue::Nil => None,
            existing => Some(existing),
        };
        let merged = merge_value(lua, left_value, right_value)?;
        result.raw_set(key, merged)?;
    }
    Ok(LuaValue::Table(result))
}

/// Concatenates two array-shaped tables: every element of `left` followed by
/// every element of `right` (resolved, since elements may carry `Replace`
/// markers of their own).
fn concat_arrays(lua: &Lua, left: &Table, right: &Table) -> LuaResult<LuaValue> {
    let result = lua.create_table()?;
    let mut index = 0usize;
    for value in left.sequence_values::<LuaValue>() {
        index += 1;
        result.raw_set(index, value?)?;
    }
    for value in right.sequence_values::<LuaValue>() {
        index += 1;
        result.raw_set(index, resolve(lua, value?)?)?;
    }
    Ok(LuaValue::Table(result))
}

/// A table is an array when it has a positive length and every key is a
/// sequential integer from `1` to that length; an empty table is treated as
/// a map, since there is no way to distinguish it from an empty array.
fn is_array(table: &Table) -> LuaResult<bool> {
    let len = table.raw_len();
    if len == 0 {
        return Ok(false);
    }

    let mut count = 0usize;
    for pair in table.pairs::<LuaValue, LuaValue>() {
        let (key, _) = pair?;
        match key {
            LuaValue::Integer(i) if i >= 1 && (i as usize) <= len => count += 1,
            _ => return Ok(false),
        }
    }
    Ok(count == len)
}

#[cfg(test)]
mod tests {
    use mlua::LuaSerdeExt;
    use rstest::{fixture, rstest};
    use serde_json::{Value as JsonValue, json};

    use super::*;

    #[fixture]
    fn lua() -> Lua {
        let lua = Lua::new();
        let ccform = lua.create_table().unwrap();
        ccform
            .set(
                "replace",
                lua.create_function(|lua, value: LuaValue| lua.create_any_userdata(Replace(value)))
                    .unwrap(),
            )
            .unwrap();
        lua.globals().set("ccform", ccform).unwrap();
        lua
    }

    fn to_json(lua: &Lua, value: LuaValue) -> JsonValue {
        lua.from_value(value).unwrap()
    }

    /// Panics if `value` contains a `Replace` marker anywhere in its structure.
    fn assert_no_replace_marker(value: &LuaValue) {
        match value {
            LuaValue::UserData(_) => panic!("expected no Replace marker to remain in merge output"),
            LuaValue::Table(table) => {
                for pair in table.pairs::<LuaValue, LuaValue>() {
                    let (_, v) = pair.unwrap();
                    assert_no_replace_marker(&v);
                }
            }
            _ => {}
        }
    }

    #[rstest]
    #[case::merges_maps_with_right_precedence(
        "{a = 1, b = {x = 1, y = 2}}",
        "{b = {y = 3, z = 4}, c = 5}",
        json!({"a": 1, "b": {"x": 1, "y": 3, "z": 4}, "c": 5})
    )]
    #[case::concatenates_sequential_integer_arrays(
        "{1, 2, 3}",
        "{4, 5}",
        json!([1, 2, 3, 4, 5])
    )]
    #[case::replaces_scalars_with_the_right_side(
        "{value = 1}",
        "{value = 'two'}",
        json!({"value": "two"})
    )]
    #[case::replaces_outright_on_array_map_shape_mismatch(
        "{1, 2, 3}",
        "{x = 'override'}",
        json!({"x": "override"})
    )]
    fn test_deep_merge_default_semantics(
        lua: Lua,
        #[case] left_src: &str,
        #[case] right_src: &str,
        #[case] expected: JsonValue,
    ) {
        let left: LuaValue = lua.load(left_src).eval().unwrap();
        let right: LuaValue = lua.load(right_src).eval().unwrap();

        let result = deep_merge(&lua, Variadic::from_iter([left, right])).unwrap();

        assert_eq!(to_json(&lua, result), expected);
    }

    #[rstest]
    #[case::top_level("{a = 1}", "ccform.replace({b = 2})", json!({"b": 2}))]
    #[case::nested_in_map(
        "{a = {x = 1, y = 2}}",
        "{a = ccform.replace({z = 3})}",
        json!({"a": {"z": 3}})
    )]
    #[case::array_element(
        "{1, 2, 3}",
        "{ccform.replace(9), 5}",
        json!([1, 2, 3, 9, 5])
    )]
    fn test_deep_merge_unwraps_replace_marker_at_any_position(
        lua: Lua,
        #[case] left_src: &str,
        #[case] right_src: &str,
        #[case] expected: JsonValue,
    ) {
        let left: LuaValue = lua.load(left_src).eval().unwrap();
        let right: LuaValue = lua.load(right_src).eval().unwrap();

        let result = deep_merge(&lua, Variadic::from_iter([left, right])).unwrap();

        assert_eq!(to_json(&lua, result), expected);
    }

    #[rstest]
    fn test_deep_merge_leaves_no_replace_marker_after_merging(lua: Lua) {
        let left: LuaValue = lua.load("{a = 1, b = {1, 2}, c = {x = 1}}").eval().unwrap();
        let right_src = "{a = ccform.replace(10), b = {ccform.replace(20), 30}, c = ccform.replace({y = ccform.replace(2)})}";
        let right: LuaValue = lua.load(right_src).eval().unwrap();

        let result = deep_merge(&lua, Variadic::from_iter([left, right])).unwrap();

        assert_no_replace_marker(&result);
        assert_eq!(
            to_json(&lua, result),
            json!({"a": 10, "b": [1, 2, 20, 30], "c": {"y": 2}}),
        );
    }

    #[rstest]
    fn test_deep_merge_with_no_arguments_returns_an_empty_table(lua: Lua) {
        let result = deep_merge(&lua, Variadic::new()).unwrap();

        assert_eq!(to_json(&lua, result), json!({}));
    }

    #[rstest]
    fn test_deep_merge_with_a_single_argument_returns_it_unchanged(lua: Lua) {
        let value: LuaValue = lua.load("{a = 1, b = {2, 3}}").eval().unwrap();

        let result = deep_merge(&lua, Variadic::from_iter([value])).unwrap();

        assert_eq!(to_json(&lua, result), json!({"a": 1, "b": [2, 3]}));
    }
}
