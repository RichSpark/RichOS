-- RichDoctor report relay, schema 1. Receipt and delivery state only: never a report's words,
-- a receipt secret, an IP address or an installation ID. Rows are kept (PRD §7: at least a year).
CREATE TABLE IF NOT EXISTS reports (
  report_id TEXT PRIMARY KEY,
  secret_hash TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('pending','unconfirmed','delivered','not_created')),
  attempt INTEGER NOT NULL CHECK (attempt >= 0),
  attempt_id TEXT,
  attempt_at INTEGER NOT NULL,
  sent INTEGER NOT NULL DEFAULT 0 CHECK (sent IN (0,1)),
  lease_until INTEGER NOT NULL DEFAULT 0,
  checked_at INTEGER NOT NULL DEFAULT 0,
  reason TEXT,
  retry_at INTEGER,
  issue_number INTEGER,
  issue_url TEXT,
  approved_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS reports_open ON reports(state, attempt_at);
CREATE TABLE IF NOT EXISTS control (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  paused INTEGER NOT NULL CHECK (paused IN (0,1)),
  daily_cap INTEGER NOT NULL CHECK (daily_cap >= 0),
  operator TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS daily (
  day TEXT PRIMARY KEY,
  creates INTEGER NOT NULL CHECK (creates >= 0)
);
-- A new database starts switched off with a cap of zero. The operator turns it on.
INSERT OR IGNORE INTO control (id, paused, daily_cap, operator, updated_at) VALUES (1, 1, 0, '', 0);
