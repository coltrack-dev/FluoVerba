# AGENTS.md — FluoVerba

## Mission
FluoVerba is a lightweight offline desktop language-learning ticker. It shows one foreign-language phrase and its translation in two synchronized lines, moving together horizontally. The first target is Spanish → Russian. Windows and Linux are target platforms.

## Stack and boundaries
- Rust + Slint. No Vue, Tauri, Electron, embedded browser, HTTP service, telemetry, or cloud dependency.
- Keep dependencies minimal; add a crate only with a brief justification.
- Do not add SQLite until the JSON/import and persistence requirements make it necessary.
- Keep the ticker UI independent of phrase storage and scheduling logic.
- Do not modify unrelated files or reformat existing comments/code without need.

## Non-negotiable behavior
- Both language lines share ONE horizontal position/progress and remain aligned throughout their movement.
- Animation must not spawn polling threads or busy loops. Stop unnecessary work when paused or hidden.
- Support Unicode text and preserve original punctuation and accents.
- Offline-first; never upload text or usage data.
- Keep the main window simple; translucent, click-through, tray and always-on-top behavior are separate OS-specific milestones, not assumed to work on Wayland.

## Development workflow
1. Read this file and README.md before changes.
2. Make small, reviewable changes. Explain architecture decisions before adding substantial dependencies.
3. Check `cargo fmt --all -- --check` and `cargo check` if local dependencies are available.
4. For GUI changes, request visual verification on Windows and Linux; do not claim it was performed if the environment cannot show windows.
5. Report executed checks, their outcomes, and any blocked checks.
6. Do NOT commit, push, create tags, or modify git remotes without explicit user confirmation.
7. Do NOT fetch dependencies or execute external downloads automatically in restricted/offline environments; ask first if packages are unavailable.

## Initial milestones
- M0: minimal project skeleton, two synchronized lines, five bundled phrases, launch instructions.
- M1: JSON file loading, UTF-8 validation, malformed-record handling, deterministic phrase scheduling.
- M2: controls for direction, speed, pause, font size, colors, and display position.
- M3: assess transparent/frameless/click-through/always-on-top behavior individually on Windows, Linux X11, and Wayland.
- M4: import Tatoeba pairs with attribution/licensing information, filtered for short phrases.
- M5: lightweight performance measurements in release builds (CPU, RAM, animation smoothness).

## Acceptance criteria for M0
- `cargo run` opens a window showing Spanish above Russian.
- Each phrase pair moves horizontally with one shared progress value; pairs change automatically.
- Rust 1.92+ and Slint 1.18.1; compatible versions must be verified against installed toolchains.
- The app needs no server and does not require internet at runtime.
- Missing build/GUI validation is explicitly disclosed.
