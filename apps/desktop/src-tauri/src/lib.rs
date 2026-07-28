pub mod app_state;
pub mod commands;
pub mod domain;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(app_state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::simulate_signal,
            commands::manual_bind,
            commands::toggle_lock,
            commands::swap_slots,
            commands::update_effect,
            commands::set_global_brightness,
            commands::reset_virtual_device
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
