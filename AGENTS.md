# AGENTS.md — FluoVerba

## Mission
FluoVerba is a lightweight offline Spanish → Russian ticker. It shows one phrase pair in two synchronized lines, moving together horizontally. Supported targets are Windows, Linux, and Android ARM64.

## Stack and boundaries
- Rust + Slint. Android uses Slint's `backend-android-activity-06` backend.
- Shared application and SQLite logic lives in the Rust library crate (`rlib` + Android `cdylib`); the desktop executable keeps its own `main()` and CLI.
- Do not add Flutter, Tauri, Electron, WebView, HTTP services, telemetry, or cloud dependencies.
- SQLite is the established persistence layer for imports and phrase storage; do not add another database.
- Keep the ticker UI independent from storage and scheduling logic.
- Keep dependencies minimal and explain any substantial addition before implementing it.

## Non-negotiable behavior
- Both language lines use ONE horizontal position/progress and remain aligned throughout movement.
- Movement integrates monotonic elapsed time in dp/sec over the actual route (viewport width plus the widest line); both rows share one rail coordinate. Never use a fixed pixel increment per timer tick.
- Animation must not spawn polling threads or busy loops. Keep frame work small, avoid database access during animation frames, and stop unnecessary work when paused or hidden.
- The ticker strip height follows text metrics (18sp Spanish, 16sp Russian, 4dp line gap, 6dp vertical padding). Android's Activity surface may fill the screen, but the strip itself must not. Respect Android's configured font scale.
- Android Activity drag gestures move the strip within the Activity surface and save its last vertical position in a sidecar settings file, never the phrase database. A true system Overlay is a separate feature and must not be described as provided by NativeActivity.
- Keep Screen On is opt-in and must use the visible Activity window flag only; do not use persistent wakelocks or keep the display on by default.
- Support Unicode text and preserve original punctuation and accents.
- Offline-first; never upload text or usage data.
- Android SQLite data must be inside the app's private internal data directory. The prepared dictionary may ship as a packaged APK asset and must be initialized/copied only once.
- System Overlay is not implemented. Desktop translucency, click-through, tray and always-on-top remain separate OS-specific milestones. Overlay work requires explicit user permission, correct `SYSTEM_ALERT_WINDOW` consent and touch/close behavior, and a verified embedding approach; no Accessibility Service.

## Development workflow
1. Read this file and `README.md` before changes.
2. Make small, reviewable changes. Do not reformat unrelated code.
3. Run `cargo fmt --all -- --check`, `cargo check`, and `cargo test` when dependencies are locally available.
4. Check Android ARM64 with `cargo check --target aarch64-linux-android` when the Rust target and Android NDK are installed. Do not fetch Rust targets, crates, or tools automatically in restricted/offline environments.
5. For GUI changes, request visual verification on Windows, Linux, and Android where those environments are available. Never claim visual/device testing that was not performed.
6. Do NOT commit, push, create tags, or modify Git remotes without explicit user confirmation.
7. Report checks, outcomes, and blocked platform validation.

## Git Policy

- Codex MAY create git commits when explicitly requested by the user.
- Codex MUST NOT create commits without an explicit user request.
- Codex MUST NOT push to remote repositories without separate explicit authorization.
- Codex MAY stage files required for an explicitly requested commit.
- Before committing, review git status and git diff.
- Do not modify Git history using reset --hard, rebase, or force push without explicit authorization.
- If .git is read-only, report the restriction instead of attempting unsafe workarounds.

## Data and attribution
- The source Tatoeba TSV lives at `data/imports/spa-rus.tsv` and is excluded from Git.
- Android packages the prepared database at `android/assets/fluoverba.sqlite3`; refresh it only from an available, licensed source dataset.
- Preserve original Tatoeba global IDs and source/license metadata. Tatoeba sentence data is attributed to Tatoeba under CC BY 2.0 FR.
- The schema supports multiple topics per sentence; the importer does not require topic labels.

## Milestones
- M0: minimal project skeleton, synchronized lines, bundled phrases and launch instructions.
- M1: TSV loading, validation, deterministic scheduling and SQLite persistence.
- M2: direction, speed, pause, font, color and display controls.
- M3: assess desktop transparency, frameless, click-through, tray and always-on-top behavior separately on Windows, Linux X11 and Wayland.
- M4: Tatoeba source attribution, license handling and language-pair import.
- M5: release CPU, RAM and animation measurements.
- M6: Android ARM64 native Slint backend, private SQLite storage and packaged dictionary asset. Android Overlay remains deferred.

## Acceptance criteria
- `cargo run` starts the desktop UI after a dictionary has been imported.
- Desktop import/statistics CLI remains available.
- Android uses `android_main()` and Slint's Android backend in a `cdylib`, with a private database seeded once from the APK asset.
- Rust 1.92+ and Slint 1.18.1 remain the application baseline; document any newer tool requirements such as cargo-apk2.
- Windows/Linux desktop behavior remains supported, and the application needs no server or runtime internet access.
- Missing build, GUI, or device validation is explicitly disclosed.
