use chrono::Local;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample};
use enigo::{Enigo, Key, Keyboard, Settings};
use image::GenericImageView;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, State,
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{Code, Modifiers, ShortcutState};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

// Tray icon ID for accessing tray from shortcut handler
const TRAY_ID: &str = "main-tray";

// Whisper model filename
const WHISPER_MODEL_NAME: &str = "ggml-large-v3-turbo.bin";

// Get the model file path in app data directory
fn get_model_path(app: &AppHandle) -> PathBuf {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .expect("Failed to get app data dir");
    std::fs::create_dir_all(&app_data_dir).ok();
    app_data_dir.join("models").join(WHISPER_MODEL_NAME)
}

// Check if model exists
fn model_exists(app: &AppHandle) -> bool {
    get_model_path(app).exists()
}

// Download Whisper model from HuggingFace
fn download_model(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let model_path = get_model_path(app);

    // Create models directory if it doesn't exist
    if let Some(parent) = model_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    println!("Downloading Whisper large-v3-turbo model (~1.5GB)...");

    // Download from HuggingFace
    let url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin";

    let mut response = reqwest::blocking::get(url)?;

    if !response.status().is_success() {
        return Err(format!("Failed to download model: HTTP {}", response.status()).into());
    }

    // Stream directly to temporary file
    let temp_path = model_path.with_extension("bin.tmp");
    let mut file = std::fs::File::create(&temp_path)?;

    println!("Downloading model, streaming to disk...");
    let bytes_written = std::io::copy(&mut response, &mut file)?;
    println!("Downloaded {} MB", bytes_written / 1024 / 1024);

    // Rename temp file to final name (atomic operation)
    std::fs::rename(&temp_path, &model_path)?;

    println!("Model downloaded successfully to: {:?}", model_path);
    Ok(())
}

// Transcribe audio using Whisper model
fn transcribe_audio(
    ctx: &mut WhisperContext,
    audio_data: &[f32],
) -> Result<String, Box<dyn std::error::Error>> {
    if audio_data.is_empty() {
        return Ok(String::new());
    }

    // Skip transcription for very short audio (< 0.3s at 16kHz)
    if audio_data.len() < 4800 {
        println!("Audio too short ({} samples), skipping transcription", audio_data.len());
        return Ok(String::new());
    }

    println!("Starting transcription of {} samples...", audio_data.len());

    // Create transcription parameters optimized for speed
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en")); // English - skips auto-detection
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    // Use half of available CPU threads (leave room for other processes)
    let n_threads = (std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4) / 2)
        .max(1);
    params.set_n_threads(n_threads as i32);

    // Run transcription
    let mut state = ctx
        .create_state()
        .map_err(|e| format!("Failed to create state: {}", e))?;
    state
        .full(params, audio_data)
        .map_err(|e| format!("Failed to run transcription: {}", e))?;

    // Get the transcribed text from all segments using iterator
    let mut transcription = String::new();

    for segment in state.as_iter() {
        if let Ok(text) = segment.to_str() {
            transcription.push_str(text);
        }
    }

    let trimmed = transcription.trim().to_string();
    println!("Transcription complete: \"{}\"", trimmed);

    Ok(trimmed)
}

// Load Whisper model (with Metal GPU support)
fn load_whisper_model(app: &AppHandle) -> Result<WhisperContext, Box<dyn std::error::Error>> {
    let model_path = get_model_path(app);

    if !model_path.exists() {
        return Err("Model not found. Please download the model first.".into());
    }

    println!("Loading Whisper model from: {:?}", model_path);

    // WhisperContext will automatically use Metal GPU if compiled with metal feature
    let params = WhisperContextParameters::default();
    let ctx =
        WhisperContext::new_with_params(model_path.to_str().ok_or("Invalid model path")?, params)?;

    println!("Whisper model loaded successfully (Metal GPU enabled via feature flag)");
    Ok(ctx)
}

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
                                    let duration_secs = if !audio_samples.is_empty() {
                                        audio_samples.len() as f32 / 16000.0 // Always 16kHz after resampling
                                    } else {
                                        0.0
                                    };

                                    // Transcribe audio using Whisper
                                    let whisper_state: tauri::State<
                                        Arc<Mutex<Option<WhisperContext>>>,
                                    > = app.state();
                                    let transcription =
                                        if let Some(ctx) = whisper_state.lock().as_mut() {
                                            match transcribe_audio(ctx, &audio_samples) {
                                                Ok(text) => text,
                                                Err(e) => {
                                                    eprintln!("Transcription failed: {}", e);
                                                    String::from("[Transcription failed]")
                                                }
                                            }
                                        } else {
                                            String::from("[Model not loaded]")
                                        };

                                    // Insert transcribed text only if not empty
                                    if !transcription.is_empty()
                                        && transcription != "[Model not loaded]"
                                        && transcription != "[Transcription failed]" {
                                        match insert_text_at_cursor(app, &transcription) {
                                            Ok(_) => println!("Inserted transcription ({:.2}s): {}", duration_secs, transcription),
                                            Err(e) => eprintln!("Failed to insert text: {}", e),
                                        }
                                    } else {
                                        println!("No text to insert ({})", transcription);
                                    }
                                }
                            }
                        }
                    }
                })
                .build(),
        )
        .manage(Arc::new(Mutex::new(None::<WhisperContext>))) // Whisper model state
        .invoke_handler(tauri::generate_handler![greet])
        .setup(|app| {
            // Hide from dock on macOS
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Check if Whisper model exists, download if missing
            let model_ready = if !model_exists(app.handle()) {
                println!(
                    "Whisper model not found at: {:?}",
                    get_model_path(app.handle())
                );
                println!("Starting model download in background...");

                // Spawn download in background thread
                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    match download_model(&app_handle) {
                        Ok(_) => {
                            println!("Model download complete! Loading model...");
                            // Try to load the model after download
                            match load_whisper_model(&app_handle) {
                                Ok(ctx) => {
                                    let whisper_state: tauri::State<
                                        Arc<Mutex<Option<WhisperContext>>>,
                                    > = app_handle.state();
                                    *whisper_state.lock() = Some(ctx);
                                    println!(
                                        "Whisper model initialized successfully after download"
                                    );
                                }
                                Err(e) => {
                                    eprintln!("Failed to load Whisper model after download: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to download Whisper model: {}", e);
                        }
                    }
                });
                false
            } else {
                println!("Whisper model found, loading...");
                match load_whisper_model(app.handle()) {
                    Ok(ctx) => {
                        let whisper_state: tauri::State<Arc<Mutex<Option<WhisperContext>>>> =
                            app.state();
                        *whisper_state.lock() = Some(ctx);
                        println!("Whisper model initialized successfully");
                        true
                    }
                    Err(e) => {
                        eprintln!("Failed to load Whisper model: {}", e);
                        false
                    }
                }
            };

            println!("Model ready: {}", model_ready);

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
