use std::{
    error::Error,
    path::{Path, PathBuf},
};

use rusqlite::{params, Connection, Transaction};

const MIGRATION: &str = include_str!("../data/migrations/001_initial.sql");
const BATCH_SIZE: usize = 5_000;
const GUI_BATCH_SIZE: usize = 256;

pub type AppResult<T> = Result<T, Box<dyn Error>>;

pub fn database_path() -> AppResult<PathBuf> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);

    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support"));

    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));

    #[cfg(not(any(unix, target_os = "windows")))]
    let base: Option<PathBuf> = None;

    let base = base.ok_or("could not determine the OS user data directory")?;
    Ok(database_path_under(&base))
}

pub fn database_path_under(base: &Path) -> PathBuf {
    base.join("FluoVerba").join("fluoverba.sqlite3")
}

pub fn open_database(path: &Path) -> AppResult<Connection> {
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(connection)
}

pub fn initialize(connection: &mut Connection) -> AppResult<()> {
    connection.execute_batch(MIGRATION)?;
    Ok(())
}

pub fn import_tsv(connection: &mut Connection, input: impl std::io::BufRead) -> AppResult<u64> {
    initialize(connection)?;
    let mut count = 0;
    let mut batch = Vec::with_capacity(BATCH_SIZE);

    for (line_number, line) in input.lines().enumerate() {
        let line = line?;
        let line = if line_number == 0 {
            line.trim_start_matches('\u{feff}')
        } else {
            &line
        };
        let fields: Vec<_> = line.splitn(4, '\t').collect();
        if fields.len() != 4 {
            return Err(format!(
                "line {}: expected four tab-separated columns",
                line_number + 1
            )
            .into());
        }
        let spanish_id = parse_id(fields[0], line_number + 1)?;
        let russian_id = parse_id(fields[2], line_number + 1)?;
        if fields[1].is_empty() || fields[3].is_empty() {
            return Err(
                format!("line {}: sentence text must not be empty", line_number + 1).into(),
            );
        }
        batch.push((
            spanish_id,
            fields[1].to_owned(),
            russian_id,
            fields[3].to_owned(),
        ));
        if batch.len() >= BATCH_SIZE {
            count += write_batch(connection, &batch)?;
            batch.clear();
        }
    }

    if !batch.is_empty() {
        count += write_batch(connection, &batch)?;
    }
    Ok(count)
}

fn parse_id(value: &str, line: usize) -> AppResult<i64> {
    let id: i64 = value
        .parse()
        .map_err(|_| format!("line {line}: invalid Tatoeba ID {value:?}"))?;
    if id <= 0 {
        return Err(format!("line {line}: Tatoeba IDs must be positive").into());
    }
    Ok(id)
}

fn write_batch(
    connection: &mut Connection,
    batch: &[(i64, String, i64, String)],
) -> AppResult<u64> {
    let transaction = connection.transaction()?;
    for (spanish_id, spanish, russian_id, russian) in batch {
        upsert_sentence(&transaction, *spanish_id, "spa", spanish)?;
        upsert_sentence(&transaction, *russian_id, "rus", russian)?;
        transaction.execute(
            "INSERT INTO translations (source_id, target_id) VALUES (?1, ?2) \
             ON CONFLICT(source_id, target_id) DO NOTHING",
            params![spanish_id, russian_id],
        )?;
    }
    transaction.commit()?;
    Ok(batch.len() as u64)
}

fn upsert_sentence(
    transaction: &Transaction<'_>,
    id: i64,
    language: &str,
    text: &str,
) -> AppResult<()> {
    transaction.execute(
        "INSERT INTO sentences (id, language, text) VALUES (?1, ?2, ?3) \
         ON CONFLICT(id) DO UPDATE SET language = excluded.language, text = excluded.text",
        params![id, language, text],
    )?;
    Ok(())
}

pub fn stats(connection: &Connection) -> AppResult<(Vec<(String, i64)>, i64)> {
    let mut statement = connection
        .prepare("SELECT language, COUNT(*) FROM sentences GROUP BY language ORDER BY language")?;
    let languages = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let links = connection.query_row("SELECT COUNT(*) FROM translations", [], |row| row.get(0))?;
    Ok((languages, links))
}

pub fn gui_pairs_after(
    connection: &Connection,
    after: Option<(i64, i64)>,
) -> AppResult<Vec<((i64, i64), String, String)>> {
    let mut statement = connection.prepare(
        "SELECT t.source_id, t.target_id, source.text, target.text FROM translations t \
         JOIN sentences source ON source.id = t.source_id \
         JOIN sentences target ON target.id = t.target_id \
         WHERE source.language = 'spa' AND target.language = 'rus' \
         AND (?1 IS NULL OR (t.source_id, t.target_id) > (?1, ?2)) \
         ORDER BY t.source_id, t.target_id LIMIT ?3",
    )?;
    let pairs = statement
        .query_map(
            params![
                after.map(|cursor| cursor.0),
                after.map(|cursor| cursor.1),
                GUI_BATCH_SIZE as i64
            ],
            |row| Ok(((row.get(0)?, row.get(1)?), row.get(2)?, row.get(3)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    if pairs.is_empty() && after.is_none() {
        return Err("database has no Spanish–Russian pairs; run `fluoverba import [path/to/spa-rus.tsv]` first".into());
    }
    Ok(pairs)
}

pub fn database_exists(path: &Path) -> bool {
    path.exists()
}

pub fn optional_stats(path: &Path) -> AppResult<Option<(Vec<(String, i64)>, i64)>> {
    if !database_exists(path) {
        return Ok(None);
    }
    let connection = open_database(path)?;
    Ok(Some(stats(&connection)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Cursor,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn test_db() -> (Connection, PathBuf) {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "fluoverba-test-{}-{suffix}.sqlite3",
            std::process::id()
        ));
        (open_database(&path).unwrap(), path)
    }

    fn data() -> Cursor<&'static [u8]> {
        Cursor::new(
            concat!(
                "\u{feff}2483\t¿Qué estás haciendo?\t5411\tЧто ты делаешь?\n",
                "2483\t¿Qué estás haciendo?\t469594\tЧто ты сейчас делаешь?\n",
                "2483\t¿Qué estás haciendo?\t5411\tЧто ты делаешь?\n",
            )
            .as_bytes(),
        )
    }

    #[test]
    fn bundled_sqlite_is_at_least_3_54_0() {
        assert!(
            rusqlite::version_number() >= 3_054_000,
            "bundled SQLite {} is older than 3.54.0",
            rusqlite::version()
        );
    }

    #[test]
    fn import_keeps_global_ids_deduplicates_links_and_supports_many_translations() {
        let (mut connection, path) = test_db();
        assert_eq!(import_tsv(&mut connection, data()).unwrap(), 3);
        assert_eq!(
            connection
                .query_row("SELECT id FROM sentences WHERE language = 'spa'", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            2483
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sentences", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM translations", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM translations WHERE source_id = 2483",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            2
        );
        let (languages, links) = stats(&connection).unwrap();
        assert_eq!(
            languages,
            vec![("rus".to_owned(), 2), ("spa".to_owned(), 1)]
        );
        assert_eq!(links, 2);
        assert!(gui_pairs_after(&connection, Some((2483, 469594)))
            .unwrap()
            .is_empty());
        drop(connection);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn reimport_is_idempotent() {
        let (mut connection, path) = test_db();
        import_tsv(&mut connection, data()).unwrap();
        import_tsv(&mut connection, data()).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM sentences", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM translations", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        drop(connection);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn malformed_row_returns_line_error() {
        let (mut connection, path) = test_db();
        let error = import_tsv(&mut connection, Cursor::new(b"1\ttext\tbad\ttexto\n")).unwrap_err();
        assert!(error.to_string().contains("line 1"));
        drop(connection);
        let _ = std::fs::remove_file(path);
    }
}
