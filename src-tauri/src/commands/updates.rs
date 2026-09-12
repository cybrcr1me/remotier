//! Checking for, and installing, a new version.
//!
//! The updater plugin is driven from here rather than from its JavaScript bindings, so the
//! webview keeps no permission to download or run an installer - it can only ask these two
//! commands, the same rule the rest of the app follows for disk and network access.

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

use crate::error::{Error, Result};

/// Emitted while the new version downloads, so a large download is not a dead button.
pub const PROGRESS_EVENT: &str = "update://progress";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// The version on offer.
    pub version: String,
    /// What is running now, so the UI can say "0.1.1 → 0.2.0" without guessing.
    pub current_version: String,
    /// The release notes from the manifest, if it carried any.
    pub notes: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    downloaded: u64,
    /// Absent until the server has said how big the download is.
    total: Option<u64>,
}

fn failed(e: tauri_plugin_updater::Error) -> Error {
    Error::Update(e.to_string())
}

/// `None` when this is already the newest version.
///
/// A failure here is ordinary - the machine may be offline - so the UI reports it quietly
/// rather than treating it as something the user has to act on.
#[tauri::command(async)]
pub async fn update_check(app: AppHandle) -> Result<Option<UpdateInfo>> {
    let update = app.updater().map_err(failed)?.check().await.map_err(failed)?;

    Ok(update.map(|update| UpdateInfo {
        version: update.version.clone(),
        current_version: update.current_version.clone(),
        notes: update.body.clone(),
    }))
}

/// Download the new version, install it, and restart into it.
///
/// Checked again rather than holding the `Update` from `update_check`: the object is not
/// `Send`-friendly to park in shared state, and one more conditional GET is cheaper than
/// the machinery to keep it.
#[tauri::command(async)]
pub async fn update_install(app: AppHandle) -> Result<()> {
    let update = app
        .updater()
        .map_err(failed)?
        .check()
        .await
        .map_err(failed)?
        .ok_or_else(|| Error::Update("there is no new version to install".into()))?;

    let mut downloaded: u64 = 0;
    let progress = app.clone();

    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                // A failed emit means the window has gone; the install should still finish.
                let _ = progress.emit(PROGRESS_EVENT, Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(failed)?;

    // Windows hands over to the NSIS installer, which closes the app itself, so this is
    // only reached on macOS and Linux - where the bundle has been replaced underneath a
    // process still running the old one.
    app.restart();
}
