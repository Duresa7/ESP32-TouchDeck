mod commands;
mod sources;

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use tauri::Manager;

pub const BUILD_SHA: &str = env!("TD_GIT_SHA");

#[used]
static DOCTOR_MARKER: &str = concat!("touchdeck-build-sha:", env!("TD_GIT_SHA"));

/// An explicit app-data override must never silently fall back to the user's directories.
fn app_data_override(value: Option<OsString>) -> io::Result<Option<PathBuf>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "TOUCHDECK_APP_DATA_DIR must be a non-empty absolute directory",
        ));
    }
    std::fs::create_dir_all(&path)?;
    path.canonicalize().map(Some)
}

pub fn run() {
    // Resolve this before source workers, app state or WebViews can initialize.
    let app_data = app_data_override(std::env::var_os("TOUCHDECK_APP_DATA_DIR"))
        .expect("invalid TOUCHDECK_APP_DATA_DIR");
    let mut context = tauri::generate_context!();
    if let Some(base) = &app_data {
        for window in &mut context.config_mut().app.windows {
            window.data_directory = Some(base.join("webview"));
        }
    }
    let mock = std::env::var("TOUCHDECK_MOCK").is_ok_and(|value| value == "1");
    let builder = tauri::Builder::default()
        .manage(commands::AppState::new(mock))
        .manage(sources::SourcesState::new(mock));
    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio_webdriver::init())
        .invoke_handler(tauri::generate_handler![
            commands::update_firmware,
            commands::list_decks,
            commands::connect,
            commands::disconnect,
            commands::deck_info,
            commands::console_send,
            commands::e2e_mock,
            commands::e2e_mock_press,
            commands::e2e_mock_sets,
            sources::sources_view,
            sources::sources_status,
            sources::sources_save,
            sources::sources_set_key,
            sources::sources_clear_key,
            sources::sources_refresh,
            sources::sources_use_layout,
            sources::statusline_preview,
            sources::statusline_install,
            commands::push_layout,
            commands::remove_unused_icons,
            commands::builtin_icons,
            commands::default_icon,
            commands::icon_file,
            commands::take_app_icon,
            commands::pull_layout,
            commands::settings_get,
            commands::settings_put,
            commands::reboot_deck,
            commands::e2e_mock_record,
            commands::mirror_state,
            commands::set_action_layout,
            commands::action_layout,
            commands::pending_approvals,
            commands::action_log,
            commands::approval_answer,
            commands::focus_window,
            commands::e2e_emit,
            commands::e2e_input_log,
        ]);
    #[cfg(not(feature = "e2e"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::update_firmware,
        commands::list_decks,
        commands::connect,
        commands::disconnect,
        commands::deck_info,
        commands::console_send,
        sources::sources_view,
        sources::sources_status,
        sources::sources_save,
        sources::sources_set_key,
        sources::sources_clear_key,
        sources::sources_refresh,
        sources::sources_use_layout,
        sources::statusline_preview,
        sources::statusline_install,
        commands::push_layout,
        commands::remove_unused_icons,
        commands::builtin_icons,
        commands::default_icon,
        commands::icon_file,
        commands::take_app_icon,
        commands::pull_layout,
        commands::settings_get,
        commands::settings_put,
        commands::reboot_deck,
        commands::mirror_state,
        commands::set_action_layout,
        commands::action_layout,
        commands::pending_approvals,
        commands::action_log,
        commands::approval_answer,
        commands::focus_window,
    ]);
    builder
        .setup(move |app| {
            let paths = app.path();
            let (config, logs, cache) = match &app_data {
                Some(base) => (
                    Some(base.join("config")),
                    Some(base.join("logs")),
                    Some(base.join("cache")),
                ),
                None => (
                    paths.app_config_dir().ok(),
                    paths.app_log_dir().ok(),
                    paths.app_cache_dir().ok(),
                ),
            };
            app.state::<commands::AppState>().use_dirs(config, logs)?;
            app.state::<commands::AppState>().use_icon_dir(cache);
            if let Some(window) = app.get_webview_window("main") {
                window.set_title(&format!("TouchDeck {BUILD_SHA}"))?;
            }
            Ok(())
        })
        .run(context)
        .expect("the TouchDeck window failed to start");
}

#[cfg(test)]
mod packaging_tests {
    use super::*;

    #[test]
    fn absent_override_preserves_defaults_and_invalid_values_fail_closed() {
        assert_eq!(app_data_override(None).unwrap(), None);
        for value in ["", "relative/scratch"] {
            assert_eq!(
                app_data_override(Some(value.into())).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn explicit_override_is_created_and_a_file_is_rejected() {
        let scratch =
            std::env::temp_dir().join(format!("touchdeck-app-data-{}", std::process::id()));
        let path = scratch.join("nested");
        let resolved = app_data_override(Some(path.clone().into_os_string()))
            .unwrap()
            .unwrap();
        assert_eq!(resolved, path.canonicalize().unwrap());
        let file = scratch.join("file");
        std::fs::write(&file, "fixture").unwrap();
        assert!(app_data_override(Some(file.into_os_string())).is_err());
        std::fs::remove_dir_all(scratch).unwrap();
    }
}
