use std::fs;
use std::path::Path;

use mlua::{Lua, Result as LuaResult, Table, Value as LuaValue, Variadic};

use crate::config::loader;
use crate::config::merge::deep_merge;

/// A value wrapped by `ccform.replace`, marking it to fully replace the
/// corresponding position during `ccform.merge` instead of being deep-merged.
/// Opaque from Lua; only unwrapped by the merge implementation.
pub struct Replace(pub LuaValue);

/// Hosts the Lua 5.4 VM used to evaluate a user's `ccform.lua` DSL.
#[derive(Debug)]
pub struct Runtime {
    lua: Lua,
}

impl Runtime {
    /// Creates a Lua VM with the `ccform` global registered (including
    /// `ccform.env`, which reads process environment variables;
    /// `ccform.replace`, which wraps a value as a full-replace marker for
    /// `ccform.merge`; and `ccform.merge`, which deep-merges any number of
    /// tables) and `require` wired up to resolve modules under `config_dir`
    /// (as `?.lua` and `?/init.lua`), ahead of the default `package.path`
    /// entries.
    pub fn new(config_dir: &Path) -> LuaResult<Self> {
        let config_dir = config_dir
            .to_str()
            .ok_or_else(|| mlua::Error::runtime("config_dir must be valid UTF-8"))?;
        if config_dir.contains(';') || config_dir.contains('?') {
            // `;` separates package.path templates and `?` is the placeholder substituted
            // with the module name (Lua 5.4 manual §6.3), so either character in the
            // directory name would corrupt path resolution.
            return Err(mlua::Error::runtime(
                "config_dir must not contain ';' or '?' (Lua package.path special characters)",
            ));
        }

        let lua = Lua::new();

        let ccform = lua.create_table()?;
        ccform.set(
            "env",
            // `.ok()` also maps a non-UTF-8 value (VarError::NotUnicode) to `nil`,
            // indistinguishable from an unset variable.
            lua.create_function(|_, name: String| Ok(std::env::var(name).ok()))?,
        )?;
        ccform.set(
            "replace",
            lua.create_function(|lua, value: LuaValue| lua.create_any_userdata(Replace(value)))?,
        )?;
        ccform.set("merge", lua.create_function(deep_merge)?)?;
        lua.globals().set("ccform", ccform)?;

        let package: Table = lua.globals().get("package")?;
        let default_path: String = package.get("path")?;
        package.set(
            "path",
            format!("{config_dir}/?.lua;{config_dir}/?/init.lua;{default_path}"),
        )?;

        Ok(Self { lua })
    }

    /// Reads `path` and evaluates it as a Lua chunk, returning its return
    /// value as-is (no validation of its shape). Lua syntax and runtime
    /// errors propagate as `mlua::Error`, named after `path` so any position
    /// info in the error refers to it.
    pub fn load_entry(&self, path: &Path) -> LuaResult<LuaValue> {
        let source = fs::read_to_string(path).map_err(|err| {
            mlua::Error::runtime(format!("failed to read {}: {err}", path.display()))
        })?;
        self.lua
            .load(source)
            // The `@` prefix marks the chunk name as a file path (mlua's own
            // `AsChunk for &Path` impl does the same), so Lua reports error
            // positions as `path:line:` instead of `[string "path"]:line:`.
            .set_name(format!("@{}", path.display()))
            .eval()
    }

    /// If `base`'s `ccform.autoImport` meta field is not explicitly `false`
    /// and `import_path` names an existing file, evaluates it and
    /// deep-merges its return value into `base` — the `import_path` content
    /// on the left (weaker) side, `base` on the right (stronger) side, so
    /// `base` wins any conflict — and returns the merged table. Otherwise
    /// returns `base` unchanged. A Lua syntax or runtime error while
    /// evaluating `import_path` propagates the same way `load_entry`
    /// propagates one; a return value that isn't a table is also an error,
    /// the same way `validate_root` rejects one for `ccform.lua`.
    pub fn maybe_apply_import_overlay(&self, base: Table, import_path: &Path) -> LuaResult<Table> {
        if !loader::auto_import_enabled(&base)? || !import_path.is_file() {
            return Ok(base);
        }

        let import_value = self.load_entry(import_path)?;
        let LuaValue::Table(import_table) = import_value else {
            return Err(mlua::Error::runtime(format!(
                "{} must return a table, got {}.",
                import_path.display(),
                import_value.type_name()
            )));
        };
        let merged = deep_merge(
            &self.lua,
            Variadic::from_iter([LuaValue::Table(import_table), LuaValue::Table(base)]),
        )?;

        let LuaValue::Table(merged) = merged else {
            unreachable!(
                "both deep_merge operands are Table (import_table and base), so the result is always a Table"
            );
        };
        Ok(merged)
    }
}

#[cfg(test)]
mod tests {
    use mlua::LuaSerdeExt;
    use rstest::{fixture, rstest};
    use serde_json::{Value as JsonValue, json};
    use tempfile::TempDir;

    use super::*;

    #[fixture]
    fn config_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[fixture]
    fn runtime(config_dir: TempDir) -> Runtime {
        Runtime::new(config_dir.path()).unwrap()
    }

    fn to_json(lua: &Lua, table: Table) -> JsonValue {
        lua.from_value(LuaValue::Table(table)).unwrap()
    }

    #[rstest]
    fn test_ccform_global_is_accessible_without_require(runtime: Runtime) {
        let ccform: mlua::Table = runtime.lua.load("return ccform").eval().unwrap();
        let mut keys: Vec<String> = ccform
            .pairs::<String, mlua::Value>()
            .map(|pair| pair.unwrap().0)
            .collect();
        keys.sort();

        assert_eq!(
            keys,
            vec![
                "env".to_string(),
                "merge".to_string(),
                "replace".to_string()
            ]
        );
    }

    #[rstest]
    #[case::defined("CARGO_PKG_NAME", Some(env!("CARGO_PKG_NAME")))]
    #[case::undefined("CCFORM_TEST_ENV_UNDEFINED_VAR", None)]
    fn test_env_reflects_process_environment(
        runtime: Runtime,
        #[case] var_name: &str,
        #[case] expected: Option<&str>,
    ) {
        let result: Option<String> = runtime
            .lua
            .load(format!("return ccform.env('{var_name}')"))
            .eval()
            .unwrap();

        assert_eq!(result, expected.map(String::from));
    }

    #[rstest]
    #[case::number("42")]
    #[case::string("'hello'")]
    #[case::boolean("true")]
    fn test_replace_preserves_wrapped_scalar(runtime: Runtime, #[case] literal: &str) {
        let userdata: mlua::AnyUserData = runtime
            .lua
            .load(format!("return ccform.replace({literal})"))
            .eval()
            .unwrap();
        let expected: mlua::Value = runtime.lua.load(literal).eval().unwrap();

        assert_eq!(userdata.borrow::<Replace>().unwrap().0, expected);
    }

    #[rstest]
    fn test_replace_preserves_wrapped_table_contents(runtime: Runtime) {
        let userdata: mlua::AnyUserData = runtime
            .lua
            .load("return ccform.replace({1, 2, 3})")
            .eval()
            .unwrap();
        let wrapped = userdata.borrow::<Replace>().unwrap();
        let table = match &wrapped.0 {
            LuaValue::Table(table) => table,
            other => panic!("expected LuaValue::Table, got {other:?}"),
        };
        let values: Vec<i64> = table.sequence_values().map(|v| v.unwrap()).collect();

        assert_eq!(values, vec![1, 2, 3]);
    }

    #[rstest]
    fn test_merge_is_reachable_via_the_ccform_global(runtime: Runtime) {
        let result: mlua::Value = runtime
            .lua
            .load("return ccform.merge({a = 1}, {b = 2})")
            .eval()
            .unwrap();
        let json: serde_json::Value = runtime.lua.from_value(result).unwrap();

        assert_eq!(json, serde_json::json!({"a": 1, "b": 2}));
    }

    #[rstest]
    #[case::single_file("foo.lua", "foo")]
    #[case::package_init("subpkg/init.lua", "subpkg")]
    fn test_require_loads_module_from_config_dir(
        config_dir: TempDir,
        #[case] relative_path: &str,
        #[case] module_name: &str,
    ) {
        let module_path = config_dir.path().join(relative_path);
        if let Some(parent) = module_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&module_path, "return 42").unwrap();

        let runtime = Runtime::new(config_dir.path()).unwrap();
        let result: i64 = runtime
            .lua
            .load(format!("return require('{module_name}')"))
            .eval()
            .unwrap();

        assert_eq!(result, 42);
    }

    #[rstest]
    #[case::semicolon(';')]
    #[case::question_mark('?')]
    fn test_new_rejects_special_characters_in_config_dir(
        config_dir: TempDir,
        #[case] special_char: char,
    ) {
        let config_dir = config_dir.path().join(format!("foo{special_char}bar"));

        let err = Runtime::new(&config_dir).unwrap_err();

        assert_eq!(
            err.to_string(),
            "runtime error: config_dir must not contain ';' or '?' (Lua package.path special characters)"
        );
    }

    #[rstest]
    fn test_load_entry_returns_the_chunks_return_value(config_dir: TempDir) {
        let entry_path = config_dir.path().join("ccform.lua");
        std::fs::write(&entry_path, "return { a = 1 }").unwrap();
        let runtime = Runtime::new(config_dir.path()).unwrap();

        let value = runtime.load_entry(&entry_path).unwrap();

        let json: serde_json::Value = runtime.lua.from_value(value).unwrap();
        assert_eq!(json, serde_json::json!({"a": 1}));
    }

    #[rstest]
    fn test_load_entry_propagates_lua_syntax_errors(config_dir: TempDir) {
        let entry_path = config_dir.path().join("ccform.lua");
        std::fs::write(&entry_path, "return {").unwrap();
        let runtime = Runtime::new(config_dir.path()).unwrap();

        let err = runtime.load_entry(&entry_path).unwrap_err();

        assert!(matches!(err, mlua::Error::SyntaxError { .. }));
    }

    #[rstest]
    fn test_load_entry_reports_a_missing_file(config_dir: TempDir) {
        let entry_path = config_dir.path().join("ccform.lua");
        let runtime = Runtime::new(config_dir.path()).unwrap();

        let err = runtime.load_entry(&entry_path).unwrap_err();

        assert!(matches!(err, mlua::Error::RuntimeError(_)));
    }

    #[rstest]
    #[case::import_lua_absent(
        None,
        "{settings = {theme = 'light'}}",
        json!({"settings": {"theme": "light"}})
    )]
    #[case::auto_import_unspecified(
        Some("return {settings = {theme = 'dark', imported = true}}"),
        "{settings = {theme = 'light'}}",
        json!({"settings": {"theme": "light", "imported": true}})
    )]
    #[case::auto_import_explicitly_true(
        Some("return {settings = {theme = 'dark', imported = true}}"),
        "{settings = {theme = 'light'}, ccform = {autoImport = true}}",
        json!({"settings": {"theme": "light", "imported": true}, "ccform": {"autoImport": true}})
    )]
    #[case::auto_import_explicitly_false(
        Some("return {settings = {theme = 'dark', imported = true}}"),
        "{settings = {theme = 'light'}, ccform = {autoImport = false}}",
        json!({"settings": {"theme": "light"}, "ccform": {"autoImport": false}})
    )]
    fn test_maybe_apply_import_overlay(
        config_dir: TempDir,
        #[case] import_lua_src: Option<&str>,
        #[case] base_src: &str,
        #[case] expected: JsonValue,
    ) {
        let import_path = config_dir.path().join("import.lua");
        if let Some(src) = import_lua_src {
            std::fs::write(&import_path, src).unwrap();
        }
        let runtime = Runtime::new(config_dir.path()).unwrap();
        let base: Table = runtime
            .lua
            .load(format!("return {base_src}"))
            .eval()
            .unwrap();

        let result = runtime
            .maybe_apply_import_overlay(base, &import_path)
            .unwrap();

        assert_eq!(to_json(&runtime.lua, result), expected);
    }

    #[rstest]
    fn test_maybe_apply_import_overlay_propagates_import_lua_errors(config_dir: TempDir) {
        let import_path = config_dir.path().join("import.lua");
        std::fs::write(&import_path, "error('boom')").unwrap();
        let runtime = Runtime::new(config_dir.path()).unwrap();
        let base: Table = runtime.lua.load("return {}").eval().unwrap();

        let overlay_err = runtime
            .maybe_apply_import_overlay(base, &import_path)
            .unwrap_err();
        let direct_err = runtime.load_entry(&import_path).unwrap_err();

        // `maybe_apply_import_overlay` propagates whatever `load_entry` produces for
        // `import.lua` as-is, stack traceback included, rather than wrapping or
        // summarizing it.
        assert_eq!(overlay_err.to_string(), direct_err.to_string());
    }

    #[rstest]
    fn test_maybe_apply_import_overlay_rejects_a_non_table_import_lua_return_value(
        config_dir: TempDir,
    ) {
        let import_path = config_dir.path().join("import.lua");
        std::fs::write(&import_path, "return 42").unwrap();
        let runtime = Runtime::new(config_dir.path()).unwrap();
        let base: Table = runtime.lua.load("return {}").eval().unwrap();

        let err = runtime
            .maybe_apply_import_overlay(base, &import_path)
            .unwrap_err();

        assert_eq!(
            err.to_string(),
            format!(
                "runtime error: {} must return a table, got integer.",
                import_path.display()
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_new_rejects_non_utf8_config_dir() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let config_dir = Path::new(OsStr::from_bytes(&[0x66, 0x6f, 0x80, 0x6f]));

        let err = Runtime::new(config_dir).unwrap_err();

        assert_eq!(
            err.to_string(),
            "runtime error: config_dir must be valid UTF-8"
        );
    }
}
