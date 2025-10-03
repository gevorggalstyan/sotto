use chrono::Local;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample};
use enigo::{Enigo, Key, Keyboard, Settings};
use image::GenericImageView;
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, State,
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{Code, Modifiers, ShortcutState};

// Tray icon ID for accessing tray from shortcut handler
const TRAY_ID: &str = "main-tray";

// Audio recording state - stores the stream and buffers audio data
struct AudioRecorder {
    stream: Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    temp_buffer: Arc<Mutex<Vec<f32>>>, // Temporary buffer for incoming 48kHz samples
}

// Safety: AudioRecorder is only accessed from the main thread via parking_lot::Mutex
unsafe impl Send for AudioRecorder {}
unsafe impl Sync for AudioRecorder {}

impl AudioRecorder {
    fn new() -> Self {
        Self {
            stream: None,
            buffer: Arc::new(Mutex::new(Vec::new())),
            sample_rate: 0,
            temp_buffer: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.stream.is_none() {
            // Clear previous buffers
            self.buffer.lock().clear();
            self.temp_buffer.lock().clear();

            // Get the default audio host and input device
            let host = cpal::default_host();
            let device = host
                .default_input_device()
                .ok_or("No input device available")?;

            // Try to get 16kHz config (Whisper requirement)
            let config = match device.supported_input_configs() {
                Ok(configs) => {
                    // Try to find a 16kHz mono config
                    let mut found_16khz = None;
                    for config in configs {
                        if config.min_sample_rate().0 <= 16000
                            && config.max_sample_rate().0 >= 16000
                        {
                            // Found a config that supports 16kHz
                            found_16khz = Some(config.with_sample_rate(cpal::SampleRate(16000)));
                            break;
                        }
                    }

                    match found_16khz {
                        Some(cfg) => cfg,
                        None => {
                            // Fallback to default config if 16kHz not supported
                            println!("16kHz not supported, using default config");
                            device.default_input_config()?
                        }
                    }
                }
                Err(_) => device.default_input_config()?,
            };

            self.sample_rate = config.sample_rate().0;
            println!("Starting audio capture with config: {:?}", config);

            // Create the audio stream based on sample format with buffering
            let buffer_clone = self.buffer.clone();
            let temp_buffer_clone = self.temp_buffer.clone();
            let record_sample_rate = self.sample_rate;

            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => build_input_stream::<f32>(
                    &device,
                    &config.into(),
                    buffer_clone,
                    temp_buffer_clone,
                    record_sample_rate,
                )?,
                cpal::SampleFormat::I16 => build_input_stream::<i16>(
                    &device,
                    &config.into(),
                    buffer_clone,
                    temp_buffer_clone,
                    record_sample_rate,
                )?,
                cpal::SampleFormat::U16 => build_input_stream::<u16>(
                    &device,
                    &config.into(),
                    buffer_clone,
                    temp_buffer_clone,
                    record_sample_rate,
                )?,
                _ => return Err("Unsupported sample format".into()),
            };

            stream.play()?;
            self.stream = Some(stream);
        }
        Ok(())
    }

    fn stop(&mut self) -> Vec<f32> {
        if let Some(stream) = self.stream.take() {
            drop(stream);
            println!("Audio capture stopped - microphone released");
        }

        // Get the buffered audio (already resampled to 16kHz in real-time) and clear
        let audio_data = self.buffer.lock().clone();
        self.buffer.lock().clear();
        self.temp_buffer.lock().clear();

        println!(
            "Captured {} samples at 16kHz (recorded at {}Hz)",
            audio_data.len(),
            self.sample_rate
        );

        audio_data
    }

    fn get_sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
    temp_buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
) -> Result<cpal::Stream, Box<dyn std::error::Error>>
where
    T: cpal::Sample + cpal::SizedSample,
    f32: FromSample<T>,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            // Convert samples to f32
            let mut temp = temp_buffer.lock();
            for &sample in data {
                temp.push(f32::from_sample(sample));
            }

            // If recording at 48kHz, downsample to 16kHz in real-time
            if sample_rate == 48000 && temp.len() >= 3 {
                let mut buf = buffer.lock();
                // Simple decimation: take every 3rd sample (48000/3 = 16000)
                for i in (0..temp.len()).step_by(3) {
                    if let Some(&sample) = temp.get(i) {
                        buf.push(sample);
                    }
                }
                temp.clear();
            } else if sample_rate == 16000 {
                // Already 16kHz, just copy directly
                let mut buf = buffer.lock();
                buf.extend_from_slice(&temp);
                temp.clear();
            }
        },
        |err| eprintln!("Audio stream error: {}", err),
        None,
    )?;

    Ok(stream)
}

// Insert text at cursor position using clipboard save/restore + paste
fn insert_text_at_cursor(app: &AppHandle, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Save current clipboard content
    let original_clipboard = app.clipboard().read_text().ok();

    // Write our text to clipboard
    app.clipboard().write_text(text)?;

    // Wait for clipboard to update and for user to release modifier keys
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Release Option key if it's still held (to avoid Cmd+Option+V)
    let mut enigo = Enigo::new(&Settings::default())?;
    enigo.key(Key::Alt, enigo::Direction::Release)?;

    // Small delay after releasing Option
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Simulate Cmd+V to paste
    enigo.key(Key::Meta, enigo::Direction::Press)?;
    enigo.key(Key::Unicode('v'), enigo::Direction::Click)?;
    enigo.key(Key::Meta, enigo::Direction::Release)?;

    // Wait a bit for paste to complete
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Restore original clipboard content
    if let Some(original) = original_clipboard {
        let _ = app.clipboard().write_text(original);
    }

    println!("Inserted text via clipboard (restored original)");

    Ok(())
}

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load tray icons early for use in shortcut handler
    let icon_bytes = include_bytes!("../icons/Sotto Logo.png");
    let icon_image = image::load_from_memory(icon_bytes).expect("Failed to load icon");
    let (width, height) = icon_image.dimensions();
    let rgba = icon_image.to_rgba8().into_raw();
    let default_icon = Arc::new(Mutex::new(Image::new_owned(rgba, width, height)));

    let active_icon_bytes = include_bytes!("../icons/Sotto Logo Active.png");
    let active_icon_image =
        image::load_from_memory(active_icon_bytes).expect("Failed to load active icon");
    let (active_width, active_height) = active_icon_image.dimensions();
    let active_rgba = active_icon_image.to_rgba8().into_raw();
    let active_icon = Arc::new(Mutex::new(Image::new_owned(
        active_rgba,
        active_width,
        active_height,
    )));

    let default_icon_clone = default_icon.clone();
    let active_icon_clone = active_icon.clone();

    // Create audio recorder wrapped in Arc<Mutex>
    let audio_recorder = Arc::new(Mutex::new(AudioRecorder::new()));
    let audio_recorder_clone = audio_recorder.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts(["alt+space"])
                .expect("Failed to register shortcut")
                .with_handler(move |app, shortcut, event| {
                    if shortcut.matches(Modifiers::ALT, Code::Space) {
                        if let Some(tray) = app.tray_by_id(TRAY_ID) {
                            match event.state {
                                ShortcutState::Pressed => {
                                    // Switch to active icon
                                    let icon = active_icon_clone.lock();
                                    let _ = tray.set_icon(Some(icon.clone()));

                                    // Start audio capture
                                    let mut recorder = audio_recorder_clone.lock();
                                    match recorder.start() {
                                        Ok(_) => {
                                            println!("Option+Space pressed - recording started")
                                        }
                                        Err(e) => eprintln!("Failed to start audio capture: {}", e),
                                    }
                                }
                                ShortcutState::Released => {
                                    // Switch back to default icon
                                    let icon = default_icon_clone.lock();
                                    let _ = tray.set_icon(Some(icon.clone()));

                                    // Stop audio capture and get buffered audio
                                    let mut recorder = audio_recorder_clone.lock();
                                    let audio_samples = recorder.stop();
                                    let sample_rate = recorder.get_sample_rate();
                                    println!("Option+Space released - recording stopped");

                                    // Calculate audio duration in seconds
                                    let duration_secs = if sample_rate > 0
                                        && !audio_samples.is_empty()
                                    {
                                        audio_samples.len() as f32 / 16000.0 // Always 16kHz after resampling
                                    } else {
                                        0.0
                                    };

                                    // TODO: Transcribe audio_samples using Whisper
                                    // For now, insert datetime with duration
                                    let datetime =
                                        Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                                    let text =
                                        format!("{} (duration: {:.2}s)", datetime, duration_secs);
                                    match insert_text_at_cursor(app, &text) {
                                        Ok(_) => println!(
                                            "Inserted: {} (captured {} samples, {:.2}s)",
                                            text,
                                            audio_samples.len(),
                                            duration_secs
                                        ),
                                        Err(e) => eprintln!("Failed to insert text: {}", e),
                                    }
                                }
                            }
                        }
                    }
                })
                .build(),
        )
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

            // Load default icon for tray
            let icon_bytes = include_bytes!("../icons/Sotto Logo.png");
            let icon_image = image::load_from_memory(icon_bytes)
                .map_err(|e| tauri::Error::AssetNotFound(format!("Failed to load icon: {}", e)))?;
            let (width, height) = icon_image.dimensions();
            let rgba = icon_image.to_rgba8().into_raw();
            let icon = Image::new_owned(rgba, width, height);

            // Create tray icon with ID (same ID as used in shortcut handler)
            let _tray = TrayIconBuilder::with_id(TRAY_ID)
                .icon(icon)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
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
