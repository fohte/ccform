use mlua::{Lua, Result as LuaResult, Table, Value as LuaValue};

/// Top-level keys recognized in a `ccform.lua` return value; anything else
/// triggers a warning in `partition_root`.
const RECOGNIZED_KEYS: [&str; 3] = ["settings", "mcpServers", "ccform"];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("ccform.lua must return a table, got {actual_type}.")]
    RootNotATable { actual_type: &'static str },
}

pub type Result<T> = std::result::Result<T, Error>;

/// The `settings` / `mcpServers` bodies extracted from a `ccform.lua` return
/// value, with the `ccform` meta key already stripped out. Left as Lua
/// tables here; conversion to `serde_json::Value` happens in a later layer.
#[derive(Debug)]
pub struct Desired {
    pub settings: Table,
    pub mcp_servers: Table,
}

/// ccform's own metadata, extracted from the `ccform` top-level key.
#[derive(Debug, PartialEq)]
pub struct Meta {
    pub auto_import: bool,
}

/// Ensures a `ccform.lua` return value is a table, as required for it to be
/// interpreted further.
pub fn validate_root(value: LuaValue) -> Result<Table> {
    match value {
        LuaValue::Table(table) => Ok(table),
        other => Err(Error::RootNotATable {
            actual_type: other.type_name(),
        }),
    }
}

/// Splits `root` into `settings` / `mcpServers` bodies and the `ccform` meta
/// table, warning to stderr about (and otherwise ignoring) any other
/// top-level key. Missing `settings` / `mcpServers` default to an empty
/// table; a missing `ccform` meta table defaults to `auto_import: true`.
pub fn partition_root(lua: &Lua, root: Table) -> LuaResult<(Desired, Meta)> {
    for key in unknown_keys(&root)? {
        eprintln!("warning: unknown top-level key '{key}' in ccform.lua return value; ignoring.");
    }

    let settings: Option<Table> = root.get("settings")?;
    let mcp_servers: Option<Table> = root.get("mcpServers")?;
    let auto_import = auto_import_enabled(&root)?;

    Ok((
        Desired {
            settings: settings.map_or_else(|| lua.create_table(), Ok)?,
            mcp_servers: mcp_servers.map_or_else(|| lua.create_table(), Ok)?,
        },
        Meta { auto_import },
    ))
}

/// Whether `root`'s `ccform` meta table permits `import.lua` to be
/// automatically merged in ahead of it: true unless `ccform.autoImport` is
/// present and explicitly `false`.
pub(crate) fn auto_import_enabled(root: &Table) -> LuaResult<bool> {
    let meta_table: Option<Table> = root.get("ccform")?;
    Ok(match &meta_table {
        Some(table) => table.get::<Option<bool>>("autoImport")?.unwrap_or(true),
        None => true,
    })
}

/// Returns the top-level keys of `root` outside `RECOGNIZED_KEYS`.
fn unknown_keys(root: &Table) -> LuaResult<Vec<String>> {
    let mut keys = Vec::new();
    for pair in root.pairs::<String, LuaValue>() {
        let (key, _) = pair?;
        if !RECOGNIZED_KEYS.contains(&key.as_str()) {
            keys.push(key);
        }
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use mlua::LuaSerdeExt;
    use rstest::{fixture, rstest};
    use serde_json::{Value as JsonValue, json};

    use super::*;

    #[fixture]
    fn lua() -> Lua {
        Lua::new()
    }

    fn to_json(lua: &Lua, table: Table) -> JsonValue {
        lua.from_value(LuaValue::Table(table)).unwrap()
    }

    #[rstest]
    #[case::nil("nil", "nil")]
    #[case::boolean("true", "boolean")]
    #[case::integer("42", "integer")]
    #[case::string("'hello'", "string")]
    fn test_validate_root_rejects_non_table_return_values(
        lua: Lua,
        #[case] literal: &str,
        #[case] expected_type: &str,
    ) {
        let value: LuaValue = lua.load(literal).eval().unwrap();

        let err = validate_root(value).unwrap_err();

        assert_eq!(
            err.to_string(),
            format!("ccform.lua must return a table, got {expected_type}.")
        );
    }

    #[rstest]
    fn test_validate_root_accepts_a_table(lua: Lua) {
        let value: LuaValue = lua.load("return {a = 1}").eval().unwrap();

        let table = validate_root(value).unwrap();

        assert_eq!(to_json(&lua, table), json!({"a": 1}));
    }

    #[rstest]
    fn test_unknown_keys_lists_top_level_keys_outside_the_recognized_set(lua: Lua) {
        let root: Table = lua
            .load("return {settings = {}, mcpServers = {}, ccform = {}, foo = 1, bar = 2}")
            .eval()
            .unwrap();

        let mut keys = unknown_keys(&root).unwrap();
        keys.sort();

        assert_eq!(keys, vec!["bar".to_string(), "foo".to_string()]);
    }

    fn to_json_partitioned(lua: &Lua, desired: Desired, meta: Meta) -> JsonValue {
        json!({
            "settings": to_json(lua, desired.settings),
            "mcp_servers": to_json(lua, desired.mcp_servers),
            "auto_import": meta.auto_import,
        })
    }

    #[rstest]
    #[case::extracts_settings_and_mcp_servers(
        "return {settings = {theme = 'dark'}, mcpServers = {foo = {command = 'bar'}}}",
        json!({"settings": {"theme": "dark"}, "mcp_servers": {"foo": {"command": "bar"}}, "auto_import": true})
    )]
    #[case::extracts_ccform_meta_and_strips_it_from_the_body(
        "return {settings = {a = 1}, ccform = {autoImport = false}}",
        json!({"settings": {"a": 1}, "mcp_servers": {}, "auto_import": false})
    )]
    #[case::ignores_unrecognized_top_level_keys_and_continues(
        "return {settings = {a = 1}, extra = 'should be ignored'}",
        json!({"settings": {"a": 1}, "mcp_servers": {}, "auto_import": true})
    )]
    #[case::defaults_missing_keys_to_empty(
        "return {}",
        json!({"settings": {}, "mcp_servers": {}, "auto_import": true})
    )]
    fn test_partition_root(lua: Lua, #[case] source: &str, #[case] expected: JsonValue) {
        let root: Table = lua.load(source).eval().unwrap();

        let (desired, meta) = partition_root(&lua, root).unwrap();

        assert_eq!(to_json_partitioned(&lua, desired, meta), expected);
    }
}
