# Sotto - Voice Transcription App

## Project Overview

Sotto is an open-source, system-wide voice transcription application for macOS. It runs in the menu bar and allows users to transcribe speech to text in any application using keyboard shortcuts. The application leverages local Whisper AI models with Metal GPU acceleration for fast and accurate transcription.

**Key Technologies:**

*   **Frontend:** TypeScript, Vite
*   **Backend:** Rust, Tauri 2
*   **AI Model:** [Whisper](https://openai.com/research/whisper) (via [whisper-rs](https://github.com/tazz4843/whisper-rs))
*   **Audio Processing:** [cpal](https://github.com/RustAudio/cpal) for audio input

**Architecture:**

The application consists of a TypeScript frontend for the user interface (settings, model selection) and a Rust backend that handles audio recording, transcription, and system integration. Tauri is used to bundle the web frontend into a native macOS application.

## Building and Running

**1. Install Dependencies:**

```bash
npm install
```

**2. Run in Development Mode:**

This command will start the Vite development server for the frontend and run the Tauri application, enabling hot-reloading for both.

```bash
npm run tauri dev
```

**3. Build the Application:**

To build the final macOS application bundle (`.app`):

```bash
npm run build:app
```

To create a distributable DMG installer:

```bash
npm run build:dmg
```

## Development Conventions

*   **Frontend:** The frontend code is located in the `src` directory and follows standard TypeScript and Vite conventions.
*   **Backend:** The backend Rust code is in the `src-tauri` directory. The main application logic is in `src-tauri/src/main.rs`.
*   **Dependencies:** Frontend dependencies are managed with `npm` in `package.json`. Backend Rust dependencies (crates) are managed with Cargo in `src-tauri/Cargo.toml`.
*   **API:** The frontend and backend communicate via the Tauri API.
*   **Testing:** No explicit testing framework is configured in the provided files.
