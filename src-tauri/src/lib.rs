pub mod commands;
pub mod deflicker;
pub mod interpolate;
pub mod params;
pub mod preview;
pub mod rrdata;
pub mod sequence;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::browse_folder,
            commands::scan_folder,
            commands::compute_plan,
            commands::write_plan,
            commands::revert_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
