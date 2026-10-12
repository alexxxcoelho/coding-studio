-- ===========================================================================
-- Coding Studio, local database
--
-- Design principles
--   1. History is kept, never overwritten: segmentations are versioned;
--      prompts, AI answers, review decisions, agreement samples and the
--      resolutions of labels to clarify are append-only.
--   2. Every write is paired with one audit event in the same transaction.
--      The audit log is append-only and hash-chained (see cs_model::AuditEvent).
--   3. The thesis coding.db is never written: the codebook is copied here and
--      exported as files.
--   4. Phase 1 is the candidate's provisional TOE pre-classification; phase 2
--      carries concrete codes and is the only phase challenged by the AI.
-- Schema version 3 (PRAGMA user_version), set by cs-store on open.
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
    code               TEXT PRIMARY KEY,
    family             TEXT NOT NULL,
    label              TEXT NOT NULL DEFAULT '',
    definition         TEXT NOT NULL DEFAULT '',
    origin             TEXT NOT NULL,               -- literature | field | legacy
    toe                TEXT,                        -- T | O | E | NULL
    active             INTEGER NOT NULL DEFAULT 1,
    in_prompt          INTEGER NOT NULL DEFAULT 1,
    research_questions TEXT NOT NULL DEFAULT '[]',  -- JSON array of ids
    merged_into        TEXT,
    version            INTEGER NOT NULL DEFAULT 1,
    ord                INTEGER NOT NULL,
    local              INTEGER NOT NULL DEFAULT 0,  -- 1 = created or edited in Coding Studio
    example            TEXT,
    reason             TEXT,
    created_at         TEXT
);

CREATE TABLE IF NOT EXISTS research_questions (
    id    TEXT PRIMARY KEY,
    text  TEXT NOT NULL DEFAULT '',
    ord   INTEGER NOT NULL
);

-- Memo per research question; current state, every change is in audit_log.
CREATE TABLE IF NOT EXISTS rq_memos (
    rq_id           TEXT PRIMARY KEY REFERENCES research_questions(id),
    documented      TEXT NOT NULL DEFAULT '',
    interpretation  TEXT NOT NULL DEFAULT '',
    rivals          TEXT NOT NULL DEFAULT '',
    limits          TEXT NOT NULL DEFAULT '',
    units_for       TEXT NOT NULL DEFAULT '[]',   -- JSON array of unit ids
    units_against   TEXT NOT NULL DEFAULT '[]',   -- JSON array of unit ids
    updated_at      TEXT NOT NULL
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
    interview_id      TEXT NOT NULL REFERENCES interviews(interview_id),
    phase             INTEGER NOT NULL,
    unit_id           TEXT NOT NULL,
    codes             TEXT NOT NULL,     -- JSON array
    subcode           TEXT,
    modifier          TEXT,
    ev_level          TEXT,
    provenance        TEXT,
    memo              TEXT,
    excerpt           TEXT,
    revised_after_ai  INTEGER NOT NULL DEFAULT 0,
    updated_at        TEXT NOT NULL,
    PRIMARY KEY (interview_id, phase, unit_id)
);

-- ------------------------------------------- unit flags, corrections, verification
-- Flags are current state (editable, audited). The segmentation is never
-- rewritten: the current text and role of a unit come from its latest correction.
CREATE TABLE IF NOT EXISTS unit_flags (
    interview_id          TEXT NOT NULL REFERENCES interviews(interview_id),
    unit_id               TEXT NOT NULL,
    flags                 TEXT NOT NULL,     -- JSON array, subset of UNIT_FLAGS
    co_constructed_kind   TEXT,              -- assent | content, only with CO-CONSTRUCTED
    updated_at            TEXT NOT NULL,
    PRIMARY KEY (interview_id, unit_id)
);

-- Append-only: the current text and role of a unit are those of its latest row.
CREATE TABLE IF NOT EXISTS unit_corrections (
    correction_id  INTEGER PRIMARY KEY AUTOINCREMENT,
    interview_id   TEXT NOT NULL REFERENCES interviews(interview_id),
    unit_id        TEXT NOT NULL,
    text           TEXT NOT NULL,
    role           TEXT NOT NULL,
    reason         TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    created_by     TEXT NOT NULL
);

-- Append-only: the current status is the latest row, downgraded to unverified
-- when text_sha256 or role no longer match the current unit.
CREATE TABLE IF NOT EXISTS unit_verifications (
    verification_id  INTEGER PRIMARY KEY AUTOINCREMENT,
    interview_id     TEXT NOT NULL REFERENCES interviews(interview_id),
    unit_id          TEXT NOT NULL,
    state            TEXT NOT NULL,     -- verified | unusable
    text_sha256      TEXT NOT NULL,
    role             TEXT NOT NULL,
    note             TEXT,
    created_at       TEXT NOT NULL,
    created_by       TEXT NOT NULL
);

-- Qualifications per statement. Editable; deletion is audited, the audit row stays.
CREATE TABLE IF NOT EXISTS claims (
    claim_id      INTEGER PRIMARY KEY AUTOINCREMENT,
    interview_id  TEXT NOT NULL REFERENCES interviews(interview_id),
    unit_id       TEXT NOT NULL,
    excerpt       TEXT NOT NULL,       -- exact substring of the current unit text
    codes         TEXT NOT NULL,       -- JSON array, subset of the unit's current codes
    ev_level      TEXT,                -- EV0..EV4
    provenance    TEXT,                -- SPONTANEOUS | PROBED | DOCUMENT-DERIVED | RESEARCHER-INFERRED
    role          TEXT,                -- trigger | retrospective-justification | enabling-condition | constraint | undetermined
    memo          TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

-- ------------------------------------------------------------ to clarify
-- Label and note stay editable while the item is open; the resolution is
-- append-only and closes the item.
CREATE TABLE IF NOT EXISTS to_clarify (
    tid           INTEGER PRIMARY KEY AUTOINCREMENT,
    interview_id  TEXT NOT NULL REFERENCES interviews(interview_id),
    unit_id       TEXT NOT NULL,
    label         TEXT NOT NULL,
    note          TEXT NOT NULL DEFAULT '',
    created_at    TEXT NOT NULL,
    created_by    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS to_clarify_resolutions (
    tid        INTEGER PRIMARY KEY REFERENCES to_clarify(tid),
    kind       TEXT NOT NULL,     -- new_code | mapped
    code       TEXT NOT NULL,
    rationale  TEXT,
    at         TEXT NOT NULL,
    by         TEXT NOT NULL
);

-- ------------------------------------------------------------ prompts, AI runs
CREATE TABLE IF NOT EXISTS prompts (
    prompt_id          INTEGER PRIMARY KEY AUTOINCREMENT,
    interview_id       TEXT NOT NULL REFERENCES interviews(interview_id),
    batch_id           INTEGER NOT NULL,
    phase              INTEGER NOT NULL,
    part               INTEGER NOT NULL,
    of                 INTEGER NOT NULL,
    unit_ids           TEXT NOT NULL,    -- JSON array
    codes              TEXT NOT NULL,    -- JSON array, codes offered (snapshot)
    segmentation_hash  TEXT NOT NULL,
    codebook_hash      TEXT NOT NULL,
    prompt_version     TEXT NOT NULL,
    text               TEXT NOT NULL,
    sha256             TEXT NOT NULL,
    created_at         TEXT NOT NULL,
    UNIQUE (interview_id, batch_id, part)
);

CREATE TABLE IF NOT EXISTS ai_runs (
    run_id             INTEGER PRIMARY KEY AUTOINCREMENT,
    interview_id       TEXT NOT NULL REFERENCES interviews(interview_id),
    batch_id           INTEGER NOT NULL,
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
    UNIQUE (interview_id, run_label)
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

-- The candidate's phase 2 coding of every unit of a prompt batch, frozen in
-- the same transaction as the prompts. Reference of every comparison with
-- the runs of that batch. Append-only.
CREATE TABLE IF NOT EXISTS human_snapshots (
    snapshot_id        INTEGER PRIMARY KEY AUTOINCREMENT,
    interview_id       TEXT NOT NULL REFERENCES interviews(interview_id),
    batch_id           INTEGER NOT NULL,
    phase              INTEGER NOT NULL,
    segmentation_hash  TEXT NOT NULL,
    codebook_hash      TEXT NOT NULL,
    codings            TEXT NOT NULL,    -- JSON object unit_id -> sorted codes
    sha256             TEXT NOT NULL,    -- of codings
    created_at         TEXT NOT NULL,
    UNIQUE (interview_id, batch_id)
);

-- Agreements drawn for review when a run becomes complete (append-only).
CREATE TABLE IF NOT EXISTS review_samples (
    run_id      INTEGER PRIMARY KEY REFERENCES ai_runs(run_id),
    seed        TEXT NOT NULL,
    unit_ids    TEXT NOT NULL,    -- JSON array
    n_agreeing  INTEGER NOT NULL,
    created_at  TEXT NOT NULL
);

-- ------------------------------------------------------------------- review
-- Append-only: the current decision for a unit is the latest arb_id.
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

-- -------------------------------------------------------------------- cases
CREATE TABLE IF NOT EXISTS cases (
    case_id         TEXT PRIMARY KEY,
    interview_id    TEXT NOT NULL REFERENCES interviews(interview_id),
    label           TEXT NOT NULL,
    fields          TEXT NOT NULL,    -- JSON of the five CaseField
    stage           TEXT,
    outcome_status  TEXT,
    notes           TEXT,
    updated_at      TEXT NOT NULL
);

-- Units attributed to a case, from any interview. The cross-case matrix
-- uses only these; a case without attributed units is undocumented.
CREATE TABLE IF NOT EXISTS case_units (
    case_id       TEXT NOT NULL REFERENCES cases(case_id),
    unit_id       TEXT NOT NULL,
    interview_id  TEXT NOT NULL REFERENCES interviews(interview_id),
    PRIMARY KEY (case_id, unit_id)
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
CREATE INDEX IF NOT EXISTS idx_arbitrations_unit ON arbitrations(interview_id, unit_id);
CREATE INDEX IF NOT EXISTS idx_runs_interview ON ai_runs(interview_id);
CREATE INDEX IF NOT EXISTS idx_outputs_run ON ai_outputs(run_id);
CREATE INDEX IF NOT EXISTS idx_to_clarify_unit ON to_clarify(interview_id, unit_id);
CREATE INDEX IF NOT EXISTS idx_cases_interview ON cases(interview_id);
CREATE INDEX IF NOT EXISTS idx_claims_unit ON claims(interview_id, unit_id);
CREATE INDEX IF NOT EXISTS idx_corrections_unit ON unit_corrections(interview_id, unit_id);
CREATE INDEX IF NOT EXISTS idx_verifications_unit ON unit_verifications(interview_id, unit_id);

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

CREATE TRIGGER IF NOT EXISTS human_snapshots_no_update BEFORE UPDATE ON human_snapshots
BEGIN SELECT RAISE(ABORT, 'human_snapshots is append-only'); END;
CREATE TRIGGER IF NOT EXISTS human_snapshots_no_delete BEFORE DELETE ON human_snapshots
BEGIN SELECT RAISE(ABORT, 'human_snapshots is append-only'); END;

CREATE TRIGGER IF NOT EXISTS review_samples_no_update BEFORE UPDATE ON review_samples
BEGIN SELECT RAISE(ABORT, 'append-only: review_samples'); END;
CREATE TRIGGER IF NOT EXISTS review_samples_no_delete BEFORE DELETE ON review_samples
BEGIN SELECT RAISE(ABORT, 'append-only: review_samples'); END;

CREATE TRIGGER IF NOT EXISTS to_clarify_resolutions_no_update BEFORE UPDATE ON to_clarify_resolutions
BEGIN SELECT RAISE(ABORT, 'append-only: to_clarify_resolutions'); END;
CREATE TRIGGER IF NOT EXISTS to_clarify_resolutions_no_delete BEFORE DELETE ON to_clarify_resolutions
BEGIN SELECT RAISE(ABORT, 'append-only: to_clarify_resolutions'); END;

CREATE TRIGGER IF NOT EXISTS to_clarify_no_delete BEFORE DELETE ON to_clarify
BEGIN SELECT RAISE(ABORT, 'append-only: to_clarify'); END;

CREATE TRIGGER IF NOT EXISTS unit_corrections_no_update BEFORE UPDATE ON unit_corrections
BEGIN SELECT RAISE(ABORT, 'append-only: unit_corrections'); END;
CREATE TRIGGER IF NOT EXISTS unit_corrections_no_delete BEFORE DELETE ON unit_corrections
BEGIN SELECT RAISE(ABORT, 'append-only: unit_corrections'); END;

CREATE TRIGGER IF NOT EXISTS unit_verifications_no_update BEFORE UPDATE ON unit_verifications
BEGIN SELECT RAISE(ABORT, 'append-only: unit_verifications'); END;
CREATE TRIGGER IF NOT EXISTS unit_verifications_no_delete BEFORE DELETE ON unit_verifications
BEGIN SELECT RAISE(ABORT, 'append-only: unit_verifications'); END;
