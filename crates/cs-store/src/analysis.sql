-- ===========================================================================
-- Coding Studio, V1 lot 5 (Analyse and Write) and the API gaps of the same lot.
-- Applied by `analysis_schema::migrate` on every open, after schema.sql. Every
-- statement is idempotent (CREATE ... IF NOT EXISTS), so the result is the same
-- whichever order the lots are merged in. Columns added to existing tables are
-- added in Rust, guarded by a column check.
-- ===========================================================================

-- ------------------------------------------------------------------- themes
-- A theme is an analytical proposition: its title is a sentence. The sheet
-- fields sit in the row. Deleted logically.
CREATE TABLE IF NOT EXISTS themes (
    theme_id                    TEXT PRIMARY KEY,         -- T-0001
    title                       TEXT NOT NULL,
    status                      TEXT NOT NULL DEFAULT 'candidate',  -- candidate | revised | retained
    proposition                 TEXT NOT NULL DEFAULT '',
    conditions                  TEXT NOT NULL DEFAULT '',
    alternative_interpretation  TEXT NOT NULL DEFAULT '',
    limits                      TEXT NOT NULL DEFAULT '',
    decision_memo               TEXT NOT NULL DEFAULT '',
    supporting_cases            TEXT NOT NULL DEFAULT '[]', -- JSON array of case ids
    divergent_cases             TEXT NOT NULL DEFAULT '[]',
    main_passages               TEXT NOT NULL DEFAULT '[]', -- JSON array of passage ids
    ord                         INTEGER NOT NULL,           -- document order of the chapter
    created_at                  TEXT NOT NULL,
    created_by                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL,
    deleted_at                  TEXT,
    deleted_by                  TEXT,
    delete_reason               TEXT
);

-- A code may serve several themes.
CREATE TABLE IF NOT EXISTS theme_codes (
    theme_id  TEXT NOT NULL REFERENCES themes(theme_id),
    code_id   TEXT NOT NULL,
    added_at  TEXT NOT NULL,
    added_by  TEXT NOT NULL,
    PRIMARY KEY (theme_id, code_id)
);

CREATE TABLE IF NOT EXISTS theme_questions (
    theme_id  TEXT NOT NULL REFERENCES themes(theme_id),
    rq_id     TEXT NOT NULL,
    PRIMARY KEY (theme_id, rq_id)
);

-- State of each check of the project list (ProjectConfig.theme_checks) for a theme.
CREATE TABLE IF NOT EXISTS theme_check_answers (
    theme_id  TEXT NOT NULL REFERENCES themes(theme_id),
    check_id  TEXT NOT NULL,
    done      INTEGER NOT NULL DEFAULT 0,
    note      TEXT NOT NULL DEFAULT '',
    at        TEXT NOT NULL,
    by        TEXT NOT NULL,
    PRIMARY KEY (theme_id, check_id)
);

-- Append-only: titles and statuses over time, with the reason and, for a move
-- to "retained" without every check done, the override.
CREATE TABLE IF NOT EXISTS theme_history (
    hist_id   INTEGER PRIMARY KEY AUTOINCREMENT,
    theme_id  TEXT NOT NULL,
    kind      TEXT NOT NULL,            -- created | title | status
    before    TEXT,
    after     TEXT,
    reason    TEXT NOT NULL DEFAULT '',
    override  INTEGER NOT NULL DEFAULT 0,
    at        TEXT NOT NULL,
    by        TEXT NOT NULL
);

-- ------------------------------------------------------------------- matrix
-- A saved view is a definition (rows, columns, filters), never stored truth.
CREATE TABLE IF NOT EXISTS matrix_views (
    view_id     TEXT PRIMARY KEY,       -- M-0001
    name        TEXT NOT NULL,
    config      TEXT NOT NULL,          -- JSON of MatrixConfig
    created_at  TEXT NOT NULL,
    created_by  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    deleted_at  TEXT
);

-- Value and one-line summary the researcher gave a cell, keyed by the stable
-- keys of its row and column (for example interview:INT-01 and theme:T-0001).
CREATE TABLE IF NOT EXISTS matrix_cells (
    row_key     TEXT NOT NULL,
    col_key     TEXT NOT NULL,
    value       TEXT,                   -- present | not_mentioned | contradicted | uncertain
    summary     TEXT NOT NULL DEFAULT '',
    updated_at  TEXT NOT NULL,
    updated_by  TEXT NOT NULL,
    PRIMARY KEY (row_key, col_key)
);

-- Passages the researcher linked to a cell by hand (needed for custom
-- dimensions, which have no code to compute from).
CREATE TABLE IF NOT EXISTS matrix_cell_passages (
    row_key     TEXT NOT NULL,
    col_key     TEXT NOT NULL,
    passage_id  TEXT NOT NULL,
    added_at    TEXT NOT NULL,
    added_by    TEXT NOT NULL,
    removed_at  TEXT,
    PRIMARY KEY (row_key, col_key, passage_id)
);

-- -------------------------------------------------------------------- links
-- Typed link between a passage and a part of the text. The type comes from the
-- project list (ProjectConfig.link_types); the note is one line. Removed
-- logically.
CREATE TABLE IF NOT EXISTS passage_links (
    link_id        TEXT PRIMARY KEY,    -- L-0001
    passage_id     TEXT NOT NULL,
    target_kind    TEXT NOT NULL,       -- theme_part | rq_part
    target_id      TEXT NOT NULL,       -- theme id or research question id
    part           TEXT NOT NULL DEFAULT '',
    link_type      TEXT NOT NULL,
    note           TEXT NOT NULL DEFAULT '',
    created_at     TEXT NOT NULL,
    created_by     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    removed_at     TEXT,
    removed_by     TEXT,
    remove_reason  TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_passage_links_active
    ON passage_links(passage_id, target_kind, target_id, part) WHERE removed_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_passage_links_target ON passage_links(target_kind, target_id);

-- ------------------------------------------------------------------ writing
-- Every save of a written part is a row; the current text is the latest row of
-- its (scope, target, sub). Append-only, so earlier versions stay readable.
--   theme_part       target = theme id, sub = part key, body = {text, nature}
--   rq_synthesis     target = research question id, sub = '', body = {text, limits}
--   framework_cell   target = theme id, sub = dimension id, body = {explains, does_not_explain, out_of_frame}
--   framework_theme  target = theme id, sub = '', body = {complementary_dimension, evidence_limit}
CREATE TABLE IF NOT EXISTS writing_versions (
    version_id  INTEGER PRIMARY KEY AUTOINCREMENT,
    scope       TEXT NOT NULL,
    target      TEXT NOT NULL,
    sub         TEXT NOT NULL DEFAULT '',
    body        TEXT NOT NULL,
    at          TEXT NOT NULL,
    by          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_writing_versions ON writing_versions(scope, target, sub);

-- ------------------------------------------------------------- final audit
-- A draw of claims (writing parts that carry links), with the seed that
-- reproduces it. Append-only, as are the verdicts: the latest verdict of a
-- claim in an audit is the current one.
CREATE TABLE IF NOT EXISTS final_audits (
    audit_id    INTEGER PRIMARY KEY AUTOINCREMENT,
    seed        TEXT NOT NULL,
    n           INTEGER NOT NULL,
    eligible    TEXT NOT NULL,          -- JSON array of the claim keys the draw was made among
    claims      TEXT NOT NULL,          -- JSON array of the claim keys drawn
    created_at  TEXT NOT NULL,
    created_by  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS final_audit_verdicts (
    verdict_id    INTEGER PRIMARY KEY AUTOINCREMENT,
    audit_id      INTEGER NOT NULL REFERENCES final_audits(audit_id),
    claim_key     TEXT NOT NULL,
    verdict       TEXT NOT NULL,        -- supported | to_nuance | to_withdraw
    note          TEXT NOT NULL DEFAULT '',
    chain_sha256  TEXT NOT NULL,        -- digest of the chain the researcher looked at
    at            TEXT NOT NULL,
    by            TEXT NOT NULL
);

-- ---------------------------------------------------- gaps reported by the UI
-- Unit marks: "to_review" on a respondent unit, settable and clearable.
CREATE TABLE IF NOT EXISTS unit_marks (
    interview_id  TEXT NOT NULL,
    unit_id       TEXT NOT NULL,
    mark          TEXT NOT NULL,        -- to_review
    note          TEXT NOT NULL DEFAULT '',
    at            TEXT NOT NULL,
    by            TEXT NOT NULL,
    PRIMARY KEY (interview_id, unit_id, mark)
);

-- Memo of a code, one row per version; the current memo is the latest row.
CREATE TABLE IF NOT EXISTS code_memo_history (
    memo_id  INTEGER PRIMARY KEY AUTOINCREMENT,
    code_id  TEXT NOT NULL,
    text     TEXT NOT NULL,
    at       TEXT NOT NULL,
    by       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_code_memo_history ON code_memo_history(code_id);

-- ------------------------------------------------------- append-only guards
CREATE TRIGGER IF NOT EXISTS theme_history_no_update BEFORE UPDATE ON theme_history
BEGIN SELECT RAISE(ABORT, 'append-only: theme_history'); END;
CREATE TRIGGER IF NOT EXISTS theme_history_no_delete BEFORE DELETE ON theme_history
BEGIN SELECT RAISE(ABORT, 'append-only: theme_history'); END;

CREATE TRIGGER IF NOT EXISTS writing_versions_no_update BEFORE UPDATE ON writing_versions
BEGIN SELECT RAISE(ABORT, 'append-only: writing_versions'); END;
CREATE TRIGGER IF NOT EXISTS writing_versions_no_delete BEFORE DELETE ON writing_versions
BEGIN SELECT RAISE(ABORT, 'append-only: writing_versions'); END;

CREATE TRIGGER IF NOT EXISTS final_audits_no_update BEFORE UPDATE ON final_audits
BEGIN SELECT RAISE(ABORT, 'append-only: final_audits'); END;
CREATE TRIGGER IF NOT EXISTS final_audits_no_delete BEFORE DELETE ON final_audits
BEGIN SELECT RAISE(ABORT, 'append-only: final_audits'); END;

CREATE TRIGGER IF NOT EXISTS final_audit_verdicts_no_update BEFORE UPDATE ON final_audit_verdicts
BEGIN SELECT RAISE(ABORT, 'append-only: final_audit_verdicts'); END;
CREATE TRIGGER IF NOT EXISTS final_audit_verdicts_no_delete BEFORE DELETE ON final_audit_verdicts
BEGIN SELECT RAISE(ABORT, 'append-only: final_audit_verdicts'); END;

CREATE TRIGGER IF NOT EXISTS code_memo_history_no_update BEFORE UPDATE ON code_memo_history
BEGIN SELECT RAISE(ABORT, 'append-only: code_memo_history'); END;
CREATE TRIGGER IF NOT EXISTS code_memo_history_no_delete BEFORE DELETE ON code_memo_history
BEGIN SELECT RAISE(ABORT, 'append-only: code_memo_history'); END;

CREATE TRIGGER IF NOT EXISTS themes_no_delete BEFORE DELETE ON themes
BEGIN SELECT RAISE(ABORT, 'themes are deleted logically'); END;
CREATE TRIGGER IF NOT EXISTS passage_links_no_delete BEFORE DELETE ON passage_links
BEGIN SELECT RAISE(ABORT, 'links are removed logically'); END;
