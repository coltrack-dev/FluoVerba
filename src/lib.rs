pub mod storage;

use std::{
    cell::RefCell,
    error::Error,
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};

slint::include_modules!();

pub fn run_gui_at(database: &Path) -> Result<(), Box<dyn Error>> {
    run_gui_with_keep_screen_callback(database, |_| {})
}

pub fn run_gui_with_keep_screen_callback(
    database: &Path,
    keep_screen_callback: impl Fn(bool) + 'static,
) -> Result<(), Box<dyn Error>> {
    if !storage::database_exists(database) {
        return Err(format!(
            "database not found at {}; import a dictionary before starting FluoVerba",
            database.display()
        )
        .into());
    }

    let connection = storage::open_database(database)?;
    let mut batch = storage::gui_pairs_after(&connection, None)?;
    let mut cursor = batch.last().map(|item| item.0);
    let mut phrases: Vec<_> = batch.drain(..).map(|item| (item.1, item.2)).collect();

    let window = MainWindow::new()?;
    let weak = window.as_weak();
    let timer = slint::Timer::default();
    let position_file = database.with_extension("ticker-position");
    if let Ok(position) = std::fs::read_to_string(&position_file) {
        if let Ok(position) = position.trim().parse::<f32>() {
            if position.is_finite() && position >= 0.0 {
                window.set_belt_y(position);
            }
        }
    }
    window.on_belt_position_saved(move |position| {
        let temporary = position_file.with_extension("ticker-position.tmp");
        #[cfg(target_os = "windows")]
        if position_file.exists() {
            if let Err(error) = std::fs::remove_file(&position_file) {
                eprintln!("Could not replace saved ticker position: {error}");
                return;
            }
        }
        let result = std::fs::write(&temporary, position.to_string())
            .and_then(|()| std::fs::rename(&temporary, &position_file));
        if let Err(error) = result {
            eprintln!("Could not save ticker position: {error}");
            let _ = std::fs::remove_file(temporary);
        }
    });
    window.set_spanish(phrases[0].0.as_str().into());
    window.set_russian(phrases[0].1.as_str().into());
    let mut pair_index = 0usize;
    let mut progress = 0.0_f32;
    let controls = Rc::new(RefCell::new(SettingsState::default()));
    let render_metrics = Rc::new(RefCell::new(RenderMetrics::default()));
    window.window().set_rendering_notifier({
        let metrics = render_metrics.clone();
        move |state, _graphics_api| {
            if matches!(state, slint::RenderingState::AfterRendering) {
                let now = Instant::now();
                let mut metrics = metrics.borrow_mut();
                if let Some(previous) = metrics.last_render {
                    let interval = now.saturating_duration_since(previous);
                    metrics.frames += 1;
                    metrics.total_frame_time += interval;
                    metrics.max_frame_time = metrics.max_frame_time.max(interval);
                }
                metrics.last_render = Some(now);
            }
        }
    })?;
    let mut previous_tick = Instant::now();
    let mut diagnostic_started = previous_tick;
    let mut diagnostic_frames = 0_u32;
    let mut diagnostic_total_frame_time = Duration::ZERO;
    let mut diagnostic_max_frame_time = Duration::ZERO;

    window.on_toggle_settings_requested({
        let controls = controls.clone();
        let weak = weak.clone();
        move || {
            let callback_started = Instant::now();
            let close_requested_at = callback_started;
            let thread_id = std::thread::current().id();
            eprintln!(
                "FluoVerba settings toggle callback begin: thread={thread_id:?}"
            );

            // Defer conditional panel creation/destruction until Slint finishes dispatching
            // the menu button's pointer event. This changes visibility only; no Android API,
            // animation timer, database, or main-window lifecycle operation is involved.
            slint::Timer::single_shot(Duration::ZERO, {
                let controls = controls.clone();
                let weak = weak.clone();
                move || {
                    let action_started = Instant::now();
                    let thread_id = std::thread::current().id();
                    if let Some(ui) = weak.upgrade() {
                        let was_open = ui.get_settings_open();
                        if was_open {
                            eprintln!(
                                "SETTINGS_CLOSE_ENTER thread={thread_id:?} elapsed_ms={:.3}",
                                close_requested_at.elapsed().as_secs_f64() * 1000.0
                            );

                            #[cfg(feature = "minimal-settings-close")]
                            {
                                // Diagnostic variant: only synchronize the panel visibility
                                // state and hide it. The animation timer, speed, Android window,
                                // Activity, and MainWindow lifecycle are untouched.
                                controls.borrow_mut().settings_open = false;
                                ui.set_settings_open(false);
                            }
                            #[cfg(not(feature = "minimal-settings-close"))]
                            {
                                let next_visibility = controls.borrow_mut().toggle_settings();
                                ui.set_settings_open(next_visibility);
                            }

                            eprintln!(
                                "SETTINGS_VISIBILITY_UPDATED thread={thread_id:?} visible=false elapsed_ms={:.3}",
                                close_requested_at.elapsed().as_secs_f64() * 1000.0
                            );
                            eprintln!(
                                "SETTINGS_CLOSE_EXIT thread={thread_id:?} elapsed_ms={:.3} action_ms={:.3}",
                                close_requested_at.elapsed().as_secs_f64() * 1000.0,
                                action_started.elapsed().as_secs_f64() * 1000.0
                            );
                        } else {
                            let next_visibility = controls.borrow_mut().toggle_settings();
                            ui.set_settings_open(next_visibility);
                            eprintln!(
                                "FluoVerba settings visibility opened: thread={thread_id:?}, elapsed_ms={:.3}",
                                close_requested_at.elapsed().as_secs_f64() * 1000.0
                            );
                        }
                    } else {
                        eprintln!(
                            "FluoVerba settings visibility skipped: main window already gone; thread={thread_id:?}"
                        );
                    }
                    eprintln!(
                        "FluoVerba settings visibility action end: thread={thread_id:?}, duration={:.3} ms",
                        action_started.elapsed().as_secs_f64() * 1000.0
                    );
                }
            });
            eprintln!(
                "FluoVerba settings toggle callback end: thread={thread_id:?}, duration={:.3} ms; visibility queued only",
                callback_started.elapsed().as_secs_f64() * 1000.0
            );
        }
    });

    window.on_speed_changed({
        let controls = controls.clone();
        move |new_speed| {
            let callback_started = Instant::now();
            let thread_id = std::thread::current().id();
            eprintln!(
                "FluoVerba speed callback begin: thread={thread_id:?}, value={new_speed:.2}, handler_enabled={}",
                !cfg!(feature = "disable-speed-handler")
            );
            #[cfg(not(feature = "disable-speed-handler"))]
            controls.borrow_mut().set_speed(new_speed);
            #[cfg(feature = "disable-speed-handler")]
            let _ = (&controls, new_speed);
            eprintln!(
                "FluoVerba speed callback end: thread={thread_id:?}, duration={:.3} ms",
                callback_started.elapsed().as_secs_f64() * 1000.0
            );
        }
    });
    window.on_keep_screen_changed(move |keep_screen_on| {
        let callback_started = Instant::now();
        let thread_id = std::thread::current().id();
        eprintln!(
            "FluoVerba keep-screen callback begin: thread={thread_id:?}, enabled={keep_screen_on}"
        );
        keep_screen_callback(keep_screen_on);
        eprintln!(
            "FluoVerba keep-screen callback end: thread={thread_id:?}, duration={:.3} ms",
            callback_started.elapsed().as_secs_f64() * 1000.0
        );
    });

    // Integrate real elapsed time in dp/sec. The same normalized progress drives both
    // text rows; there is no fixed-pixel stepping and SQLite is only read at phrase changes.
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(16),
        move || {
            if let Some(ui) = weak.upgrade() {
                let now = Instant::now();
                let frame_time = now.saturating_duration_since(previous_tick);
                previous_tick = now;
                diagnostic_frames += 1;
                diagnostic_total_frame_time += frame_time;
                diagnostic_max_frame_time = diagnostic_max_frame_time.max(frame_time);

                let travel_distance_dp = ui.get_travel_distance().max(1.0);
                let speed = controls.borrow().speed_dp_per_second;
                let (next_progress, completed_trips) =
                    advance_ticker(progress, speed, frame_time, travel_distance_dp);
                progress = next_progress;
                let mut phrase_changed = false;

                for _ in 0..completed_trips {
                    pair_index += 1;
                    if pair_index >= phrases.len() {
                        let mut next = match storage::gui_pairs_after(&connection, cursor) {
                            Ok(next) => next,
                            Err(error) => {
                                eprintln!("Could not load the next phrase batch: {error}");
                                progress = 0.0;
                                break;
                            }
                        };
                        if next.is_empty() {
                            next = match storage::gui_pairs_after(&connection, None) {
                                Ok(next) => next,
                                Err(error) => {
                                    eprintln!("Could not reload phrase batch: {error}");
                                    progress = 0.0;
                                    break;
                                }
                            };
                        }
                        cursor = next.last().map(|item| item.0);
                        phrases = next.into_iter().map(|item| (item.1, item.2)).collect();
                        pair_index = 0;
                    }
                    ui.set_spanish(phrases[pair_index].0.as_str().into());
                    ui.set_russian(phrases[pair_index].1.as_str().into());
                    phrase_changed = true;
                }
                ui.set_snap_progress(phrase_changed);
                ui.set_progress(progress);
                ui.set_snap_progress(false);

                if now.saturating_duration_since(diagnostic_started) >= Duration::from_secs(5) {
                    let elapsed = now.saturating_duration_since(diagnostic_started).as_secs_f64();
                    let mean_callback_ms = diagnostic_total_frame_time.as_secs_f64()
                        * 1000.0
                        / f64::from(diagnostic_frames.max(1));
                    let (render_fps, mean_render_ms, max_render_ms) = {
                        let mut metrics = render_metrics.borrow_mut();
                        let fps = f64::from(metrics.frames) / elapsed;
                        let mean = if metrics.frames == 0 {
                            0.0
                        } else {
                            metrics.total_frame_time.as_secs_f64() * 1000.0
                                / f64::from(metrics.frames)
                        };
                        let max = metrics.max_frame_time.as_secs_f64() * 1000.0;
                        metrics.frames = 0;
                        metrics.total_frame_time = Duration::ZERO;
                        metrics.max_frame_time = Duration::ZERO;
                        (fps, mean, max)
                    };
                    eprintln!(
                        "FluoVerba frame diagnostics: {:.1} rendered frames/s, mean render interval {:.2} ms, max {:.2} ms; UI callbacks {:.1}/s, mean interval {:.2} ms, max {:.2} ms",
                        render_fps,
                        mean_render_ms,
                        max_render_ms,
                        f64::from(diagnostic_frames) / elapsed,
                        mean_callback_ms,
                        diagnostic_max_frame_time.as_secs_f64() * 1000.0
                    );
                    diagnostic_started = now;
                    diagnostic_frames = 0;
                    diagnostic_total_frame_time = Duration::ZERO;
                    diagnostic_max_frame_time = Duration::ZERO;
                }
            }
        },
    );

    window.run()?;
    Ok(())
}

#[derive(Default)]
struct RenderMetrics {
    last_render: Option<Instant>,
    frames: u32,
    total_frame_time: Duration,
    max_frame_time: Duration,
}

#[derive(Debug)]
struct SettingsState {
    settings_open: bool,
    speed_dp_per_second: f32,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            settings_open: false,
            speed_dp_per_second: 20.0,
        }
    }
}

impl SettingsState {
    fn toggle_settings(&mut self) -> bool {
        self.settings_open = !self.settings_open;
        self.settings_open
    }

    #[cfg(any(not(feature = "disable-speed-handler"), test))]
    fn set_speed(&mut self, speed_dp_per_second: f32) {
        self.speed_dp_per_second = speed_dp_per_second.clamp(5.0, 120.0);
    }
}

fn advance_ticker(
    progress: f32,
    speed_dp_per_second: f32,
    elapsed: Duration,
    travel_distance_dp: f32,
) -> (f32, usize) {
    let progress =
        progress + speed_dp_per_second * elapsed.as_secs_f32() / travel_distance_dp.max(1.0);
    let trips = progress.floor() as usize;
    (progress - trips as f32, trips)
}

#[cfg(test)]
mod animation_tests {
    use super::{advance_ticker, SettingsState};
    use std::time::Duration;

    #[test]
    fn distance_and_duration_match_configured_speeds() {
        const DISTANCE_DP: f32 = 800.0;
        for speed in [10.0_f32, 20.0, 40.0, 80.0] {
            let duration = Duration::from_secs_f32(DISTANCE_DP / speed);
            let (progress, completed_trips) = advance_ticker(0.0, speed, duration, DISTANCE_DP);
            assert_eq!(completed_trips, 1, "speed {speed} dp/s");
            assert!(progress.abs() < 0.0001, "speed {speed} dp/s: {progress}");
        }
    }

    #[test]
    fn speed_change_does_not_change_current_position() {
        const DISTANCE_DP: f32 = 800.0;
        let (position, trips) = advance_ticker(0.0, 20.0, Duration::from_secs(4), DISTANCE_DP);
        assert_eq!(trips, 0);
        // Changing the speed setting only affects the next elapsed-time integration.
        let (same_position, trips) = advance_ticker(position, 80.0, Duration::ZERO, DISTANCE_DP);
        assert_eq!(trips, 0);
        assert_eq!(same_position, position);
    }

    #[test]
    fn repeated_settings_open_close_preserves_speed_changes() {
        let mut controls = SettingsState::default();
        assert_eq!(controls.speed_dp_per_second, 20.0);

        for iteration in 0..100 {
            assert!(controls.toggle_settings(), "iteration {iteration}: open");
            let speed = if iteration % 2 == 0 { 5.0 } else { 120.0 };
            controls.set_speed(speed);
            assert_eq!(controls.speed_dp_per_second, speed);
            assert!(!controls.toggle_settings(), "iteration {iteration}: close");
            assert_eq!(controls.speed_dp_per_second, speed);
        }
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: slint::android::AndroidApp) {
    if let Err(error) = android_entry(app) {
        eprintln!("FluoVerba startup failed: {error}");
    }
}

#[cfg(target_os = "android")]
fn android_entry(app: slint::android::AndroidApp) -> Result<(), Box<dyn Error>> {
    slint::android::init(app.clone())?;
    let data_dir = app
        .internal_data_path()
        .ok_or("Android did not provide the private internal data directory")?;
    let database = storage::database_path_under(&data_dir);

    if !database.exists() {
        let parent = database
            .parent()
            .ok_or("database path has no parent directory")?;
        std::fs::create_dir_all(parent)?;
        let temporary = database.with_extension("sqlite3.tmp");
        if temporary.exists() {
            std::fs::remove_file(&temporary)?;
        }
        let asset_name = std::ffi::CString::new("fluoverba.sqlite3")?;

        let mut asset = app
            .asset_manager()
            .open(asset_name.as_c_str())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("Android asset not found: {asset_name:?}"),
                )
            })?;

        let mut output = std::fs::File::create(&temporary)?;
        std::io::copy(&mut asset, &mut output)?;
        output.sync_all()?;
        std::fs::rename(temporary, &database)?;
    }

    // android-activity's set_window_flags() takes its NativeActivity mutex and may
    // wait for platform work. Keep that call off Slint's event/render thread.
    let activity = app.clone();
    let (keep_screen_tx, keep_screen_rx) = std::sync::mpsc::channel::<bool>();
    std::thread::Builder::new()
        .name("fluoverba-window-flags".into())
        .spawn(move || {
            use android_activity::WindowManagerFlags;

            while let Ok(keep_screen_on) = keep_screen_rx.recv() {
                let update_started = Instant::now();
                let thread_id = std::thread::current().id();
                eprintln!(
                    "FluoVerba window flag update begin: thread={thread_id:?}, keep_screen_on={keep_screen_on}"
                );
                if keep_screen_on {
                    activity.set_window_flags(
                        WindowManagerFlags::KEEP_SCREEN_ON,
                        WindowManagerFlags::empty(),
                    );
                } else {
                    activity.set_window_flags(
                        WindowManagerFlags::empty(),
                        WindowManagerFlags::KEEP_SCREEN_ON,
                    );
                }
                eprintln!(
                    "FluoVerba window flag update end: thread={thread_id:?}, duration={:.3} ms",
                    update_started.elapsed().as_secs_f64() * 1000.0
                );
            }
        })?;

    run_gui_with_keep_screen_callback(&database, move |keep_screen_on| {
        let callback_started = Instant::now();
        let thread_id = std::thread::current().id();
        eprintln!(
            "FluoVerba keep-screen callback begin: thread={thread_id:?}, enabled={keep_screen_on}"
        );
        match keep_screen_tx.send(keep_screen_on) {
            Ok(()) => eprintln!(
                "FluoVerba keep-screen callback queued: thread={thread_id:?}, enabled={keep_screen_on}"
            ),
            Err(error) => eprintln!(
                "FluoVerba keep-screen callback queue failed: thread={thread_id:?}, error={error}"
            ),
        }
        eprintln!(
            "FluoVerba keep-screen callback end: thread={thread_id:?}, duration={:.3} ms",
            callback_started.elapsed().as_secs_f64() * 1000.0
        );
    })
}
