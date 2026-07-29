pub mod app_state;
pub mod commands;
pub mod device;
pub mod domain;
pub mod probe_adapter;

use crate::probe_adapter::resolve_probe_dir;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(app_state::AppState::default())
        .setup(|app| {
            let environment_override = std::env::var_os("CODEX_HALO_PROBE_DIR");
            let home_dir = app.path().home_dir().ok();
            let probe_dir =
                resolve_probe_dir(None, environment_override.as_deref(), home_dir.as_deref());
            app.state::<app_state::AppState>()
                .start_probe_worker(app.handle().clone(), probe_dir);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_adapter_status,
            commands::simulate_signal,
            commands::manual_bind,
            commands::toggle_lock,
            commands::swap_slots,
            commands::update_effect,
            commands::set_global_brightness,
            commands::reset_virtual_device
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            app_handle
                .state::<app_state::AppState>()
                .stop_probe_worker();
        }
    });
}
