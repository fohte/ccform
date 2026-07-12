use std::path::Path;

use mlua::{Lua, Result as LuaResult, Table};

/// Hosts the Lua 5.4 VM used to evaluate a user's `ccform.lua` DSL.
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
        let lua = Lua::new();

        lua.globals().set("ccform", lua.create_table()?)?;

        let package: Table = lua.globals().get("package")?;
        let default_path: String = package.get("path")?;
        let config_dir = config_dir.display();
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
}
