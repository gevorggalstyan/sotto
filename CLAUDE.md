# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Sotto is a macOS menu bar voice transcription app that uses Whisper AI models for local, system-wide speech-to-text. It runs as a background service with global hotkey support (`Option+Space` or `Ctrl+Option+Space`).

## Development Commands

```bash
# Install dependencies
npm install

# Run in development mode (launches both Vite dev server and Tauri)
npm run tauri dev

# Build TypeScript and frontend
npm run build

# Build macOS app bundle
npm run build:app

# Build DMG installer
npm run build:dmg
```

## Architecture

### Tauri Hybrid Structure

Sotto uses **Tauri 2** - a Rust + TypeScript hybrid app:
- **Frontend**: TypeScript/Vite (settings UI shown on-demand)
- **Backend**: Rust (audio recording, transcription, system integration)
- Communication via Tauri's IPC commands and events

### Key Components

**Rust Backend (`src-tauri/src/lib.rs`)**
- `WhisperManager`: Manages loaded Whisper model context and current active model
- `DownloadManager`: Tracks model download progress and states
- `AudioRecorder`: Handles microphone capture via cpal, resamples audio to 16kHz for Whisper
- Global shortcut handler: Registered via `tauri_plugin_global_shortcut`, triggers on key press/release
- Model management: Downloads models from HuggingFace, stores in app data directory
- Text insertion: Uses clipboard manipulation + keyboard simulation (enigo) to paste transcribed text

**Frontend (`src/main.ts`)**
- Tab-based settings UI (Models, About)
- Model catalog: Cards for each available Whisper model with download/use/refresh/remove actions
- Real-time download progress via Tauri events (`model-download-progress`, `active-model-changed`)
- State synchronization: Polls backend for model statuses, updates UI accordingly

### Hotkey Flow

1. User presses `Option+Space` or `Ctrl+Option+Space`
2. Shortcut handler in Rust detects press → starts audio recording, changes tray icon to "active"
3. User speaks while holding keys
4. User releases keys → stops recording, gets buffered audio samples
5. Backend transcribes audio using loaded Whisper model (with Metal GPU acceleration)
6. Transcribed text is inserted at cursor position via clipboard paste simulation
7. Tray icon reverts to default state

### Model System

- **Available models**: Defined in `get_available_models()` (tiny, base, small, medium, large variants)
- **Multilingual vs English-only**: Models without `.en` suffix support multiple languages with auto-translation to English
- **On-demand downloads**: Models downloaded from HuggingFace on first use or via settings UI
- **Active model**: Stored in `WhisperManager`, persisted to localStorage in frontend
- **Default model**: `tiny.en-q5_1` (fast, small, English-only)

### Audio Pipeline

1. **Capture**: cpal opens default input device (attempts 16kHz, falls back to 48kHz)
2. **Resampling**: If recorded at 48kHz, decimates to 16kHz in real-time (every 3rd sample)
3. **Buffering**: Samples accumulated in `Arc<Mutex<Vec<f32>>>`
4. **Transcription**: Passed to `whisper-rs` with Metal feature enabled
5. **Output**: Text returned from Whisper, filtered for empty/error cases

### Tauri Commands

Backend exposes these IPC commands to frontend:
- `get_model_statuses`: Returns all models with download/active status
- `start_model_download`: Begins downloading a model
- `refresh_model_download`: Re-downloads existing model (overwrites)
- `remove_model`: Deletes model file (prevented if active)
- `switch_model`: Loads a different model into WhisperManager
- `open_models_folder`: Opens models directory in Finder

### Frontend State Management

- **Model state**: Maintained per-model in `Map<string, ModelState>` (download progress, active status, errors)
- **UI updates**: Driven by backend events and periodic polling via `get_model_statuses`
- **Local persistence**: Active model stored in localStorage, restored on app launch
- **UI conventions**: Cards show buttons contextually (Use/Download/Refresh/Remove based on state)

## Important Notes

- **macOS-specific**: Uses Metal GPU for Whisper, macOS-specific tray icons and activation policy
- **Permissions required**: Microphone access (for recording), Accessibility (for text insertion via keyboard simulation)
- **Window behavior**: Main window hidden by default (menu bar app), shows on "Settings" menu click
- **Clipboard handling**: Saves/restores clipboard around paste operation to avoid data loss
- **Error handling**: Transcription failures (empty audio, model errors) don't insert text, log errors instead

## Git Commit and Versioning Guidelines

### Semantic Versioning (SemVer)
Follow semantic versioning: `MAJOR.MINOR.PATCH` (e.g., 1.2.3)

- **MAJOR** (X.0.0): Breaking changes or major architectural changes
- **MINOR** (0.X.0): New features, functionality additions (backward compatible)
- **PATCH** (0.0.X): Bug fixes, minor improvements, documentation updates (backward compatible)

### When to Bump Versions
- **New feature**: Bump MINOR version (0.1.x → 0.2.0)
- **Bug fix**: Bump PATCH version (0.1.1 → 0.1.2)
- **Breaking change**: Bump MAJOR version (0.9.x → 1.0.0)
- **Documentation only**: Bump PATCH version (0.1.1 → 0.1.2)
- **Dependency updates**: Bump PATCH version (0.1.1 → 0.1.2)

### Commit Message Format
Follow conventional commit style:

```
<type>: <subject>

<body (optional)>
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `refactor`: Code refactoring without feature changes
- `perf`: Performance improvements
- `test`: Adding or updating tests
- `chore`: Maintenance tasks (dependencies, config)

**Examples:**
- `feat: Add automatic model recovery system`
- `fix: Correct icon path in About page`
- `chore: Update dependencies to latest versions`
- `docs: Add troubleshooting guide for microphone issues`

### Commit Guidelines
- **Never include attribution**: Do not add notes indicating commits were generated by Claude Code or AI
- **Keep commits clean**: Write standard, professional commit messages
- **Be descriptive**: Explain what changed and why
- **Use imperative mood**: "Add feature" not "Added feature"
