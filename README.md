# FluoVerba

Offline Spanish → Russian ticker built with Rust and Slint. The two lines share one horizontal position and progress value. The same Slint UI and SQLite layer are used by the Windows/Linux desktop app and the Android ARM64 app.

## Features and data

- Desktop CLI: `import` and `stats`; the desktop app reads SQLite from the standard per-user data directory.
- Android: the prebuilt dictionary database is bundled in the APK and copied once into the app's private internal data directory on first launch. Later launches reuse that database.
- The GUI keeps up to 256 phrase pairs in memory and reads the next page only when needed. Animation frames do not query SQLite.
- The two-line strip height follows the rendered text metrics: Spanish 18sp, Russian 16sp, 4dp between lines, and 6dp vertical padding. Android's system font scale is applied. The text moves from just beyond the right edge to beyond the left edge on one shared rail. Speed is adjustable from 5 to 120 dp/s (default 20 dp/s); changing it preserves the current progress.
- On Android the strip can be dragged vertically inside the Activity surface; its last position is saved to a small sidecar settings file next to the per-user database, without writing to SQLite. A compact settings panel includes an opt-in Keep Screen On switch, implemented with the visible Activity's `KEEP_SCREEN_ON` window flag. It is off by default.
- Runtime diagnostics report Slint's post-render callback rate and mean/max render interval, plus UI update callback cadence, every five seconds. The notifier runs after Slint renders but before the backend presents the buffer, so it measures rendered-frame cadence rather than display scanout/present timing.
- Topics and the many-to-many `sentence_topics` relation are available for future classification; imports do not assign topics.
- No server, WebView, telemetry, cloud service, or runtime network access. The current Android NativeActivity is a regular full-screen Activity surface; the drawn ticker strip is compact and movable within it. It is not a floating system Overlay.

## Desktop requirements and run

- Rust 1.92+ and Slint 1.18.1
- Windows desktop or Linux graphical session and the selected Slint backend's system libraries

```bash
cargo run
cargo run -- stats
cargo run -- import
cargo run -- import /path/to/spa-rus.tsv
```

The source TSV is kept separately at `data/imports/spa-rus.tsv` and is ignored by Git. It has no header and four tab-separated columns: Spanish Tatoeba ID, Spanish text, Russian Tatoeba ID, Russian text. Import streams rows in transactions of 5,000. Re-importing is safe. Desktop SQLite files are stored under `%APPDATA%/FluoVerba` on Windows, `~/Library/Application Support/FluoVerba` on macOS, or `$XDG_DATA_HOME/FluoVerba` / `~/.local/share/FluoVerba` on Linux.

## Android ARM64 build and install

The Android target uses Slint's `backend-android-activity-06`, a Rust `cdylib`, and [cargo-apk2](https://github.com/slint-ui/cargo-apk2). The app itself keeps the Rust 1.92+ baseline; cargo-apk2 1.5.0 currently declares Rust 1.95+ as its MSRV. Install Rust's Android target and cargo-apk2, and configure Android SDK/NDK plus a JDK:

```bash
rustup target add aarch64-linux-android
cargo install cargo-apk2
export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_NDK_ROOT="$ANDROID_HOME/ndk/<installed-version>"
export PATH="$ANDROID_HOME/platform-tools:$PATH"
```

Connect an ARM64 device with USB debugging enabled. Build, install, and launch with:

```bash
cargo apk2 build --release --target aarch64-linux-android --lib
cargo apk2 run --target aarch64-linux-android --lib
```

The Cargo Android metadata packages `android/assets/fluoverba.sqlite3` as an APK asset. On first launch the app copies it atomically to Android's private internal data directory. The checked-in 25 MB SQLite asset is a prepared Tatoeba snapshot (136,109 Spanish–Russian links); the original TSV is not included in Git. To refresh the asset from an available TSV:

```bash
mkdir -p /tmp/fluoverba-seed
XDG_DATA_HOME=/tmp/fluoverba-seed cargo run -- import /path/to/spa-rus.tsv
cp /tmp/fluoverba-seed/FluoVerba/fluoverba.sqlite3 android/assets/fluoverba.sqlite3
```

## Data attribution

Sentence text comes from [Tatoeba](https://tatoeba.org/) and is licensed under [Creative Commons Attribution 2.0 France (CC BY 2.0 FR)](https://creativecommons.org/licenses/by/2.0/fr/). The database records the source and license; Tatoeba sentence IDs are retained. The source TSV format does not contain individual contributor names.

## Development checks

```bash
cargo fmt --all -- --check
cargo check
cargo test
cargo check --target aarch64-linux-android
```

An Android APK also requires the Android Rust target, NDK, SDK, Java, and cargo-apk2. Building the target library alone does not validate installation or behavior on a physical Android device.

## Animation diagnostics and Android Overlay status

The ticker integrates elapsed monotonic time at the configured dp/s over the actual travel distance (screen width plus the widest of the two text lines). It does not advance by a fixed number of pixels per timer tick. Slint linearly animates progress between timestamp-derived positions on one shared timeline. The UI timer schedules updates at 16 ms, while the distance calculation uses the actual elapsed interval so scheduling jitter does not change the average speed. SQLite is consulted only when a phrase batch is exhausted, never for an animation frame.

`cargo test` includes deterministic kinematics checks at 10, 20, 40, and 80 dp/s and checks that a speed change alone leaves progress unchanged. Render callback diagnostics and these kinematic checks do not replace visual smoothness testing on Linux/Android hardware or measure display presentation timing.

Slider diagnostics log callback entry/exit, Rust thread ID, value and callback duration. For an A/B Android build with the speed state update disabled (while the Slider and logging remain active), use `cargo apk2 build --release --target aarch64-linux-android --lib --features disable-speed-handler`. This produces the same APK path as the regular build; install/run the variants separately when comparing on a device.

The `minimal-settings-close` Android diagnostic feature keeps the existing window and animation alive and makes the close branch hide only the settings panel. It logs `SETTINGS_CLOSE_ENTER`, `SETTINGS_VISIBILITY_UPDATED`, and `SETTINGS_CLOSE_EXIT` with thread ID and elapsed time. Build it with `cargo apk2 build --release --target aarch64-linux-android --lib --features minimal-settings-close`.

The current `backend-android-activity-06` backend creates and renders through Slint's NativeActivity/ANativeWindow adapter. It does not expose a Slint renderer as an embeddable Android `View` for a `WindowManager` `TYPE_APPLICATION_OVERLAY`. A real Overlay therefore needs a small Kotlin/Java host that requests `SYSTEM_ALERT_WINDOW`, owns/removes the overlay view, and handles explicit consent, drag/touch, hide and close actions; Rust can remain responsible for phrase scheduling and storage, but integrating Slint rendering into that view needs a supported embedding backend or a verified custom surface bridge. No Overlay permission is requested, no Accessibility Service is used, and no Overlay implementation is included in this task.
