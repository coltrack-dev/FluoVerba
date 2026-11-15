# FluoVerba

Offline Spanish → Russian phrase ticker built with Rust and Slint. The two lines share one horizontal progress value. The GUI loads at most 256 phrase pairs from SQLite once at startup; animation frames do not query the database.

## Requirements

- Rust toolchain >= 1.92 and Slint 1.18.1
- Linux graphical session or Windows desktop
- System/build dependencies needed by the selected Slint backend

## Import the Tatoeba TSV

The source file is kept separately at `data/imports/spa-rus.tsv`. Its expected format has no header and four tab-separated columns: Spanish Tatoeba ID, Spanish text, Russian Tatoeba ID, Russian text.

```bash
cargo run -- import
# Or specify another TSV:
cargo run -- import /path/to/spa-rus.tsv
cargo run -- stats
cargo run
```

The importer streams rows in batches of 5,000, stores original Tatoeba IDs, and can safely be rerun. It creates the SQLite database in the operating system's standard per-user data directory (`%APPDATA%/FluoVerba` on Windows, `~/Library/Application Support/FluoVerba` on macOS, or `$XDG_DATA_HOME/FluoVerba` / `~/.local/share/FluoVerba` on Linux). Import runs only when explicitly requested. The GUI reports a clear error if the database or imported pairs are missing.

## Data attribution

Sentence text comes from [Tatoeba](https://tatoeba.org/) and is licensed under [Creative Commons Attribution 2.0 France (CC BY 2.0 FR)](https://creativecommons.org/licenses/by/2.0/fr/). The database records the source, license, and attribution notice. Tatoeba sentence IDs are retained for traceability. The TSV format used here does not contain individual contributor names.

Topics and the many-to-many `sentence_topics` relation are available for future classification; import does not assign topics.

## Development checks

```bash
cargo fmt --all -- --check
cargo check
cargo test
```

No server, web view, telemetry, cloud service, or runtime internet access is used. The bundled SQLite library is built through `rusqlite`'s `bundled` feature.
