-- ===========================================================================
-- Coding Studio, local database
--
-- Design principles
--   1. History is kept, never overwritten: segmentations are versioned,
--      prompts, AI answers and arbitration decisions are append-only.
--   2. Every write is paired with one audit event in the same transaction.
--      The audit log is append-only and hash-chained (see cs_model::AuditEvent).
--   3. The thesis coding.db is never written: the codebook is copied here.
--   4. Coding happens in two phases (cs_model::phase): phase 1 against the
--      TOE contexts, phase 2 with study-specific codes on the relevant units.
--      Every coding, prompt, run and arbitration row carries its phase.
-- Schema version 2 (PRAGMA user_version), set by cs-store on open.
-- ===========================================================================

CREATE TABLE IF NOT EXISTS settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- ------------------------------------------------------------------ library
CREATE TABLE IF NOT EXISTS interviews (
    interview_id    TEXT PRIMARY KEY,
    folder_name     TEXT NOT NULL,
    folder_path     TEXT NOT NULL,
    primary_source  TEXT,
    warnings        TEXT NOT NULL DEFAULT '[]',   -- JSON array of strings
    scanned_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sources (
    interview_id  TEXT NOT NULL REFERENCES interviews(interview_id),
    rel_path      TEXT NOT NULL,
    kind          TEXT NOT NULL,
    sha256        TEXT NOT NULL,
    size          INTEGER NOT NULL,
    modified      TEXT,
    PRIMARY KEY (interview_id, rel_path)
);

-- ------------------------------------------------------------------ codebook
CREATE TABLE IF NOT EXISTS codes (
    code        TEXT PRIMARY KEY,
    family      TEXT NOT NULL,
    label       TEXT NOT NULL DEFAULT '',
    definition  TEXT NOT NULL DEFAULT '',
    origin      TEXT NOT NULL,
    active      INTEGER NOT NULL DEFAULT 1,
    in_prompt   INTEGER NOT NULL DEFAULT 1,
    ord         INTEGER NOT NULL,
    local       INTEGER NOT NULL DEFAULT 0,    -- 1 = created in Coding Studio (add_code)
    example     TEXT,                          -- inductive log: example segment or quote
    reason      TEXT,                          -- inductive log: why the code was created
    created_at  TEXT                           -- inductive log: creation time
);

-- -------------------------------------------------------------- segmentation
-- One row per version; the current segmentation is the latest seg_id.
CREATE TABLE IF NOT EXISTS segmentations (
    seg_id           INTEGER PRIMARY KEY AUTOINCREMENT,
    interview_id     TEXT NOT NULL REFERENCES interviews(interview_id),
    source_rel_path  TEXT NOT NULL,
    source_sha256    TEXT NOT NULL,
    rule             TEXT NOT NULL,
    hash             TEXT NOT NULL,
    warnings         TEXT NOT NULL DEFAULT '[]',
    saved_at         TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS units (
    seg_id       INTEGER NOT NULL REFERENCES segmentations(seg_id),
    unit_id      TEXT NOT NULL,
    seq          INTEGER NOT NULL,
    speaker_raw  TEXT NOT NULL,
    role         TEXT NOT NULL,
    timestamp    TEXT NOT NULL,
    text         TEXT NOT NULL,
    PRIMARY KEY (seg_id, unit_id)
);

-- -------------------------------------------------------------- human coding
-- Current state, one row per unit and phase. Every change is in audit_log (before/after).
CREATE TABLE IF NOT EXISTS human_codings (
    interview_id  TEXT NOT NULL REFERENCES interviews(interview_id),
    phase         INTEGER NOT NULL,
    unit_id       TEXT NOT NULL,
    codes         TEXT NOT NULL,     -- JSON array
    subcode       TEXT,
    modifier      TEXT,
    ev_level      TEXT,
    provenance    TEXT,
    memo          TEXT,
    excerpt       TEXT,
    updated_at    TEXT NOT NULL,
    PRIMARY KEY (interview_id, phase, unit_id)
);

-- ------------------------------------------------------------ prompts, AI runs
CREATE TABLE IF NOT EXISTS prompts (
    prompt_id          INTEGER PRIMARY KEY AUTOINCREMENT,
    interview_id       TEXT NOT NULL REFERENCES interviews(interview_id),
    phase              INTEGER NOT NULL,
    part               INTEGER NOT NULL,
    of                 INTEGER NOT NULL,
    unit_ids           TEXT NOT NULL,    -- JSON array
    segmentation_hash  TEXT NOT NULL,
    codebook_hash      TEXT NOT NULL,
    prompt_version     TEXT NOT NULL,
    text               TEXT NOT NULL,
    sha256             TEXT NOT NULL,
    created_at         TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS ai_runs (
    run_id             INTEGER PRIMARY KEY AUTOINCREMENT,
    interview_id       TEXT NOT NULL REFERENCES interviews(interview_id),
    phase              INTEGER NOT NULL,
    run_label          TEXT NOT NULL,
    model_name         TEXT NOT NULL,
    model_version      TEXT NOT NULL,
    temperature        REAL,
    seed               TEXT,
    context_length     INTEGER,
    batch_size         INTEGER,
    prompt_id_version  TEXT NOT NULL,
    executed_at        TEXT NOT NULL,
    notes              TEXT,
    created_at         TEXT NOT NULL,
    UNIQUE (interview_id, phase, run_label)
);

CREATE TABLE IF NOT EXISTS ai_outputs (
    output_id          INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id             INTEGER NOT NULL REFERENCES ai_runs(run_id),
    part               INTEGER NOT NULL,
    of                 INTEGER NOT NULL,
    raw_text           TEXT NOT NULL,    -- exactly as pasted
    raw_sha256         TEXT NOT NULL,
    segmentation_hash  TEXT NOT NULL,
    parsed             TEXT NOT NULL,    -- JSON of cs_model::AiOutput
    imported_at        TEXT NOT NULL,
    UNIQUE (run_id, part)
);

-- --------------------------------------------------------------- arbitration
-- Append-only: the current decision for a unit and phase is the latest arb_id.
CREATE TABLE IF NOT EXISTS arbitrations (
    arb_id        INTEGER PRIMARY KEY AUTOINCREMENT,
    interview_id  TEXT NOT NULL REFERENCES interviews(interview_id),
    phase         INTEGER NOT NULL,
    unit_id       TEXT NOT NULL,
    choice        TEXT NOT NULL,    -- JSON of cs_model::ArbitrationChoice
    final_codes   TEXT NOT NULL,    -- JSON array
    rationale     TEXT NOT NULL,
    decided_by    TEXT NOT NULL,
    decided_at    TEXT NOT NULL
);

-- --------------------------------------------------------------- audit trail
CREATE TABLE IF NOT EXISTS audit_log (
    seq           INTEGER PRIMARY KEY,
    ts            TEXT NOT NULL,
    actor         TEXT NOT NULL,
    action        TEXT NOT NULL,
    interview_id  TEXT,
    target        TEXT,
    detail        TEXT NOT NULL,    -- JSON
    prev_hash     TEXT NOT NULL,
    hash          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_interview ON audit_log(interview_id);
CREATE INDEX IF NOT EXISTS idx_arbitrations_unit ON arbitrations(interview_id, phase, unit_id);
CREATE INDEX IF NOT EXISTS idx_runs_phase ON ai_runs(interview_id, phase);
CREATE INDEX IF NOT EXISTS idx_outputs_run ON ai_outputs(run_id);

-- ------------------------------------------------------- append-only guards
CREATE TRIGGER IF NOT EXISTS audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN SELECT RAISE(ABORT, 'append-only: audit_log'); END;
CREATE TRIGGER IF NOT EXISTS audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN SELECT RAISE(ABORT, 'append-only: audit_log'); END;

CREATE TRIGGER IF NOT EXISTS prompts_no_update BEFORE UPDATE ON prompts
BEGIN SELECT RAISE(ABORT, 'append-only: prompts'); END;
CREATE TRIGGER IF NOT EXISTS prompts_no_delete BEFORE DELETE ON prompts
BEGIN SELECT RAISE(ABORT, 'append-only: prompts'); END;

CREATE TRIGGER IF NOT EXISTS ai_outputs_no_update BEFORE UPDATE ON ai_outputs
BEGIN SELECT RAISE(ABORT, 'append-only: ai_outputs'); END;
CREATE TRIGGER IF NOT EXISTS ai_outputs_no_delete BEFORE DELETE ON ai_outputs
BEGIN SELECT RAISE(ABORT, 'append-only: ai_outputs'); END;

CREATE TRIGGER IF NOT EXISTS arbitrations_no_update BEFORE UPDATE ON arbitrations
BEGIN SELECT RAISE(ABORT, 'append-only: arbitrations'); END;
CREATE TRIGGER IF NOT EXISTS arbitrations_no_delete BEFORE DELETE ON arbitrations
BEGIN SELECT RAISE(ABORT, 'append-only: arbitrations'); END;
