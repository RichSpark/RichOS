// An in-memory stand-in for the R2 calls the report relay makes (head, get, put, delete), with
// one-shot faults. The workerd test uses Miniflare's real local R2 instead.
export class FakeR2 {
  constructor() { this.objects = new Map(); this.faults = []; this.puts = 0; this.deletes = []; }
  fault(name) { this.faults.push(name); }
  take(name) { const i = this.faults.indexOf(name); if (i < 0) return false; this.faults.splice(i, 1); return true; }
  async head(key) {
    const o = this.objects.get(key);
    return o ? { key, size: o.bytes.byteLength, customMetadata: { ...o.customMetadata }, httpMetadata: { ...o.httpMetadata } } : null;
  }
  async get(key) {
    const o = this.objects.get(key);
    return o ? { ...(await this.head(key)), body: new Blob([o.bytes]).stream() } : null;
  }
  async put(key, value, options = {}) {
    if (this.take('put')) throw Error('r2_unavailable');
    const bytes = new Uint8Array(value);
    if (options.sha256) {
      const digest = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), b => b.toString(16).padStart(2, '0')).join('');
      if (digest !== options.sha256) throw Error('r2_checksum_mismatch');
    }
    this.puts++;
    this.objects.set(key, { bytes, customMetadata: options.customMetadata || {}, httpMetadata: options.httpMetadata || {} });
    return this.head(key);
  }
  async delete(keys) {
    if (this.take('delete')) throw Error('r2_unavailable');
    for (const k of [].concat(keys)) { this.deletes.push(k); this.objects.delete(k); }
  }
}
