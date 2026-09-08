//! The sync IPC surface.
//!
//! Networking lives here rather than in the webview because it has to: the CSP is
//! `connect-src 'self'` and the capability set grants no http plugin, both deliberately.

use remotier_sync_proto::api::{InstanceInfo, Share};
use tauri::State;

use crate::error::{Error, Result};
use crate::state::AppState;
use crate::sync::client::Client;
use crate::sync::engine::{DeviceLayout, SyncStatus};

/// Emitted when the engine finishes a cycle or the session changes. Low-frequency
/// lifecycle, which is what Tauri events are for here - terminal bytes use a Channel.
pub const SYNC_STATUS_EVENT: &str = "sync://status";

#[tauri::command(async)]
pub async fn sync_status(state: State<'_, AppState>) -> Result<SyncStatus> {
    state.sync().status().await
}

/// Ask an address whether it is a Remotier instance, before anything is sent to it.
///
/// The sign-in screen calls this when the URL changes, so a typo reads as "no Remotier
/// instance there" rather than as a failed login against a stranger's server.
#[tauri::command(async)]
pub async fn sync_probe_instance(url: String) -> Result<InstanceInfo> {
    let info = Client::new(&url)?.instance().await?;

    if !info.format_versions.contains(&remotier_sync_proto::FORMAT_VERSION) {
        return Err(Error::Sync(format!(
            "that instance speaks format {:?}, this build speaks {}",
            info.format_versions,
            remotier_sync_proto::FORMAT_VERSION
        )));
    }
    Ok(info)
}

/// Create an account. The returned recovery code is shown once and never stored.
#[tauri::command(async)]
pub async fn sync_register(
    state: State<'_, AppState>,
    url: String,
    email: String,
    password: String,
    device_name: String,
) -> Result<String> {
    let code = state
        .sync()
        .register(&url, &email, &password, &device_name)
        .await?;
    Ok(code.to_string())
}

#[tauri::command(async)]
pub async fn sync_login(
    state: State<'_, AppState>,
    url: String,
    email: String,
    password: String,
    device_name: String,
) -> Result<SyncStatus> {
    state
        .sync()
        .login(&url, &email, &password, &device_name)
        .await?;
    state.sync().status().await
}

#[tauri::command(async)]
pub async fn sync_recover(
    state: State<'_, AppState>,
    url: String,
    email: String,
    recovery_code: String,
    device_name: String,
) -> Result<SyncStatus> {
    state
        .sync()
        .recover(&url, &email, &recovery_code, &device_name)
        .await?;
    state.sync().status().await
}

#[tauri::command(async)]
pub async fn sync_logout(state: State<'_, AppState>) -> Result<SyncStatus> {
    state.sync().logout().await?;
    state.sync().status().await
}

#[tauri::command(async)]
pub async fn sync_now(state: State<'_, AppState>) -> Result<SyncStatus> {
    state.sync().sync_now().await
}

/// Machines with a saved layout, this one excluded.
#[tauri::command(async)]
pub async fn sync_devices(state: State<'_, AppState>) -> Result<Vec<DeviceLayout>> {
    state.sync().devices()
}

/// One device's saved tabs, as the opaque JSON the frontend serialises layouts to.
///
/// Fetched on request and never applied by the engine: replacing what is on screen with
/// another machine's tabs is the user's decision, not a sync outcome.
#[tauri::command(async)]
pub async fn sync_device_layout(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<String> {
    state.sync().device_layout(&device_id)
}

/// Share a group, and everything beneath it, with another account on this instance.
///
/// They get the hostnames, ports and usernames in that branch. They do not get a
/// credential: no password, passphrase or private key syncs at all.
#[tauri::command(async)]
pub async fn sync_share_group(
    state: State<'_, AppState>,
    group_id: String,
    email: String,
) -> Result<Vec<Share>> {
    state.sync().share_group(&group_id, &email).await?;
    state.sync().shares(&group_id).await
}

#[tauri::command(async)]
pub async fn sync_list_shares(
    state: State<'_, AppState>,
    group_id: String,
) -> Result<Vec<Share>> {
    state.sync().shares(&group_id).await
}

/// Stop sharing, and rotate the group's key so the removed member's copy stops working
/// for anything new.
#[tauri::command(async)]
pub async fn sync_unshare_group(
    state: State<'_, AppState>,
    group_id: String,
    user_id: String,
) -> Result<Vec<Share>> {
    state.sync().unshare_group(&group_id, &user_id).await?;
    state.sync().shares(&group_id).await
}
