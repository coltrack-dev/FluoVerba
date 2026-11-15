PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS sentences (
    id INTEGER PRIMARY KEY,
    language TEXT NOT NULL,
    text TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sentences_language ON sentences(language);

CREATE TABLE IF NOT EXISTS translations (
    source_id INTEGER NOT NULL REFERENCES sentences(id) ON DELETE CASCADE,
    target_id INTEGER NOT NULL REFERENCES sentences(id) ON DELETE CASCADE,
    PRIMARY KEY (source_id, target_id)
);

CREATE INDEX IF NOT EXISTS idx_translations_target ON translations(target_id);

CREATE TABLE IF NOT EXISTS topics (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS sentence_topics (
    sentence_id INTEGER NOT NULL REFERENCES sentences(id) ON DELETE CASCADE,
    topic_id INTEGER NOT NULL REFERENCES topics(id) ON DELETE CASCADE,
    PRIMARY KEY (sentence_id, topic_id)
);

CREATE INDEX IF NOT EXISTS idx_sentence_topics_topic ON sentence_topics(topic_id);

CREATE TABLE IF NOT EXISTS dataset_sources (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    url TEXT NOT NULL,
    license TEXT NOT NULL,
    attribution TEXT NOT NULL
);

INSERT INTO dataset_sources (id, name, url, license, attribution)
VALUES (1, 'Tatoeba', 'https://tatoeba.org/', 'CC BY 2.0 FR',
        'Sentence data from Tatoeba; individual contributors retain attribution.')
ON CONFLICT(name) DO NOTHING;
