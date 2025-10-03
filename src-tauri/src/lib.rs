use tauri::{Manager, menu::{Menu, MenuItem}, tray::TrayIconBuilder, image::Image};
use image::GenericImageView;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .setup(|app| {
            // Hide from dock on macOS
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            
            // Create menu items
            let show_i = MenuItem::with_id(app, "show", "Settings", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            
            // Build menu
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;
            
            // Load tray icon from embedded PNG file
            let icon_bytes = include_bytes!("../icons/Sotto Logo.png");
            let icon_image = image::load_from_memory(icon_bytes)
                .map_err(|e| tauri::Error::AssetNotFound(format!("Failed to load icon: {}", e)))?;
            let (width, height) = icon_image.dimensions();
            let rgba = icon_image.to_rgba8().into_raw();
            let icon = Image::new_owned(rgba, width, height);

            // Create tray icon
            let _tray = TrayIconBuilder::new()
                .icon(icon)
                .menu(&menu)
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .build(app)?;
            
            // Configure window to hide instead of close
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Prevent the window from closing, hide it instead
                        api.prevent_close();
                        // Hide the window
                        let _ = window_clone.hide();
                    }
                });
            }
            
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
