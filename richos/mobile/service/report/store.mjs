// D1 receipt and delivery state. Every write that decides who may call GitHub is one conditional
// statement, so concurrent requests for one report ID serialize in the database, across isolates.
export class Store {
  constructor(db, now = Date.now) { this.db = db; this.now = now; }
  statement(sql, ...args) { return this.db.prepare(sql).bind(...args); }
  get(id) { return this.statement('SELECT * FROM reports WHERE report_id=?', id).first(); }
  control() { return this.statement('SELECT paused, daily_cap FROM control WHERE id=1').first(); }

  // One creation slot of today's global cap (UTC day), or false when the cap is used up.
  async reserve(day, cap) {
    const result = await this.statement(`INSERT INTO daily (day, creates) SELECT ?1, 1 WHERE ?2 > 0
      ON CONFLICT(day) DO UPDATE SET creates = creates + 1 WHERE creates < ?2`, day, cap).run();
    return result.meta.changes === 1;
  }
  async refund(day) { await this.statement('UPDATE daily SET creates = creates - 1 WHERE day=? AND creates > 0', day).run(); }

  // The durable pending record, written before GitHub is called. False if another request won.
  async begin(r, attemptId, lease) {
    const now = this.now();
    const result = await this.statement(`INSERT INTO reports (report_id, secret_hash, content_hash, state, attempt, attempt_id,
      attempt_at, sent, lease_until, approved_at, created_at, updated_at) VALUES (?,?,?,'pending',1,?,?,0,?,?,?,?)
      ON CONFLICT(report_id) DO NOTHING`, r.reportId, r.secretHash, r.contentHash, attemptId, now, now + lease, r.approvedAt, now, now).run();
    return result.meta.changes === 1;
  }
  // The one new serialized attempt a persisted authoritative non-creation permits.
  async retry(id, fromAttempt, attemptId, lease) {
    const now = this.now();
    const result = await this.statement(`UPDATE reports SET state='pending', attempt=attempt+1, attempt_id=?, attempt_at=?, sent=0,
      lease_until=?, checked_at=0, reason=NULL, retry_at=NULL, updated_at=? WHERE report_id=? AND state='not_created' AND attempt=?`,
    attemptId, now, now + lease, now, id, fromAttempt).run();
    return result.meta.changes === 1;
  }
  // Recorded immediately before the create request leaves, and only while the lease holds. A
  // pending record with sent=0 after its lease therefore proves no create request was made.
  async markSent(id, attemptId) {
    const now = this.now();
    const result = await this.statement(`UPDATE reports SET sent=1, updated_at=? WHERE report_id=? AND attempt_id=?
      AND state='pending' AND sent=0 AND lease_until > ?`, now, id, attemptId, now).run();
    return result.meta.changes === 1;
  }
  async deliver(id, issue) {
    await this.statement(`UPDATE reports SET state='delivered', issue_number=?, issue_url=?, reason=NULL, retry_at=NULL, updated_at=?
      WHERE report_id=? AND state IN ('pending','unconfirmed')`, issue.number, issue.url, this.now(), id).run();
  }
  // A delivered record for an older unknown ID whose footer was found before any create.
  async adopt(r, issue) {
    const now = this.now();
    await this.statement(`INSERT INTO reports (report_id, secret_hash, content_hash, state, attempt, attempt_at, sent, issue_number,
      issue_url, approved_at, created_at, updated_at) VALUES (?,?,?,'delivered',0,?,0,?,?,?,?,?) ON CONFLICT(report_id) DO NOTHING`,
    r.reportId, r.secretHash, r.contentHash, now, issue.number, issue.url, r.approvedAt, now, now).run();
  }
  // Authoritative non-creation: GitHub refused (sent=1 allowed), or the attempt never sent.
  async notCreated(id, attemptId, reason, retryAt, requireUnsent = false) {
    await this.statement(`UPDATE reports SET state='not_created', reason=?, retry_at=?, updated_at=? WHERE report_id=? AND attempt_id=?
      AND state='pending'${requireUnsent ? ' AND sent=0 AND lease_until <= ?' : ''}`,
    reason, retryAt, this.now(), id, attemptId, ...(requireUnsent ? [this.now()] : [])).run();
  }
  async unconfirm(id, attemptId) {
    await this.statement(`UPDATE reports SET state='unconfirmed', updated_at=? WHERE report_id=? AND attempt_id=? AND state='pending' AND sent=1`,
      this.now(), id, attemptId).run();
  }
  // At most one reconciliation listing per report per interval, whoever asks.
  async claimCheck(id, interval) {
    const now = this.now();
    const result = await this.statement('UPDATE reports SET checked_at=? WHERE report_id=? AND checked_at <= ?', now, id, now - interval).run();
    return result.meta.changes === 1;
  }
  // The operator's list: reports whose delivery could not be confirmed.
  async unconfirmed() {
    return (await this.statement("SELECT report_id, attempt, attempt_at, approved_at FROM reports WHERE state='unconfirmed' ORDER BY attempt_at").all()).results;
  }
}
