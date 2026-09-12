pub mod commands;
pub mod crypto;
pub mod db;
pub mod error;
pub mod icons;
pub mod ssh;
pub mod state;
pub mod sync;
pub mod vars;

use tauri::Manager;

use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.set_focus();
                }
            }))
            .plugin(tauri_plugin_window_state::Builder::default().build())
            // Registered for its Rust side only: `commands::updates` drives it, so the
            // webview never gets permission to download or run an installer.
            .plugin(tauri_plugin_updater::Builder::new().build());
    }

    builder
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Debug)
                        .build(),
                )?;
            }

            let data_dir = app.path().app_data_dir()?;
            app.manage(AppState::new(&data_dir)?);

            // Sync runs in the background from launch. It is a no-op while signed out,
            // and a failure here must never stop the app starting - the same rule the
            // key store follows.
            {
                let state = app.state::<AppState>();
                sync::worker::spawn(
                    app.handle().clone(),
                    state.sync(),
                    std::sync::Arc::clone(&state.db),
                );
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::groups::list_groups,
            commands::groups::create_group,
            commands::groups::update_group,
            commands::groups::delete_group,
            commands::hosts::list_hosts,
            commands::hosts::create_host,
            commands::hosts::update_host,
            commands::hosts::delete_host,
            commands::hosts::move_hosts,
            commands::icons::icon_catalog,
            commands::icons::icon_image,
            commands::identities::list_identities,
            commands::identities::create_identity,
            commands::identities::update_identity,
            commands::identities::delete_identity,
            commands::keys::list_keys,
            commands::keys::register_system_key,
            commands::keys::update_key,
            commands::keys::delete_key,
            commands::keys::generate_key,
            commands::keys::import_key,
            commands::ssh::resolve_host,
            commands::ssh::ssh_connect,
            commands::ssh::ssh_write,
            commands::ssh::ssh_resize,
            commands::ssh::ssh_disconnect,
            commands::ssh::ssh_sessions,
            commands::ssh::list_agent_keys,
            commands::ssh::scan_system_keys,
            commands::ssh::list_known_hosts,
            commands::ssh::revoke_known_host,
            commands::ssh::preview_ssh_config,
            commands::ssh::import_ssh_config,
            commands::sync::sync_status,
            commands::sync::sync_probe_instance,
            commands::sync::sync_register,
            commands::sync::sync_login,
            commands::sync::sync_recover,
            commands::sync::sync_logout,
            commands::sync::sync_now,
            commands::sync::sync_history,
            commands::sync::sync_devices,
            commands::sync::sync_device_layout,
            commands::sync::sync_share_group,
            commands::sync::sync_list_shares,
            commands::sync::sync_unshare_group,
            commands::settings::get_settings,
            commands::settings::set_setting,
            commands::settings::vault_status,
            commands::settings::get_session_state,
            commands::settings::set_session_state,
            commands::workspaces::list_workspaces,
            commands::workspaces::save_workspace,
            commands::workspaces::delete_workspace,
            commands::vars::list_var_defs,
            commands::vars::upsert_var_def,
            commands::vars::delete_var_def,
            commands::vars::list_var_values,
            commands::vars::set_var_value,
            commands::vars::clear_var_value,
            commands::updates::update_check,
            commands::updates::update_install,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
