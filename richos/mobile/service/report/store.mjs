// D1 receipt and delivery state. Every write that decides who may call GitHub, store or delete is
// one conditional statement or one transaction (batch), so concurrent requests for one report ID
// serialize in the database, across isolates. `busy` is the per-receipt lock that upload,
// completion, retirement and cleanup all take; `lease_until` bounds one GitHub attempt.
// Never-begun (or refused) receipts after the abandonment time; retired ones at once (PRD v6 handover 2).
const ELIGIBLE = "(state='retired' OR (state IN ('receiving','ready','not_created') AND active_at < ?1))";
export class Store {
  constructor(db, now = Date.now) { this.db = db; this.now = now; }
  statement(sql, ...args) { return this.db.prepare(sql).bind(...args); }
  get(id) { return this.statement('SELECT * FROM reports WHERE report_id=?', id).first(); }
  async attachments(id) { return (await this.statement('SELECT * FROM attachments WHERE report_id=? ORDER BY ord', id).all()).results; }
  control() { return this.statement('SELECT paused, daily_cap, daily_bytes_cap FROM control WHERE id=1').first(); }

  // One creation slot of today's global cap (UTC day), or false when the cap is used up.
  async reserve(day, cap) {
    const result = await this.statement(`INSERT INTO daily (day, creates, bytes) SELECT ?1, 1, 0 WHERE ?2 > 0
      ON CONFLICT(day) DO UPDATE SET creates = creates + 1 WHERE creates < ?2`, day, cap).run();
    return result.meta.changes === 1;
  }
  async refund(day) { await this.statement('UPDATE daily SET creates = creates - 1 WHERE day=? AND creates > 0', day).run(); }
  // Today's stored screenshot bytes stay within the operator's cap.
  async reserveBytes(day, bytes, cap) {
    const result = await this.statement(`INSERT INTO daily (day, creates, bytes) SELECT ?1, 0, ?2 WHERE ?2 <= ?3
      ON CONFLICT(day) DO UPDATE SET bytes = bytes + ?2 WHERE bytes + ?2 <= ?3`, day, bytes, cap).run();
    return result.meta.changes === 1;
  }

  // The receipt, its words until posting, and its approved manifest, in one transaction.
  async register(r) {
    const now = this.now(), state = r.manifest.length ? 'receiving' : 'ready';
    const results = await this.db.batch([
      this.statement(`INSERT INTO reports (report_id, secret_hash, content_hash, protocol, state, title, body, approved_at, active_at, created_at, updated_at)
        VALUES (?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(report_id) DO NOTHING`, r.reportId, r.secretHash, r.contentHash, r.schema, state, r.title, r.body, r.approvedAt, now, now, now),
      ...r.manifest.map(item => this.statement(`INSERT INTO attachments (report_id, attachment_id, ord, media_type, width, height, bytes, sha256)
        SELECT ?1,?2,?3,?4,?5,?6,?7,?8 WHERE EXISTS (SELECT 1 FROM reports WHERE report_id=?1 AND content_hash=?9) ON CONFLICT DO NOTHING`,
      r.reportId, item.attachment_id, item.order, item.media_type, item.width, item.height, item.bytes, item.sha256, r.contentHash)),
    ]);
    return results[0].meta.changes === 1;
  }

  async acquire(id, ms = 30_000) {
    const token = crypto.randomUUID(), now = this.now();
    const result = await this.statement('UPDATE reports SET busy_id=?, busy_until=? WHERE report_id=? AND busy_until < ?', token, now + ms, id, now).run();
    return result.meta.changes === 1 ? token : null;
  }
  async release(id, token) { await this.statement('UPDATE reports SET busy_id=NULL, busy_until=0 WHERE report_id=? AND busy_id=?', id, token).run(); }

  // An attachment is stored; the receipt becomes ready when none is missing. A finished cleanup
  // is reset, so a resumed report can be cleaned up again later.
  async markStored(id, attachmentId, token) {
    const now = this.now();
    const results = await this.db.batch([
      this.statement(`UPDATE attachments SET stored=1, deleted_at=NULL WHERE report_id=?1 AND attachment_id=?2
        AND EXISTS (SELECT 1 FROM reports WHERE report_id=?1 AND busy_id=?3 AND state IN ('receiving','ready','not_created') AND (cleanup IS NULL OR cleanup='done'))`, id, attachmentId, token),
      this.statement(`UPDATE reports SET state=CASE WHEN state='receiving' AND NOT EXISTS (SELECT 1 FROM attachments WHERE report_id=?1 AND stored=0) THEN 'ready' ELSE state END,
        cleanup=NULL, active_at=?2, updated_at=?2 WHERE report_id=?1 AND busy_id=?3 AND state IN ('receiving','ready','not_created') AND (cleanup IS NULL OR cleanup='done')`, id, now, token),
    ]);
    return results[0].meta.changes === 1;
  }
  // Completion found these attachments absent from storage: they are missing again.
  async markMissing(id, attachmentIds, token) {
    await this.db.batch([
      ...attachmentIds.map(a => this.statement(`UPDATE attachments SET stored=0 WHERE report_id=?1 AND attachment_id=?2
        AND EXISTS (SELECT 1 FROM reports WHERE report_id=?1 AND busy_id=?3)`, id, a, token)),
      this.statement("UPDATE reports SET state=CASE WHEN state='ready' THEN 'receiving' ELSE state END, updated_at=? WHERE report_id=? AND busy_id=?", this.now(), id, token),
    ]);
  }
  async touch(id) { const now = this.now(); await this.statement('UPDATE reports SET active_at=?, updated_at=? WHERE report_id=?', now, now, id).run(); }

  // The durable pending record, written before GitHub is called: the one new attempt a ready
  // receipt, or a persisted authoritative non-creation, permits. Only under the receipt's lock.
  async claim(id, fromAttempt, attemptId, lease, token) {
    const now = this.now();
    const result = await this.statement(`UPDATE reports SET state='pending', attempt=attempt+1, attempt_id=?, attempt_at=?, sent=0,
      lease_until=?, checked_at=0, reason=NULL, retry_at=NULL, active_at=?, updated_at=? WHERE report_id=? AND state IN ('ready','not_created')
      AND attempt=? AND busy_id=?`, attemptId, now, now + lease, now, now, id, fromAttempt, token).run();
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
  // Posted: the words are dropped (GitHub has them), the screenshots are kept for `retain` ms.
  async deliver(id, issue, retain) {
    const now = this.now();
    await this.statement(`UPDATE reports SET state='delivered', issue_number=?, issue_url=?, reason=NULL, retry_at=NULL, title=NULL, body=NULL,
      delivered_at=?, expires_at=?, updated_at=? WHERE report_id=? AND state IN ('pending','unconfirmed')`, issue.number, issue.url, now, now + retain, now, id).run();
  }
  // A delivered record for an older unknown ID whose footer was found before any create, with its
  // approved manifest in the same transaction, so its screenshots are counted, served and expire.
  // `stored` names the attachment IDs whose objects are in the bucket; the count is re-verified on view.
  async adopt(r, issue, retain, stored = []) {
    const now = this.now();
    await this.db.batch([
      this.statement(`INSERT INTO reports (report_id, secret_hash, content_hash, protocol, state, issue_number, issue_url, approved_at,
        delivered_at, expires_at, active_at, created_at, updated_at) VALUES (?,?,?,?,'delivered',?,?,?,?,?,?,?,?) ON CONFLICT(report_id) DO NOTHING`,
      r.reportId, r.secretHash, r.contentHash, r.schema, issue.number, issue.url, r.approvedAt, now, now + retain, now, now, now),
      ...this.manifestRows(r, stored),
    ]);
  }
  // An older unknown ID whose footer was not found: no match is not proof that nothing was created
  // (its receipt may have been lost), so it is recorded unconfirmed, on the operator's list, and
  // never created. attempt=0 marks a receipt this relay never attempted; attempt_at holds the
  // first listing's look-back, so a status check lists as far back. Just listed: checked_at is now.
  async holdUnknown(r, since, stored = []) {
    const now = this.now();
    await this.db.batch([
      this.statement(`INSERT INTO reports (report_id, secret_hash, content_hash, protocol, state, title, body, attempt_at, checked_at, approved_at,
        active_at, created_at, updated_at) VALUES (?,?,?,?,'unconfirmed',?,?,?,?,?,?,?,?) ON CONFLICT(report_id) DO NOTHING`,
      r.reportId, r.secretHash, r.contentHash, r.schema, r.title, r.body, since, now, r.approvedAt, now, now, now),
      ...this.manifestRows(r, stored),
    ]);
  }
  manifestRows(r, stored) {
    return r.manifest.map(item => this.statement(`INSERT INTO attachments (report_id, attachment_id, ord, media_type, width, height, bytes, sha256, stored)
      SELECT ?1,?2,?3,?4,?5,?6,?7,?8,?9 WHERE EXISTS (SELECT 1 FROM reports WHERE report_id=?1 AND content_hash=?10) ON CONFLICT DO NOTHING`,
    r.reportId, item.attachment_id, item.order, item.media_type, item.width, item.height, item.bytes, item.sha256, stored.includes(item.attachment_id) ? 1 : 0, r.contentHash));
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

  // Retirement. An ID the relay never saw gets a retired receipt bound to this secret, so a late
  // registration cannot revive it. A known receipt retires only while nothing was created.
  async retireUnseen(id, secretHash) {
    const now = this.now();
    await this.statement(`INSERT INTO reports (report_id, secret_hash, protocol, state, active_at, created_at, updated_at)
      VALUES (?,?,0,'retired',?,?,?) ON CONFLICT(report_id) DO NOTHING`, id, secretHash, now, now, now).run();
  }
  async retire(id, token) {
    const result = await this.statement(`UPDATE reports SET state='retired', title=NULL, body=NULL, updated_at=? WHERE report_id=?
      AND state IN ('receiving','ready','not_created') AND busy_id=?`, this.now(), id, token).run();
    return result.meta.changes === 1;
  }

  // Cleanup: abandoned uploads of a receipt that never began creation (or was refused) after
  // `abandon` ms without activity, a retired receipt's at once, and delivered screenshots past
  // expires_at. A claimed cleanup that never finished is always resumed first.
  async cleanupCandidates(abandon, limit = 25) {
    const now = this.now();
    return (await this.statement(`SELECT report_id FROM reports r WHERE (cleanup='claimed' OR (cleanup IS NULL AND (
      ${ELIGIBLE} OR (state='delivered' AND expires_at < ?2))))
      AND EXISTS (SELECT 1 FROM attachments a WHERE a.report_id=r.report_id AND a.deleted_at IS NULL) LIMIT ?3`, now - abandon, now, limit).all()).results;
  }
  // Re-checked under the lock: the claim and "unavailable" are one transaction, before any delete.
  async claimCleanup(id, token, abandon) {
    const now = this.now();
    const results = await this.db.batch([
      this.statement(`UPDATE reports SET cleanup='claimed', updated_at=?2 WHERE report_id=?3 AND busy_id=?4 AND (cleanup='claimed' OR (cleanup IS NULL AND (
        ${ELIGIBLE} OR (state='delivered' AND expires_at < ?2))))`, now - abandon, now, id, token),
      this.statement(`UPDATE attachments SET stored=0 WHERE report_id=?1 AND EXISTS (SELECT 1 FROM reports WHERE report_id=?1 AND cleanup='claimed' AND busy_id=?2)`, id, token),
      this.statement(`UPDATE reports SET state=CASE WHEN state='ready' THEN 'receiving' ELSE state END WHERE report_id=? AND cleanup='claimed' AND busy_id=?`, id, token),
    ]);
    return (await this.get(id))?.cleanup === 'claimed' && results.length === 3;
  }
  async finishCleanup(id, token) {
    const now = this.now();
    await this.db.batch([
      this.statement(`UPDATE attachments SET deleted_at=?1 WHERE report_id=?2 AND deleted_at IS NULL
        AND EXISTS (SELECT 1 FROM reports WHERE report_id=?2 AND cleanup='claimed' AND busy_id=?3)`, now, id, token),
      this.statement("UPDATE reports SET cleanup='done', updated_at=? WHERE report_id=? AND cleanup='claimed' AND busy_id=?", now, id, token),
    ]);
  }
  // The operator's list: reports whose delivery could not be confirmed.
  async unconfirmed() {
    return (await this.statement("SELECT report_id, attempt, attempt_at, approved_at FROM reports WHERE state='unconfirmed' ORDER BY attempt_at").all()).results;
  }
}
