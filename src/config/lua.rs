use std::path::Path;

use mlua::{Lua, Result as LuaResult, Table};

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
    /// Creates a Lua VM with the `ccform` global registered and `require`
    /// wired up to resolve modules under `config_dir` (as `?.lua` and
    /// `?/init.lua`), ahead of the default `package.path` entries.
    pub fn new(config_dir: &Path) -> LuaResult<Self> {
        let config_dir = config_dir
            .to_str()
            .ok_or_else(|| mlua::Error::runtime("config_dir must be valid UTF-8"))?;
        if config_dir.contains(';') {
            // `;` is the package.path template separator (Lua 5.4 manual §6.3),
            // so a literal `;` in the directory name would corrupt the path.
            return Err(mlua::Error::runtime(
                "config_dir must not contain ';' (Lua package.path separator)",
            ));
        }

        let lua = Lua::new();

        lua.globals().set("ccform", lua.create_table()?)?;

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
    use mlua::LuaSerdeExt;
    use rstest::{fixture, rstest};
    use tempfile::TempDir;

    use super::*;

    #[fixture]
    fn config_dir() -> TempDir {
        tempfile::tempdir().unwrap()
    }

    #[rstest]
    fn test_ccform_global_is_accessible_without_require(config_dir: TempDir) {
        let runtime = Runtime::new(config_dir.path()).unwrap();

        let ccform: mlua::Value = runtime.lua.load("return ccform").eval().unwrap();
        let result: serde_json::Value = runtime.lua.from_value(ccform).unwrap();

        assert_eq!(result, serde_json::json!({}));
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
    fn test_new_rejects_semicolon_in_config_dir(config_dir: TempDir) {
        let config_dir = config_dir.path().join("foo;bar");

        let err = Runtime::new(&config_dir).unwrap_err();

        assert_eq!(
            err.to_string(),
            "runtime error: config_dir must not contain ';' (Lua package.path separator)"
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
