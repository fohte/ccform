//! `ccform show`: reads state.json and prints it as pretty-printed JSON.
//! Purely read-only — it never writes anywhere.

use std::path::PathBuf;

use crate::paths;
use crate::state::store;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path} not found. Run `ccform init` first.")]
    NotFound { path: PathBuf },

    #[error(transparent)]
    State(#[from] store::Error),

    #[error("failed to serialize state as JSON")]
    Serialize(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Prints state.json to stdout as pretty-printed JSON, or fails with
/// [`Error::NotFound`] if it does not exist yet.
pub fn run() -> Result<()> {
    let state = store::load()?.ok_or_else(|| Error::NotFound {
        path: paths::state_path(),
    })?;
    println!("{}", serde_json::to_string_pretty(&state.to_value())?);
    Ok(())
}
