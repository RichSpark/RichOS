-- RichDoctor report relay, schema 2 (screenshots, PRD v6 §7). Receipt and delivery state, and a
-- report's words only until it is posted or retired. Never a receipt secret, an IP address or an
-- installation ID. Screenshot bytes live in R2; these rows say which are stored. Schema 2 replaced
-- schema 1 before any production database existed.
CREATE TABLE IF NOT EXISTS reports (
  report_id TEXT PRIMARY KEY,
  secret_hash TEXT NOT NULL,
  content_hash TEXT,
  protocol INTEGER NOT NULL DEFAULT 1,
  state TEXT NOT NULL CHECK (state IN ('receiving','ready','pending','unconfirmed','delivered','not_created','retired')),
  title TEXT,
  body TEXT,
  attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
  attempt_id TEXT,
  attempt_at INTEGER NOT NULL DEFAULT 0,
  sent INTEGER NOT NULL DEFAULT 0 CHECK (sent IN (0,1)),
  lease_until INTEGER NOT NULL DEFAULT 0,
  checked_at INTEGER NOT NULL DEFAULT 0,
  reason TEXT,
  retry_at INTEGER,
  issue_number INTEGER,
  issue_url TEXT,
  approved_at INTEGER NOT NULL DEFAULT 0,
  busy_id TEXT,
  busy_until INTEGER NOT NULL DEFAULT 0,
  cleanup TEXT CHECK (cleanup IS NULL OR cleanup IN ('claimed','done')),
  active_at INTEGER NOT NULL,
  delivered_at INTEGER,
  expires_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS reports_open ON reports(state, attempt_at);
CREATE INDEX IF NOT EXISTS reports_cleanup ON reports(state, active_at);
CREATE TABLE IF NOT EXISTS attachments (
  report_id TEXT NOT NULL,
  attachment_id TEXT NOT NULL,
  ord INTEGER NOT NULL,
  media_type TEXT NOT NULL CHECK (media_type IN ('image/png','image/jpeg')),
  width INTEGER NOT NULL,
  height INTEGER NOT NULL,
  bytes INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  stored INTEGER NOT NULL DEFAULT 0 CHECK (stored IN (0,1)),
  deleted_at INTEGER,
  PRIMARY KEY (report_id, attachment_id)
);
CREATE TABLE IF NOT EXISTS control (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  paused INTEGER NOT NULL CHECK (paused IN (0,1)),
  daily_cap INTEGER NOT NULL CHECK (daily_cap >= 0),
  daily_bytes_cap INTEGER NOT NULL DEFAULT 0 CHECK (daily_bytes_cap >= 0),
  operator TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS daily (
  day TEXT PRIMARY KEY,
  creates INTEGER NOT NULL DEFAULT 0 CHECK (creates >= 0),
  bytes INTEGER NOT NULL DEFAULT 0 CHECK (bytes >= 0)
);
-- A new database starts switched off with caps of zero. The operator turns it on.
INSERT OR IGNORE INTO control (id, paused, daily_cap, daily_bytes_cap, operator, updated_at) VALUES (1, 1, 0, 0, '', 0);
