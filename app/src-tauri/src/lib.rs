mod commands;

use tauri::Manager;

pub const BUILD_SHA: &str = env!("TD_GIT_SHA");

/// `$TA doctor` finds the commit this binary was built from by this marker.
#[used]
static BUILD_MARK: &str = concat!("touchdeck-build-sha:", env!("TD_GIT_SHA"));

pub fn run() {
    let mock = std::env::var("TOUCHDECK_MOCK").is_ok_and(|value| value == "1");
    let builder = tauri::Builder::default().manage(commands::AppState::new(mock));
    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio_webdriver::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_decks,
            commands::connect,
            commands::disconnect,
            commands::deck_info,
            commands::console_send,
            commands::e2e_mock
        ]);
    #[cfg(not(feature = "e2e"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::list_decks,
        commands::connect,
        commands::disconnect,
        commands::deck_info,
        commands::console_send
    ]);
    builder
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                window.set_title(&format!("TouchDeck {BUILD_SHA}"))?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the TouchDeck window failed to start");
}
