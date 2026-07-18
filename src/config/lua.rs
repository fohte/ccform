use std::path::Path;

use mlua::{Lua, Result as LuaResult, Table, Value as LuaValue};

/// A value wrapped by `ccform.replace`, marking it to fully replace the
/// corresponding position during `ccform.merge` instead of being deep-merged.
/// Opaque from Lua; only unwrapped by the merge implementation.
pub struct Replace(pub LuaValue);

/// Hosts the Lua 5.4 VM used to evaluate a user's `ccform.lua` DSL.
#[derive(Debug)]
pub struct Runtime {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "consumed by Runtime::load_entry")
    )]
    lua: Lua,
}

impl Runtime {
    /// Creates a Lua VM with the `ccform` global registered (including
    /// `ccform.env`, which reads process environment variables, and
    /// `ccform.replace`, which wraps a value as a full-replace marker for
    /// `ccform.merge`) and `require` wired up to resolve modules under
    /// `config_dir` (as `?.lua` and `?/init.lua`), ahead of the default
    /// `package.path` entries.
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
        lua.globals().set("ccform", ccform)?;

        let package: Table = lua.globals().get("package")?;
        let default_path: String = package.get("path")?;
        package.set(
            "path",
            format!("{config_dir}/?.lua;{config_dir}/?/init.lua;{default_path}"),
        )?;

        Ok(Self { lua })
    }
}

#[cfg(test)]
mod tests {
    use rstest::{fixture, rstest};
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

    #[rstest]
    fn test_ccform_global_is_accessible_without_require(runtime: Runtime) {
        let ccform: mlua::Table = runtime.lua.load("return ccform").eval().unwrap();
        let mut keys: Vec<String> = ccform
            .pairs::<String, mlua::Value>()
            .map(|pair| pair.unwrap().0)
            .collect();
        keys.sort();

        assert_eq!(keys, vec!["env".to_string(), "replace".to_string()]);
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
