use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample};
use enigo::{Enigo, Key, Keyboard, Settings};
use image::GenericImageView;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::{Code, Modifiers, ShortcutState};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

// Tray icon ID for accessing tray from shortcut handler
const TRAY_ID: &str = "main-tray";

// Whisper model information
#[derive(Clone)]
struct ModelInfo {
    name: &'static str,
    filename: &'static str,
    url: &'static str,
    size_mb: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DownloadStatus {
    Downloading,
    Completed,
    Failed,
}

#[derive(Clone, Debug)]
struct DownloadRecord {
    status: DownloadStatus,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    error: Option<String>,
}

impl DownloadRecord {
    fn new(status: DownloadStatus) -> Self {
        Self {
            status,
            downloaded_bytes: 0,
            total_bytes: None,
            error: None,
        }
    }
}

#[derive(Clone, Default)]
struct DownloadManager {
    inner: Arc<Mutex<HashMap<String, DownloadRecord>>>,
}

#[derive(Default)]
struct WhisperRuntime {
    current_model: Option<String>,
    context: Option<WhisperContext>,
}

#[derive(Clone, Default)]
struct WhisperManager {
    inner: Arc<Mutex<WhisperRuntime>>,
}

#[derive(Clone, Serialize)]
struct ModelStatus {
    name: String,
    size_mb: u32,
    is_downloaded: bool,
    is_downloading: bool,
    is_active: bool,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
struct DownloadEventPayload {
    #[serde(rename = "modelName")]
    model_name: String,
    #[serde(rename = "downloadedBytes")]
    downloaded_bytes: u64,
    #[serde(rename = "totalBytes")]
    total_bytes: Option<u64>,
    #[serde(rename = "percent")]
    percent: Option<f64>,
    status: &'static str,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
struct ActiveModelPayload {
    #[serde(rename = "modelName")]
    model_name: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AppConfig {
    selected_model: Option<String>,
}

fn get_config_path(app: &AppHandle) -> Result<PathBuf, String> {
    let base_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "Failed to get app data dir".to_string())?;
    fs::create_dir_all(&base_dir)
        .map_err(|e| format!("Failed to create app data directory: {}", e))?;
    Ok(base_dir.join("settings.json"))
}

fn load_app_config(app: &AppHandle) -> Result<AppConfig, String> {
    let path = get_config_path(app)?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let contents =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read app config: {}", e))?;
    serde_json::from_str(&contents).map_err(|e| format!("Failed to parse app config: {}", e))
}

fn save_app_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = get_config_path(app)?;
    let serialized = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    fs::write(&path, serialized).map_err(|e| format!("Failed to write app config: {}", e))
}

fn load_selected_model(app: &AppHandle) -> Option<String> {
    match load_app_config(app) {
        Ok(config) => config.selected_model.and_then(|name| {
            if find_model_info(&name).is_some() {
                Some(name)
            } else {
                eprintln!(
                    "Stored model '{}' is not recognized; falling back to default.",
                    name
                );
                None
            }
        }),
        Err(err) => {
            eprintln!("Failed to load app config: {}", err);
            None
        }
    }
}

fn persist_selected_model(app: &AppHandle, model_name: &str) {
    let mut config = load_app_config(app).unwrap_or_else(|err| {
        eprintln!("Failed to load existing app config: {}", err);
        AppConfig::default()
    });
    config.selected_model = Some(model_name.to_string());
    if let Err(err) = save_app_config(app, &config) {
        eprintln!("Failed to persist selected model '{}': {}", model_name, err);
    }
}

// Available Whisper models - all models from whisper.cpp repository with correct sizes
fn get_available_models() -> Vec<ModelInfo> {
    vec![
        // Tiny models
        ModelInfo { name: "tiny", filename: "ggml-tiny.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin", size_mb: 75 },
        ModelInfo { name: "tiny.en", filename: "ggml-tiny.en.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin", size_mb: 75 },
        ModelInfo { name: "tiny-q5_1", filename: "ggml-tiny-q5_1.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny-q5_1.bin", size_mb: 31 },
        ModelInfo { name: "tiny.en-q5_1", filename: "ggml-tiny.en-q5_1.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en-q5_1.bin", size_mb: 31 },
        ModelInfo { name: "tiny-q8_0", filename: "ggml-tiny-q8_0.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny-q8_0.bin", size_mb: 42 },
        ModelInfo { name: "tiny.en-q8_0", filename: "ggml-tiny.en-q8_0.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en-q8_0.bin", size_mb: 42 },

        // Base models
        ModelInfo { name: "base", filename: "ggml-base.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin", size_mb: 142 },
        ModelInfo { name: "base.en", filename: "ggml-base.en.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin", size_mb: 142 },
        ModelInfo { name: "base-q5_1", filename: "ggml-base-q5_1.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin", size_mb: 57 },
        ModelInfo { name: "base.en-q5_1", filename: "ggml-base.en-q5_1.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en-q5_1.bin", size_mb: 57 },
        ModelInfo { name: "base-q8_0", filename: "ggml-base-q8_0.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q8_0.bin", size_mb: 78 },
        ModelInfo { name: "base.en-q8_0", filename: "ggml-base.en-q8_0.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en-q8_0.bin", size_mb: 78 },

        // Small models
        ModelInfo { name: "small", filename: "ggml-small.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin", size_mb: 466 },
        ModelInfo { name: "small.en", filename: "ggml-small.en.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin", size_mb: 466 },
        ModelInfo { name: "small-q5_1", filename: "ggml-small-q5_1.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin", size_mb: 181 },
        ModelInfo { name: "small.en-q5_1", filename: "ggml-small.en-q5_1.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en-q5_1.bin", size_mb: 181 },
        ModelInfo { name: "small-q8_0", filename: "ggml-small-q8_0.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q8_0.bin", size_mb: 252 },
        ModelInfo { name: "small.en-q8_0", filename: "ggml-small.en-q8_0.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en-q8_0.bin", size_mb: 252 },

        // Medium models
        ModelInfo { name: "medium", filename: "ggml-medium.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin", size_mb: 1536 },
        ModelInfo { name: "medium.en", filename: "ggml-medium.en.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin", size_mb: 1536 },
        ModelInfo { name: "medium-q5_0", filename: "ggml-medium-q5_0.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium-q5_0.bin", size_mb: 514 },
        ModelInfo { name: "medium.en-q5_0", filename: "ggml-medium.en-q5_0.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en-q5_0.bin", size_mb: 514 },
        ModelInfo { name: "medium-q8_0", filename: "ggml-medium-q8_0.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium-q8_0.bin", size_mb: 785 },
        ModelInfo { name: "medium.en-q8_0", filename: "ggml-medium.en-q8_0.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en-q8_0.bin", size_mb: 785 },

        // Large models
        ModelInfo { name: "large-v3", filename: "ggml-large-v3.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin", size_mb: 2965 },
        ModelInfo { name: "large-v3-q5_0", filename: "ggml-large-v3-q5_0.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin", size_mb: 1126 },
        ModelInfo { name: "large-v3-turbo", filename: "ggml-large-v3-turbo.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin", size_mb: 1536 },
        ModelInfo { name: "large-v3-turbo-q5_0", filename: "ggml-large-v3-turbo-q5_0.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin", size_mb: 547 },
        ModelInfo { name: "large-v3-turbo-q8_0", filename: "ggml-large-v3-turbo-q8_0.bin", url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q8_0.bin", size_mb: 834 },
    ]
}

fn find_model_info<'a>(model_name: &str) -> Option<ModelInfo> {
    get_available_models()
        .into_iter()
        .find(|m| m.name == model_name)
}

fn emit_download_event(app: &AppHandle, payload: DownloadEventPayload) {
    let _ = app.emit("model-download-progress", payload);
}

fn spawn_model_download(
    app: &AppHandle,
    downloads: DownloadManager,
    whisper: WhisperManager,
    model_name: String,
    overwrite: bool,
) -> Result<(), String> {
    let model_info =
        find_model_info(&model_name).ok_or_else(|| "Unknown model name".to_string())?;
    let model_path = get_model_path_for(app, &model_name);

    if model_path.exists() && !overwrite {
        return Err("Model already downloaded".to_string());
    }

    {
        let mut map = downloads.inner.lock();
        if let Some(entry) = map.get(&model_name) {
            if entry.status == DownloadStatus::Downloading {
                return Err("Download already in progress".to_string());
            }
        }
        map.insert(
            model_name.clone(),
            DownloadRecord::new(DownloadStatus::Downloading),
        );
    }

    emit_download_event(
        app,
        DownloadEventPayload {
            model_name: model_name.clone(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: None,
            status: if overwrite { "refreshing" } else { "queued" },
            error: None,
        },
    );

    let app_handle = app.clone();
    let downloads_clone = downloads.clone();
    let whisper_clone = whisper.clone();

    std::thread::spawn(move || {
        download_model_task(
            app_handle,
            downloads_clone,
            whisper_clone,
            model_name,
            model_info,
            overwrite,
        );
    });

    Ok(())
}

fn download_model_task(
    app: AppHandle,
    downloads: DownloadManager,
    whisper: WhisperManager,
    model_name: String,
    model_info: ModelInfo,
    overwrite: bool,
) {
    let model_path = get_model_path_for(&app, &model_name);
    let temp_path = model_path.with_extension("download");

    let result: Result<(), Box<dyn std::error::Error>> = (|| {
        if let Some(parent) = model_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if temp_path.exists() {
            let _ = std::fs::remove_file(&temp_path);
        }

        let mut response = reqwest::blocking::get(model_info.url)?;

        if !response.status().is_success() {
            return Err(format!("Failed to download model: HTTP {}", response.status()).into());
        }

        let total_bytes = response.content_length();

        {
            let mut map = downloads.inner.lock();
            if let Some(entry) = map.get_mut(&model_name) {
                entry.status = DownloadStatus::Downloading;
                entry.downloaded_bytes = 0;
                entry.total_bytes = total_bytes;
                entry.error = None;
            }
        }

        emit_download_event(
            &app,
            DownloadEventPayload {
                model_name: model_name.clone(),
                downloaded_bytes: 0,
                total_bytes,
                percent: total_bytes.map(|_| 0.0),
                status: "started",
                error: None,
            },
        );

        let mut file = std::fs::File::create(&temp_path)?;
        let mut buffer = [0u8; 1024 * 64];
        let mut downloaded: u64 = 0;

        loop {
            let bytes_read = response.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            file.write_all(&buffer[..bytes_read])?;
            downloaded += bytes_read as u64;

            {
                let mut map = downloads.inner.lock();
                if let Some(entry) = map.get_mut(&model_name) {
                    entry.downloaded_bytes = downloaded;
                    entry.total_bytes = total_bytes;
                }
            }

            let percent = total_bytes.map(|total| {
                if total == 0 {
                    0.0
                } else {
                    (downloaded as f64 / total as f64) * 100.0
                }
            });

            emit_download_event(
                &app,
                DownloadEventPayload {
                    model_name: model_name.clone(),
                    downloaded_bytes: downloaded,
                    total_bytes,
                    percent,
                    status: "downloading",
                    error: None,
                },
            );
        }

        file.flush()?;
        file.sync_all()?;

        if overwrite && model_path.exists() {
            std::fs::remove_file(&model_path)?;
        }

        std::fs::rename(&temp_path, &model_path)?;

        Ok(())
    })();

    match result {
        Ok(()) => {
            {
                let mut map = downloads.inner.lock();
                if let Some(entry) = map.get_mut(&model_name) {
                    entry.status = DownloadStatus::Completed;
                    entry.error = None;
                }
            }

            emit_download_event(
                &app,
                DownloadEventPayload {
                    model_name: model_name.clone(),
                    downloaded_bytes: {
                        let map = downloads.inner.lock();
                        map.get(&model_name)
                            .map(|entry| entry.downloaded_bytes)
                            .unwrap_or(0)
                    },
                    total_bytes: {
                        let map = downloads.inner.lock();
                        map.get(&model_name).and_then(|entry| entry.total_bytes)
                    },
                    percent: Some(100.0),
                    status: "completed",
                    error: None,
                },
            );

            let should_reload = {
                let runtime = whisper.inner.lock();
                runtime
                    .current_model
                    .as_ref()
                    .map(|active| active == &model_name)
                    .unwrap_or(false)
            };

            if should_reload {
                match load_whisper_model_for(&app, &model_name) {
                    Ok(ctx) => {
                        let mut runtime = whisper.inner.lock();
                        runtime.context = Some(ctx);
                        runtime.current_model = Some(model_name.clone());
                        let _ = app.emit(
                            "active-model-changed",
                            ActiveModelPayload {
                                model_name: Some(model_name.clone()),
                            },
                        );
                    }
                    Err(e) => {
                        eprintln!("Failed to reload Whisper model after refresh: {}", e);
                    }
                }
            }
        }
        Err(err) => {
            let message = err.to_string();
            {
                let mut map = downloads.inner.lock();
                if let Some(entry) = map.get_mut(&model_name) {
                    entry.status = DownloadStatus::Failed;
                    entry.error = Some(message.clone());
                }
            }

            let _ = std::fs::remove_file(&temp_path);

            emit_download_event(
                &app,
                DownloadEventPayload {
                    model_name: model_name.clone(),
                    downloaded_bytes: {
                        let map = downloads.inner.lock();
                        map.get(&model_name)
                            .map(|entry| entry.downloaded_bytes)
                            .unwrap_or(0)
                    },
                    total_bytes: {
                        let map = downloads.inner.lock();
                        map.get(&model_name).and_then(|entry| entry.total_bytes)
                    },
                    percent: None,
                    status: "error",
                    error: Some(message),
                },
            );
        }
    }
}

fn gather_model_statuses(
    app: &AppHandle,
    downloads: &DownloadManager,
    whisper: &WhisperManager,
) -> Vec<ModelStatus> {
    let active_model = {
        let runtime = whisper.inner.lock();
        runtime.current_model.clone()
    };

    let download_snapshot = downloads.inner.lock().clone();

    get_available_models()
        .into_iter()
        .map(|model| {
            let model_name = model.name.to_string();
            let record = download_snapshot.get(&model_name);
            let is_downloaded = model_exists_for(app, &model_name);
            let is_downloading = record
                .map(|entry| entry.status == DownloadStatus::Downloading)
                .unwrap_or(false);
            let is_active = active_model
                .as_ref()
                .map(|current| current == &model_name)
                .unwrap_or(false);

            let (downloaded_bytes, total_bytes) = if let Some(entry) = record {
                (entry.downloaded_bytes, entry.total_bytes)
            } else if is_downloaded {
                match std::fs::metadata(get_model_path_for(app, &model_name)) {
                    Ok(meta) => {
                        let len = meta.len();
                        (len, Some(len))
                    }
                    Err(_) => (0, None),
                }
            } else {
                (0, None)
            };

            ModelStatus {
                name: model_name,
                size_mb: model.size_mb,
                is_downloaded,
                is_downloading,
                is_active,
                downloaded_bytes,
                total_bytes,
                error: record.and_then(|entry| entry.error.clone()),
            }
        })
        .collect()
}

// Get default model name
const DEFAULT_MODEL: &str = "tiny.en-q5_1";

// Get the model file path for a specific model
fn get_model_path_for(app: &AppHandle, model_name: &str) -> PathBuf {
    let models_dir = get_model_base_path(app).expect("Failed to get model base path");

    // Find the model info
    let model_info = get_available_models()
        .into_iter()
        .find(|m| m.name == model_name)
        .expect("Unknown model name");

    models_dir.join(model_info.filename)
}

fn get_model_base_path(app: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "Failed to get app data dir".to_string())?;
    let models_dir = app_data_dir.join("models");
    std::fs::create_dir_all(&models_dir)
        .map_err(|e| format!("Failed to create models directory: {}", e))?;
    Ok(models_dir)
}

// Check if a specific model exists
fn model_exists_for(app: &AppHandle, model_name: &str) -> bool {
    get_model_path_for(app, model_name).exists()
}

// Transcribe audio using Whisper model
fn transcribe_audio(
    ctx: &mut WhisperContext,
    audio_data: &[f32],
    model_name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    if audio_data.is_empty() {
        return Ok(String::new());
    }

    // Skip transcription for very short audio (< 0.3s at 16kHz)
    if audio_data.len() < 4800 {
        println!(
            "Audio too short ({} samples), skipping transcription",
            audio_data.len()
        );
        return Ok(String::new());
    }

    println!("Starting transcription of {} samples...", audio_data.len());

    // Create transcription parameters optimized for speed
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    // Only set language to English for English-only models (.en variants)
    // Multilingual models will auto-detect the language
    if model_name.contains(".en") {
        params.set_language(Some("en"));
        println!("Using English-only model - language set to 'en'");
    } else {
        println!("Using multilingual model - language auto-detection enabled");
    }
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    // Let whisper-rs handle thread count automatically based on the system
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

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

// Load Whisper model (with adaptive GPU/CPU support)
fn load_whisper_model_for(
    app: &AppHandle,
    model_name: &str,
) -> Result<WhisperContext, Box<dyn std::error::Error>> {
    let model_path = get_model_path_for(app, model_name);

    if !model_path.exists() {
        return Err("Model not found. Please download the model first.".into());
    }

    println!("Loading Whisper model from: {:?}", model_path);

    // Configure parameters based on architecture
    let params = WhisperContextParameters::default();
    
    #[cfg(target_os = "macos")]
    {
        if std::env::consts::ARCH == "aarch64" {
            println!("Detected Apple Silicon - Using Metal GPU acceleration");
            // Metal GPU is automatically enabled if available
        } else {
            // For Intel Macs, check if dedicated GPU is available
            use metal::{Device, MTLFeatureSet};
            if let Some(device) = Device::system_default() {
                println!("Detected Intel Mac with Metal GPU: {}", device.name());
                if device.supports_feature_set(MTLFeatureSet::macOS_GPUFamily1_v1) {
                    println!("Using Metal GPU acceleration with dedicated GPU");
                    // Metal GPU will be automatically enabled
                } else {
                    println!("Metal GPU feature set not supported, falling back to CPU");
                }
            } else {
                println!("No Metal GPU found on Intel Mac, using CPU optimizations");
            }
            // Thread count is managed by whisper-rs internally
        }
    }

    let ctx = WhisperContext::new_with_params(
        model_path.to_str().ok_or("Invalid model path")?,
        params,
    )?;

    println!("Whisper model loaded successfully with architecture-specific optimizations");
    Ok(ctx)
}

// Audio recording state - stores the stream and buffers audio data
struct AudioRecorder {
    stream: Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
    temp_buffer: Arc<Mutex<Vec<f32>>>, // Temporary buffer for incoming 48kHz samples
}

#[cfg(test)]
mod tests;

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

// Tauri command to get list of downloaded models
#[tauri::command]
fn get_downloaded_models(app: tauri::AppHandle) -> Vec<String> {
    get_available_models()
        .into_iter()
        .filter(|model| model_exists_for(&app, model.name))
        .map(|model| model.name.to_string())
        .collect()
}

#[tauri::command]
fn get_model_statuses(
    app: tauri::AppHandle,
    downloads: tauri::State<'_, DownloadManager>,
    whisper: tauri::State<'_, WhisperManager>,
) -> Vec<ModelStatus> {
    gather_model_statuses(&app, downloads.inner(), whisper.inner())
}

#[tauri::command]
fn start_model_download(
    app: tauri::AppHandle,
    downloads: tauri::State<'_, DownloadManager>,
    whisper: tauri::State<'_, WhisperManager>,
    model_name: String,
) -> Result<(), String> {
    spawn_model_download(
        &app,
        downloads.inner().clone(),
        whisper.inner().clone(),
        model_name,
        false,
    )
}

#[tauri::command]
fn refresh_model_download(
    app: tauri::AppHandle,
    downloads: tauri::State<'_, DownloadManager>,
    whisper: tauri::State<'_, WhisperManager>,
    model_name: String,
) -> Result<(), String> {
    // Prevent refresh while download already in progress
    {
        let map = downloads.inner().inner.lock();
        if let Some(entry) = map.get(&model_name) {
            if entry.status == DownloadStatus::Downloading {
                return Err("Download already in progress".to_string());
            }
        }
    }

    spawn_model_download(
        &app,
        downloads.inner().clone(),
        whisper.inner().clone(),
        model_name,
        true,
    )
}

#[tauri::command]
fn remove_model(
    app: tauri::AppHandle,
    downloads: tauri::State<'_, DownloadManager>,
    whisper: tauri::State<'_, WhisperManager>,
    model_name: String,
) -> Result<(), String> {
    println!("remove_model called for {}", model_name);
    let is_active = {
        let runtime = whisper.inner().inner.lock();
        runtime
            .current_model
            .as_ref()
            .map(|current| current == &model_name)
            .unwrap_or(false)
    };

    if is_active {
        println!("remove_model abort: {} is active", model_name);
        return Err(
            "Model is currently active. Switch to another model before removing.".to_string(),
        );
    }

    {
        let map = downloads.inner().inner.lock();
        if let Some(entry) = map.get(&model_name) {
            if entry.status == DownloadStatus::Downloading {
                return Err("Download in progress. Please wait for it to finish.".to_string());
            }
        }
    }

    let model_path = get_model_path_for(&app, &model_name);
    if model_path.exists() {
        std::fs::remove_file(&model_path).map_err(|e| format!("Failed to remove model: {}", e))?;
        println!("Removed file: {:?}", model_path);
    } else {
        println!("remove_model abort: file not found for {}", model_name);
        return Err("Model file not found.".to_string());
    }

    {
        let mut map = downloads.inner().inner.lock();
        map.remove(&model_name);
    }

    emit_download_event(
        &app,
        DownloadEventPayload {
            model_name: model_name.clone(),
            downloaded_bytes: 0,
            total_bytes: None,
            percent: None,
            status: "removed",
            error: None,
        },
    );

    Ok(())
}

#[tauri::command]
fn open_models_folder(app: tauri::AppHandle) -> Result<(), String> {
    let path = get_model_base_path(&app)?;

    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .arg(&path)
            .status()
            .map_err(|e| format!("Failed to open models directory: {}", e))?;
        if !status.success() {
            return Err("Failed to open models directory".into());
        }
    }

    #[cfg(target_os = "windows")]
    {
        let path_str = path
            .to_str()
            .ok_or_else(|| "Invalid models directory path".to_string())?;
        let status = std::process::Command::new("explorer")
            .arg(path_str)
            .status()
            .map_err(|e| format!("Failed to open models directory: {}", e))?;
        if !status.success() {
            return Err("Failed to open models directory".into());
        }
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let status = std::process::Command::new("xdg-open")
            .arg(&path)
            .status()
            .map_err(|e| format!("Failed to open models directory: {}", e))?;
        if !status.success() {
            return Err("Failed to open models directory".into());
        }
    }

    Ok(())
}

// Tauri command to switch Whisper model
#[tauri::command]
async fn switch_model(
    app: tauri::AppHandle,
    downloads: tauri::State<'_, DownloadManager>,
    whisper: tauri::State<'_, WhisperManager>,
    model_name: String,
) -> Result<String, String> {
    {
        let map = downloads.inner().inner.lock();
        if let Some(entry) = map.get(&model_name) {
            if entry.status == DownloadStatus::Downloading {
                return Err("Model is still downloading.".to_string());
            }
        }
    }

    if !model_exists_for(&app, &model_name) {
        return Err("Model not downloaded.".to_string());
    }

    let ctx = load_whisper_model_for(&app, &model_name).map_err(|e| e.to_string())?;

    {
        let mut runtime = whisper.inner().inner.lock();
        runtime.context = Some(ctx);
        runtime.current_model = Some(model_name.clone());
    }

    {
        let mut map = downloads.inner().inner.lock();
        if let Some(entry) = map.get_mut(&model_name) {
            entry.status = DownloadStatus::Completed;
            entry.error = None;
            if entry.total_bytes.is_none() {
                if let Ok(meta) = std::fs::metadata(get_model_path_for(&app, &model_name)) {
                    let len = meta.len();
                    entry.total_bytes = Some(len);
                    entry.downloaded_bytes = len;
                }
            }
        }
    }

    emit_download_event(
        &app,
        DownloadEventPayload {
            model_name: model_name.clone(),
            downloaded_bytes: {
                let map = downloads.inner().inner.lock();
                map.get(&model_name)
                    .map(|entry| entry.downloaded_bytes)
                    .unwrap_or(0)
            },
            total_bytes: {
                let map = downloads.inner().inner.lock();
                map.get(&model_name).and_then(|entry| entry.total_bytes)
            },
            percent: Some(100.0),
            status: "active",
            error: None,
        },
    );

    persist_selected_model(&app, &model_name);

    let _ = app.emit(
        "active-model-changed",
        ActiveModelPayload {
            model_name: Some(model_name.clone()),
        },
    );

    println!("Successfully switched to model: {}", model_name);
    Ok(format!("Model {} loaded successfully", model_name))
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

    let download_manager = DownloadManager::default();
    let whisper_manager = WhisperManager::default();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts(["alt+space", "ctrl+alt+space"])
                .expect("Failed to register shortcuts")
                .with_handler(move |app, shortcut, event| {
                    if shortcut.matches(Modifiers::ALT, Code::Space)
                        || shortcut.matches(Modifiers::ALT | Modifiers::CONTROL, Code::Space)
                    {
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
                                    println!("Option+Space released - recording stopped");

                                    // Calculate audio duration in seconds
                                    let duration_secs = if !audio_samples.is_empty() {
                                        audio_samples.len() as f32 / 16000.0 // Always 16kHz after resampling
                                    } else {
                                        0.0
                                    };

                                    // Transcribe audio using Whisper
                                    let whisper_state: tauri::State<WhisperManager> = app.state();
                                    let transcription = {
                                        let mut runtime = whisper_state.inner().inner.lock();
                                        let model_name =
                                            runtime.current_model.clone().unwrap_or_default();
                                        if let Some(ctx) = runtime.context.as_mut() {
                                            match transcribe_audio(ctx, &audio_samples, &model_name)
                                            {
                                                Ok(text) => text,
                                                Err(e) => {
                                                    eprintln!("Transcription failed: {}", e);
                                                    String::from("[Transcription failed]")
                                                }
                                            }
                                        } else {
                                            String::from("[Model not loaded]")
                                        }
                                    };

                                    // Insert transcribed text only if not empty
                                    if !transcription.is_empty()
                                        && transcription != "[Model not loaded]"
                                        && transcription != "[Transcription failed]"
                                    {
                                        match insert_text_at_cursor(app, &transcription) {
                                            Ok(_) => println!(
                                                "Inserted transcription ({:.2}s): {}",
                                                duration_secs, transcription
                                            ),
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
        .manage(download_manager.clone())
        .manage(whisper_manager.clone())
        .invoke_handler(tauri::generate_handler![
            greet,
            switch_model,
            get_downloaded_models,
            get_model_statuses,
            start_model_download,
            refresh_model_download,
            remove_model,
            open_models_folder
        ])
        .setup(|app| {
            // Hide from dock on macOS
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let download_state: tauri::State<DownloadManager> = app.state();
            let whisper_state: tauri::State<WhisperManager> = app.state();
            let app_handle = app.handle();

            let startup_model_name =
                load_selected_model(&app_handle).unwrap_or_else(|| DEFAULT_MODEL.to_string());
            let startup_path = get_model_path_for(&app_handle, &startup_model_name);
            let startup_exists = startup_path.exists();

            let model_ready = if startup_exists {
                println!("Whisper model '{}' found, loading...", startup_model_name);
                match load_whisper_model_for(&app_handle, &startup_model_name) {
                    Ok(ctx) => {
                        {
                            let mut runtime = whisper_state.inner().inner.lock();
                            runtime.context = Some(ctx);
                            runtime.current_model = Some(startup_model_name.clone());
                        }

                        {
                            let mut map = download_state.inner().inner.lock();
                            let entry = map
                                .entry(startup_model_name.clone())
                                .or_insert_with(|| DownloadRecord::new(DownloadStatus::Completed));
                            entry.status = DownloadStatus::Completed;
                            entry.error = None;
                            if let Ok(meta) = fs::metadata(&startup_path) {
                                let len = meta.len();
                                entry.downloaded_bytes = len;
                                entry.total_bytes = Some(len);
                            }
                        }

                        emit_download_event(
                            &app_handle,
                            DownloadEventPayload {
                                model_name: startup_model_name.clone(),
                                downloaded_bytes: {
                                    let map = download_state.inner().inner.lock();
                                    map.get(&startup_model_name)
                                        .map(|entry| entry.downloaded_bytes)
                                        .unwrap_or(0)
                                },
                                total_bytes: {
                                    let map = download_state.inner().inner.lock();
                                    map.get(&startup_model_name)
                                        .and_then(|entry| entry.total_bytes)
                                },
                                percent: Some(100.0),
                                status: "active",
                                error: None,
                            },
                        );

                        persist_selected_model(&app_handle, &startup_model_name);

                        let _ = app_handle.emit(
                            "active-model-changed",
                            ActiveModelPayload {
                                model_name: Some(startup_model_name.clone()),
                            },
                        );

                        println!(
                            "Whisper model '{}' initialized successfully",
                            startup_model_name
                        );
                        true
                    }
                    Err(e) => {
                        eprintln!(
                            "Failed to load Whisper model '{}': {}",
                            startup_model_name, e
                        );
                        false
                    }
                }
            } else {
                println!(
                    "Whisper model '{}' not found at: {:?}. Starting download...",
                    startup_model_name, startup_path
                );

                {
                    let mut runtime = whisper_state.inner().inner.lock();
                    runtime.current_model = Some(startup_model_name.clone());
                    runtime.context = None;
                }

                persist_selected_model(&app_handle, &startup_model_name);

                if let Err(err) = spawn_model_download(
                    &app_handle,
                    download_state.inner().clone(),
                    whisper_state.inner().clone(),
                    startup_model_name.clone(),
                    false,
                ) {
                    eprintln!(
                        "Failed to start download for '{}': {}",
                        startup_model_name, err
                    );
                }

                false
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

            // Start periodic model check to ensure recommended model is always available
            let app_handle_for_check = app_handle.clone();
            let download_state_for_check = download_state.inner().clone();
            let whisper_state_for_check = whisper_state.inner().clone();
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(30));

                    let default_path = get_model_path_for(&app_handle_for_check, DEFAULT_MODEL);

                    // Check if default model exists
                    if !default_path.exists() {
                        println!("Recommended model missing, starting automatic download...");

                        // Check if not already downloading
                        let already_downloading = {
                            let map = download_state_for_check.inner.lock();
                            map.get(DEFAULT_MODEL)
                                .map(|entry| entry.status == DownloadStatus::Downloading)
                                .unwrap_or(false)
                        };

                        if !already_downloading {
                            if let Err(err) = spawn_model_download(
                                &app_handle_for_check,
                                download_state_for_check.clone(),
                                whisper_state_for_check.clone(),
                                DEFAULT_MODEL.to_string(),
                                false,
                            ) {
                                eprintln!("Failed to auto-download missing model: {}", err);
                            } else {
                                println!("Automatic download of recommended model started");
                            }
                        }
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
