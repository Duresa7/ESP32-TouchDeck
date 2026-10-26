mod commands;
mod sources;

use tauri::Manager;

pub const BUILD_SHA: &str = env!("TD_GIT_SHA");

#[used]
static DOCTOR_MARKER: &str = concat!("touchdeck-build-sha:", env!("TD_GIT_SHA"));

pub fn run() {
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
            commands::wifi_state,
            commands::wifi_scan,
            commands::wifi_set,
            commands::wifi_forget,
            commands::ble_command,
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
        commands::wifi_state,
        commands::wifi_scan,
        commands::wifi_set,
        commands::wifi_forget,
        commands::ble_command,
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
        .setup(|app| {
            let paths = app.path();
            app.state::<commands::AppState>()
                .use_dirs(paths.app_config_dir().ok(), paths.app_log_dir().ok())?;
            app.state::<commands::AppState>()
                .use_icon_dir(paths.app_cache_dir().ok());
            if let Some(window) = app.get_webview_window("main") {
                window.set_title(&format!("TouchDeck {BUILD_SHA}"))?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the TouchDeck window failed to start");
}
