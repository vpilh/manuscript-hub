-- Normalized metadata and source references only (FR15, PRD §6).
-- Timestamps are Unix seconds (UTC); dates are ISO `YYYY-MM-DD`.

CREATE TABLE accounts (
    id            INTEGER PRIMARY KEY,
    provider      TEXT NOT NULL CHECK (provider IN ('gmail', 'microsoft_graph', 'groupwise_imap', 'mock')),
    alias         TEXT NOT NULL,
    -- Connection settings without credentials (IMAP host/port, tenant hint) as JSON.
    settings_json TEXT NOT NULL DEFAULT '{}',
    paused        INTEGER NOT NULL DEFAULT 0,
    last_sync_at  INTEGER,
    last_error    TEXT,
    UNIQUE (provider, alias)
);

CREATE TABLE sync_cursors (
    account_id    INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    folder        TEXT NOT NULL,
    cursor_json   TEXT NOT NULL,
    updated_at    INTEGER NOT NULL,
    PRIMARY KEY (account_id, folder)
);

CREATE TABLE manuscripts (
    id            INTEGER PRIMARY KEY,
    journal       TEXT NOT NULL,
    -- Journal-issued ID; NULL for records created without one.
    external_id   TEXT,
    title         TEXT,
    -- Resubmission chains: the earlier manuscript this one continues.
    previous_id   INTEGER REFERENCES manuscripts (id),
    created_at    INTEGER NOT NULL,
    UNIQUE (journal, external_id)
);

-- One row per processed message, so rescans never create duplicate events (FR10).
CREATE TABLE source_messages (
    id             INTEGER PRIMARY KEY,
    account_id     INTEGER NOT NULL REFERENCES accounts (id) ON DELETE CASCADE,
    message_id     TEXT NOT NULL,
    folder         TEXT NOT NULL,
    received_at    INTEGER NOT NULL,
    parser_version TEXT NOT NULL,
    UNIQUE (account_id, message_id)
);

CREATE TABLE events (
    id                 INTEGER PRIMARY KEY,
    -- NULL for manual entries (FR17).
    source_message_id  INTEGER REFERENCES source_messages (id) ON DELETE SET NULL,
    -- While proposed: the suggested manuscript. After review: the confirmed one.
    manuscript_id      INTEGER REFERENCES manuscripts (id) ON DELETE CASCADE,
    match_kind         TEXT NOT NULL CHECK (match_kind IN ('exact_id', 'id_only', 'title',
                           'ambiguous', 'none', 'manual')),
    journal            TEXT,
    external_id        TEXT,
    title              TEXT,
    status             TEXT CHECK (status IN ('submitted', 'with_editor', 'under_review',
                           'revision_required', 'revised', 'accepted', 'rejected', 'withdrawn')),
    original_phrase    TEXT,
    deadline           TEXT,
    requested_action   TEXT,
    -- The full proposal: per-field confidence, original phrases, uncertainty,
    -- match rationale and parser version (provenance).
    extraction_json    TEXT NOT NULL,
    review_state       TEXT NOT NULL CHECK (review_state IN ('proposed', 'confirmed', 'edited',
                           'ignored', 'unrelated')),
    occurred_at        INTEGER NOT NULL,
    reviewed_at        INTEGER
);

CREATE INDEX events_by_manuscript ON events (manuscript_id, occurred_at);
CREATE INDEX events_awaiting_review ON events (review_state) WHERE review_state = 'proposed';
