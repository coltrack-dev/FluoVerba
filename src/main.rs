use std::{error::Error, fs, io::BufReader, path::PathBuf};

use fluoverba::storage;

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
        None => fluoverba::run_gui_at(&storage::database_path()?),
        Some(other) => Err(format!(
            "unknown command {other:?}; use `import`, `stats`, or no command for the GUI"
        )
        .into()),
    }
}

fn default_tsv_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/imports/spa-rus.tsv")
}
