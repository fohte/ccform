//! `ccform plan`: evaluates `ccform.lua` (auto-merging `import.lua` ahead of
//! it), reads the current `~/.claude/settings.json` and the `mcpServers` key
//! of `~/.claude.json`, loads `state.json`, and renders the 3-way diff
//! between them. Writes nothing.
//!
//! [`compute`] does everything up to the diff and is `pub(crate)` so
//! `ccform apply` can reuse it before prompting and writing.

use mlua::Value as LuaValue;
use serde_json::Value;

use crate::cli::render;
use crate::config::loader;
use crate::config::lua::Runtime;
use crate::config::lua_json::lua_to_json;
use crate::paths;
use crate::state::diff::{self, DiffReport};
use crate::state::store;
use crate::target::mcp_servers::McpServers;
use crate::target::settings::Settings;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Lua(#[from] mlua::Error),

    #[error(transparent)]
    Loader(#[from] loader::Error),

    #[error(transparent)]
    Settings(#[from] crate::target::settings::Error),

    #[error(transparent)]
    McpServers(#[from] crate::target::mcp_servers::Error),

    #[error(transparent)]
    State(#[from] store::Error),

    #[error(transparent)]
    Render(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// The desired configuration evaluated from `ccform.lua`, and the diff report
/// computed against actual/state for each of `settings` / `mcpServers`.
pub struct Plan {
    pub desired_settings: Value,
    pub desired_mcp_servers: Value,
    pub settings_report: DiffReport,
    pub mcp_servers_report: DiffReport,
}

/// Prints the plan to stdout: structured text, or JSON when `json` is `true`.
pub fn run(json: bool) -> Result<()> {
    let plan = compute()?;
    let reports: [(&str, &DiffReport); 2] = [
        ("settings", &plan.settings_report),
        ("mcpServers", &plan.mcp_servers_report),
    ];
    let output = render::render_diff(&reports, json)?;
    if json {
        println!("{output}");
    } else {
        print!("{output}");
    }
    Ok(())
}

/// Evaluates `ccform.lua`, reads actual settings/mcpServers and `state.json`,
/// and computes the 3-way diff for each. Performs no writes.
pub(crate) fn compute() -> Result<Plan> {
    let runtime = Runtime::new(&paths::config_dir())?;
    let root = loader::validate_root(runtime.load_entry(&paths::entry_path())?)?;
    let root = runtime.maybe_apply_import_overlay(root, &paths::import_path())?;
    let (desired, _meta) = loader::partition_root(runtime.lua(), root)?;
    let desired_settings = lua_to_json(&LuaValue::Table(desired.settings))?;
    let desired_mcp_servers = lua_to_json(&LuaValue::Table(desired.mcp_servers))?;

    let actual_settings = Settings::new(paths::settings_path()).read()?;
    let actual_mcp_servers = McpServers::from_home().read_or_empty()?;

    let state = store::load()?;

    let settings_report = diff::compute_diff(
        state.as_ref().map(|s| &s.settings),
        &actual_settings,
        &desired_settings,
    );
    let mcp_servers_report = diff::compute_diff(
        state.as_ref().map(|s| &s.mcp_servers),
        &actual_mcp_servers,
        &desired_mcp_servers,
    );

    Ok(Plan {
        desired_settings,
        desired_mcp_servers,
        settings_report,
        mcp_servers_report,
    })
}
