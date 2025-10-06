<div align="center">
  <img src="src-tauri/icons/Sotto Logo.png" alt="Sotto Logo" width="128" height="128">
  <h1>Sotto</h1>
  <p>Open-source voice transcription app for macOS.</p>
</div>

## Installation

### Homebrew (Recommended)

```bash
brew install gevorggalstyan/tap/sotto
```

### Manual Installation

1. Download the latest DMG from [Releases](https://github.com/gevorggalstyan/sotto/releases)
2. Open the DMG and drag Sotto.app to your Applications folder
3. Launch Sotto from Applications

## What is Sotto?

Sotto is a lightweight, system-wide voice transcription tool that runs in your macOS menu bar. It allows you to transcribe speech to text anywhere on your system using simple keyboard shortcuts.

The name "Sotto" comes from the Italian musical term "sotto voce," meaning "under the voice" or spoken in a quiet, soft manner - reflecting the app's unobtrusive, always-available nature.

## Features

- **System-wide accessibility**: Works in any application across macOS
- **Multiple hotkey support**: Use either `Option+Space` or `Ctrl+Option+Space` to trigger recording
- **Local transcription**: Uses Whisper AI models running locally on your Mac with Metal GPU acceleration
- **Multiple model options**: Choose from various Whisper models (tiny, base, small, medium, large) with different speed/accuracy trade-offs
- **Multilingual support**: Models without ".en" suffix support transcription from multiple languages, automatically translating to English
- **Automatic text insertion**: Transcribed text is automatically inserted at your cursor position
- **Menu bar integration**: Lives in your menu bar for easy access to settings

## How It Works

1. Press and hold `Option+Space` or `Ctrl+Option+Space`
2. Speak while holding the hotkey
3. Release the hotkey to stop recording
4. The transcribed text is automatically inserted at your cursor position

The app icon in the menu bar changes to indicate when it's actively recording.

## Hotkeys

- **Option+Space**: Start/stop voice recording
- **Ctrl+Option+Space**: Alternative hotkey for the same functionality

Both hotkeys trigger the same recording functionality. Press and hold to record, release to transcribe and insert text.

## Models

Sotto supports multiple Whisper models that can be downloaded on-demand:

- **Tiny**: Fastest, smallest (31-75 MB)
- **Base**: Balanced speed/accuracy (57-142 MB)
- **Small**: Better accuracy (181-466 MB)
- **Medium**: High accuracy (514-1536 MB)
- **Large**: Best accuracy (547-2965 MB)

Models are available in both multilingual and English-only (.en) variants, as well as quantized versions for smaller file sizes.

## Requirements

- macOS (with Metal GPU support for optimal performance)
- Microphone access permission
- Accessibility permissions (for automatic text insertion)

## Technical Stack

- **Frontend**: TypeScript + Vite
- **Backend**: Rust + Tauri 2
- **AI Model**: Whisper (via whisper-rs with Metal acceleration)
- **Audio**: cpal for cross-platform audio recording

## Development

### Installation & Launch

```bash
# Install dependencies
npm install

# Run in development mode
npm run tauri dev
```

### Build

```bash
# Build macOS app bundle
npm run build:app

# Build DMG installer
npm run build:dmg
```

## License

Open source - see repository for license details.
