# Repository Guidelines

## Project Structure & Module Organization
Sotto combines a Vite front end with a Tauri backend. UI code lives in `src/`: `src/main.ts` is the entry point, shared styles sit in `src/styles.css`, and assets live under `src/assets/`. Place web tests in `src/__tests__/`. Native logic lives in `src-tauri/`: `src-tauri/src/lib.rs` handles audio capture, Whisper management, and Tauri commands, while `src-tauri/src/main.rs` boots the app. Desktop metadata sits in `src-tauri/tauri.conf.json`, with icons in `src-tauri/icons/` and platform folders like `src-tauri/macos/`. Build outputs land in `dist/` (web) and `src-tauri/target/` (Rust); Whisper model downloads stay outside the repo.

## Build, Test, and Development Commands
- `npm install` syncs Node dependencies; rerun after `package.json` updates.
- `npm run dev` starts the Vite dev server.
- `npm run tauri dev` boots the native shell with tray integration.
- `npm run build` type-checks with `tsc` and bundles for production.
- `npm run build:app` and `npm run build:dmg` create distributable binaries.
- In `src-tauri/`, run `cargo fmt`, `cargo check`, and `cargo test` to keep the Rust backend healthy.

## Coding Style & Naming Conventions
Use two-space indentation in TypeScript and prefer single-quote imports. Keep CSS class names kebab-case in `src/styles.css` and colocate helpers with the components they serve. Run `rustfmt` (via `cargo fmt`) for backend formatting, favor snake_case functions, PascalCase types, and register new commands with `#[tauri::command]` near the bottom of `src-tauri/src/lib.rs`.

## Testing Guidelines
Add front-end unit tests with Vitest under `src/__tests__/`, naming files after the component or helper under test. For Whisper integrations, add Rust tests in the same module behind `#[cfg(test)]`. Before reviews, run `cargo test`, `cargo check`, and exercise critical flows with `npm run tauri dev` (record audio, download a model).

## Commit & Pull Request Guidelines
Write concise, imperative commit titles (e.g., `Add macOS bundle metadata`). Reference issues in the description when applicable. Pull requests should summarize changes, list validation steps (tests run, manual checks), and include screenshots or screen recordings for UI updates. Call out new commands, assets, or configuration updates so reviewers can verify them.

## Security & Configuration Tips
Keep secrets and signing assets out of source control; rely on platform keychains or Tauri secure storage. Review `tauri.conf.json` before enabling new plugins to avoid unnecessary entitlements. Confirm Whisper models download to the per-user app data directory and remain ignored by Git.
