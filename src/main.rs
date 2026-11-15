mod storage;

use std::{
    error::Error,
    fs,
    io::BufReader,
    path::PathBuf,
    time::{Duration, Instant},
};

slint::include_modules!();

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    match args.next().as_deref().and_then(|arg| arg.to_str()) {
        Some("import") => {
            let input_path = args
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(default_tsv_path);
            if args.next().is_some() {
                return Err("usage: fluoverba import [path/to/spa-rus.tsv]".into());
            }
            let database = storage::database_path()?;
            if let Some(parent) = database.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut connection = storage::open_database(&database)?;
            let input = BufReader::new(fs::File::open(&input_path).map_err(|error| {
                format!(
                    "could not open import file {}: {error}",
                    input_path.display()
                )
            })?);
            let records = storage::import_tsv(&mut connection, input)?;
            println!("Imported {records} TSV rows into {}", database.display());
            Ok(())
        }
        Some("stats") => {
            if args.next().is_some() {
                return Err("usage: fluoverba stats".into());
            }
            let database = storage::database_path()?;
            match storage::optional_stats(&database)? {
                None => println!(
                    "Database not found at {}. Run `fluoverba import` first.",
                    database.display()
                ),
                Some((languages, links)) => {
                    println!("Database: {}", database.display());
                    for (language, count) in languages {
                        println!("Sentences ({language}): {count}");
                    }
                    println!("Translation links: {links}");
                    println!(
                        "Translations per Spanish sentence: {:.2}",
                        if links == 0 {
                            0.0
                        } else {
                            let spanish_count = storage::open_database(&database)?.query_row(
                                "SELECT COUNT(*) FROM sentences WHERE language = 'spa'",
                                [],
                                |row| row.get::<_, i64>(0),
                            )?;
                            links as f64 / spanish_count as f64
                        }
                    );
                }
            }
            Ok(())
        }
        None => run_gui(),
        Some(other) => Err(format!(
            "unknown command {other:?}; use `import`, `stats`, or no command for the GUI"
        )
        .into()),
    }
}

fn default_tsv_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/imports/spa-rus.tsv")
}

fn run_gui() -> Result<(), Box<dyn Error>> {
    let database = storage::database_path()?;
    if !storage::database_exists(&database) {
        return Err(format!(
            "database not found at {}; run `fluoverba import [path/to/spa-rus.tsv]` first",
            database.display()
        )
        .into());
    }
    let connection = storage::open_database(&database)?;
    let mut batch = storage::gui_pairs_after(&connection, None)?;
    let mut cursor = batch.last().map(|item| item.0);
    let mut phrases: Vec<_> = batch.drain(..).map(|item| (item.1, item.2)).collect();

    let window = MainWindow::new()?;
    let weak = window.as_weak();
    let timer = slint::Timer::default();
    let start = Instant::now();
    window.set_spanish(phrases[0].0.as_str().into());
    window.set_russian(phrases[0].1.as_str().into());
    let mut displayed_cycle = 0usize;
    let mut pair_index = 0usize;

    // Read bounded pages only when the current page is exhausted, never once per frame.
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(33),
        move || {
            if let Some(ui) = weak.upgrade() {
                let elapsed = start.elapsed().as_secs_f32();
                let cycle = (elapsed / 8.0) as usize;
                if cycle > displayed_cycle {
                    while displayed_cycle < cycle {
                        pair_index += 1;
                        if pair_index >= phrases.len() {
                            let mut next = match storage::gui_pairs_after(&connection, cursor) {
                                Ok(next) => next,
                                Err(error) => {
                                    eprintln!("Could not load the next phrase batch: {error}");
                                    ui.set_progress((elapsed % 8.0) / 8.0);
                                    return;
                                }
                            };
                            if next.is_empty() {
                                next = match storage::gui_pairs_after(&connection, None) {
                                    Ok(next) => next,
                                    Err(error) => {
                                        eprintln!("Could not reload phrase batch: {error}");
                                        ui.set_progress((elapsed % 8.0) / 8.0);
                                        return;
                                    }
                                };
                            }
                            cursor = next.last().map(|item| item.0);
                            phrases = next.into_iter().map(|item| (item.1, item.2)).collect();
                            pair_index = 0;
                        }
                        displayed_cycle += 1;
                    }
                    ui.set_spanish(phrases[pair_index].0.as_str().into());
                    ui.set_russian(phrases[pair_index].1.as_str().into());
                }
                ui.set_progress((elapsed % 8.0) / 8.0);
            }
        },
    );

    window.run()?;
    Ok(())
}
