use std::path::{Path, PathBuf};

use xdg::BaseDirectories;

const APP_NAME: &str = "ccform";

/// All absolute paths ccform resolves, bundled so callers only pay for one
/// `$HOME` / XDG lookup regardless of how many paths they end up using.
#[cfg_attr(test, derive(Debug, PartialEq))]
struct Resolved {
    config_dir: PathBuf,
    state_dir: PathBuf,
    entry_path: PathBuf,
    import_path: PathBuf,
    state_path: PathBuf,
    state_backup_path: PathBuf,
    settings_path: PathBuf,
    claude_json_path: PathBuf,
}

/// Pure path arithmetic over an already-resolved `BaseDirectories` and
/// `$HOME`, kept separate from env access so it can be unit tested without
/// touching process-global env vars (see `clippy.toml`'s `disallowed-methods`
/// ban on `std::env::set_var`/`remove_var`).
fn resolve(dirs: &BaseDirectories, home: &Path) -> Resolved {
    let config_dir = dirs.get_config_home().unwrap_or_default();
    let state_dir = dirs.get_state_home().unwrap_or_default();
    Resolved {
        entry_path: config_dir.join("ccform.lua"),
        import_path: config_dir.join("import.lua"),
        state_path: state_dir.join("state.json"),
        state_backup_path: state_dir.join("state.json.backup"),
        settings_path: home.join(".claude").join("settings.json"),
        claude_json_path: home.join(".claude.json"),
        config_dir,
        state_dir,
    }
}

fn resolved() -> Resolved {
    resolve(&BaseDirectories::with_prefix(APP_NAME), &home_dir())
}

fn home_dir() -> PathBuf {
    std::env::home_dir().unwrap_or_default()
}

/// $XDG_CONFIG_HOME/ccform, falling back to ~/.config/ccform.
pub fn config_dir() -> PathBuf {
    resolved().config_dir
}

/// $XDG_STATE_HOME/ccform, falling back to ~/.local/state/ccform.
pub fn state_dir() -> PathBuf {
    resolved().state_dir
}

pub fn entry_path() -> PathBuf {
    resolved().entry_path
}

pub fn import_path() -> PathBuf {
    resolved().import_path
}

pub fn state_path() -> PathBuf {
    resolved().state_path
}

pub fn state_backup_path() -> PathBuf {
    resolved().state_backup_path
}

/// ~/.claude/settings.json. Fixed under $HOME, independent of XDG.
pub fn settings_path() -> PathBuf {
    resolved().settings_path
}

/// ~/.claude.json. Fixed under $HOME, independent of XDG.
pub fn claude_json_path() -> PathBuf {
    resolved().claude_json_path
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    /// `BaseDirectories` derives `config_home`/`state_home` from process env
    /// at construction time, with no way to inject fake values through its
    /// public constructors. Its `#[non_exhaustive]` marker blocks literal
    /// construction from outside the crate, but not field assignment on an
    /// already-owned value (both fields are `pub`), so we build one for real
    /// via `with_prefix` (to get `user_prefix` set correctly) and then
    /// overwrite just the two fields under test.
    fn fake_dirs(config_home: PathBuf, state_home: PathBuf) -> BaseDirectories {
        let mut dirs = BaseDirectories::with_prefix(APP_NAME);
        dirs.config_home = Some(config_home);
        dirs.state_home = Some(state_home);
        dirs
    }

    #[rstest]
    // XDG_CONFIG_HOME / XDG_STATE_HOME both set to explicit values.
    #[case::xdg_vars_set(
        PathBuf::from("/fake/home"),
        PathBuf::from("/fake/xdg-config"),
        PathBuf::from("/fake/xdg-state")
    )]
    // Both unset: BaseDirectories itself falls back to $HOME/.config and
    // $HOME/.local/state (verified by the `xdg` crate's own test suite), so
    // "unset" is modeled here as that already-resolved fallback value.
    #[case::xdg_vars_unset(
        PathBuf::from("/fake/home"),
        PathBuf::from("/fake/home/.config"),
        PathBuf::from("/fake/home/.local/state")
    )]
    // Only XDG_CONFIG_HOME set; XDG_STATE_HOME falls back independently.
    #[case::config_set_state_unset(
        PathBuf::from("/fake/home"),
        PathBuf::from("/fake/xdg-config"),
        PathBuf::from("/fake/home/.local/state")
    )]
    // HOME swapped to a different tree entirely, XDG vars unset.
    #[case::home_overridden(
        PathBuf::from("/tmp/ccform-test-home"),
        PathBuf::from("/tmp/ccform-test-home/.config"),
        PathBuf::from("/tmp/ccform-test-home/.local/state")
    )]
    fn test_resolve_builds_all_paths_from_config(
        #[case] home: PathBuf,
        #[case] config_home: PathBuf,
        #[case] state_home: PathBuf,
    ) {
        let dirs = fake_dirs(config_home.clone(), state_home.clone());

        let config_dir = config_home.join(APP_NAME);
        let state_dir = state_home.join(APP_NAME);

        assert_eq!(
            resolve(&dirs, &home),
            Resolved {
                entry_path: config_dir.join("ccform.lua"),
                import_path: config_dir.join("import.lua"),
                state_path: state_dir.join("state.json"),
                state_backup_path: state_dir.join("state.json.backup"),
                settings_path: home.join(".claude").join("settings.json"),
                claude_json_path: home.join(".claude.json"),
                config_dir,
                state_dir,
            }
        );
    }
}
