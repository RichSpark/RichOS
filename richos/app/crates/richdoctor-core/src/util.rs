//! Small shared helpers: time, randomness, owner-only atomic files.
use std::io::{self, Read, Write};
use std::path::Path;

pub fn now_millis() -> u64 {
    richos_core::util::now_millis()
}

/// `n` bytes from the operating system's random source. A failure is an error, never a
/// weaker fallback: report IDs and receipt secrets must be unguessable (PRD §7).
pub fn random_bytes(n: usize) -> io::Result<Vec<u8>> {
    let mut out = vec![0u8; n];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut out)?;
    Ok(out)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex(&sha2::Sha256::digest(bytes))
}

/// Unpadded base64url (RFC 4648 §5): 32 bytes are 43 characters.
pub fn base64url(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 2);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        let take = chunk.len() + 1;
        for i in 0..take {
            out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

/// Standard base64 with padding (RFC 4648 §4), the encoding an image block's `data` uses.
pub fn base64_std(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decode standard base64 (padding optional); `None` on any character outside the alphabet.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let bytes: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let trimmed: &[u8] = {
        let mut end = bytes.len();
        while end > 0 && bytes[end - 1] == b'=' {
            end -= 1;
        }
        &bytes[..end]
    };
    let mut out = Vec::with_capacity(trimmed.len() * 3 / 4);
    for chunk in trimmed.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i);
        }
        let take = match chunk.len() {
            4 => 3,
            3 => 2,
            2 => 1,
            _ => return None,
        };
        for i in 0..take {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    Some(out)
}

/// A short random identifier with a prefix: `ev-1a2b3c4d5e6f`.
pub fn new_id(prefix: &str) -> io::Result<String> {
    Ok(format!("{prefix}-{}", hex(&random_bytes(6)?)))
}

/// Create `dir` and every missing parent, owner-only (0700).
pub fn private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

/// Write `bytes` to `path` atomically, owner-only (0600): a temporary file in the same folder,
/// synced, then renamed over the old one. A reader sees the old bytes or the new ones, never a
/// mix, and a failure leaves the old file as it was.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let dir = path.parent().ok_or_else(|| io::Error::other("no parent folder"))?;
    private_dir(dir)?;
    let tmp = dir.join(format!(".{}.{}.tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or("file"), hex(&random_bytes(4)?)));
    let result = (|| {
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        if let Ok(d) = std::fs::File::open(dir) {
            d.sync_all().ok();
        }
        Ok(())
    })();
    if result.is_err() {
        std::fs::remove_file(&tmp).ok();
    }
    result
}

/// Whether an I/O error means the disk is full (PRD §8: "Local disk full/unwritable").
pub fn is_disk_full(error: &io::Error) -> bool {
    error.raw_os_error() == Some(libc::ENOSPC) || error.raw_os_error() == Some(libc::EDQUOT)
}

/// `1757493852000` -> `2026-09-10T08:44:12Z`, the RichOS logs' own stamp.
pub fn utc_stamp(millis: u64) -> String {
    let secs = millis / 1000;
    let (y, m, d) = civil(secs as i64 / 86_400);
    let tod = secs % 86_400;
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", tod / 3600, tod % 3600 / 60, tod % 60)
}

/// Howard Hinnant's `civil_from_days`, exact for every date this program sees.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

/// The inverse of [`civil`]: days since 1970-01-01.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `2026-09-10T08:44:12Z` or `2026-09-10T08:44:12.123Z` -> epoch millis. `None` for anything
/// else: an unreadable stamp is reported, never guessed.
pub fn parse_utc_stamp(s: &str) -> Option<u64> {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || !s.ends_with('Z') {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<u64>().ok();
    let (y, mo, d, h, mi, se) = (num(0..4)?, num(5..7)?, num(8..10)?, num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    let frac = match &s[19..s.len() - 1] {
        "" => 0,
        f if f.starts_with('.') && f.len() > 1 && f[1..].chars().all(|c| c.is_ascii_digit()) => {
            let digits = &f[1..f.len().min(4)];
            digits.parse::<u64>().ok()? * 10u64.pow(3 - digits.len() as u32)
        }
        _ => return None,
    };
    let days = days_from_civil(y as i64, mo as u32, d as u32);
    if days < 0 {
        return None;
    }
    Some((days as u64 * 86_400 + h * 3600 + mi * 60 + se) * 1000 + frac)
}

/// This Mac's offset from UTC at `millis`, in seconds, from the C library's own time zone
/// rules (so daylight saving time is right for that instant).
pub fn local_offset_seconds(millis: u64) -> i64 {
    let t = (millis / 1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: localtime_r writes only into `tm`, which lives on this stack frame.
    let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
    if ok { tm.tm_gmtoff } else { 0 }
}

/// The local wall clock at `millis` as a person reads it: "8:43 AM".
pub fn local_clock(millis: u64) -> String {
    let local = millis as i64 / 1000 + local_offset_seconds(millis);
    let tod = local.rem_euclid(86_400);
    let (h, m) = (tod / 3600, tod % 3600 / 60);
    let (h12, ampm) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{h12}:{m:02} {ampm}")
}

/// The local date at `millis`: "October 22, 2026".
pub fn local_date(millis: u64) -> String {
    const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let local = millis as i64 / 1000 + local_offset_seconds(millis);
    let (y, m, d) = civil(local.div_euclid(86_400));
    format!("{} {d}, {y}", MONTHS[(m - 1) as usize])
}

/// The time zone as an offset a reader in any zone understands: "UTC-07:00".
pub fn utc_offset_label(millis: u64) -> String {
    let off = local_offset_seconds(millis);
    let sign = if off < 0 { '-' } else { '+' };
    let off = off.abs();
    format!("UTC{sign}{:02}:{:02}", off / 3600, off % 3600 / 60)
}

/// Read at most `cap` bytes of a regular file, from its END when it is larger (the most recent
/// lines of a log), within `within`. Never follows a link at the last component, never opens a
/// device or a pipe, and a slow or growing file cannot hold the caller past the bound (PRD §5:
/// 5-second read limit, 2 MiB input cap, declared truncation).
pub fn bounded_tail(path: &Path, cap: u64, within: std::time::Duration) -> io::Result<Bounded> {
    bounded_read(path, cap, within, true)
}

/// [`bounded_tail`] for a document that must be read from its start (a crash report is one
/// JSON document): a file larger than `cap` reads its first `cap` bytes, marked truncated.
pub fn bounded_head(path: &Path, cap: u64, within: std::time::Duration) -> io::Result<Bounded> {
    bounded_read(path, cap, within, false)
}

fn bounded_read(path: &Path, cap: u64, within: std::time::Duration, from_end: bool) -> io::Result<Bounded> {
    use std::os::unix::fs::OpenOptionsExt;
    let path = path.to_path_buf();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> io::Result<Bounded> {
            let mut f = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(&path)?;
            let meta = f.metadata()?;
            if !meta.is_file() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a regular file"));
            }
            let len = meta.len();
            let start = if from_end { len.saturating_sub(cap) } else { 0 };
            if start > 0 {
                use std::io::Seek;
                f.seek(io::SeekFrom::Start(start))?;
            }
            let mut bytes = Vec::with_capacity(len.min(cap) as usize);
            f.take(cap).read_to_end(&mut bytes)?;
            let mut truncated = start > 0;
            if truncated {
                // Start on a whole line, so no half line is ever presented as evidence.
                if let Some(nl) = bytes.iter().position(|&b| b == b'\n') {
                    bytes.drain(..=nl);
                }
            }
            if len > cap {
                truncated = true;
            }
            Ok(Bounded { bytes, truncated, modified_ms: meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64) })
        })();
        tx.send(result).ok();
    });
    rx.recv_timeout(within).map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "the read did not finish in time"))?
}

/// What [`bounded_tail`] read.
#[derive(Debug, Clone)]
pub struct Bounded {
    pub bytes: Vec<u8>,
    /// The file held more than was read.
    pub truncated: bool,
    pub modified_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_of_32_bytes_is_43_characters_without_padding() {
        let s = base64url(&[0xffu8; 32]);
        assert_eq!(s.len(), 43);
        assert!(s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_eq!(base64url(b"Man"), "TWFu");
        assert_eq!(base64url(b"Ma"), "TWE");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn standard_base64_round_trips() {
        assert_eq!(base64_std(b"Man"), "TWFu");
        assert_eq!(base64_std(b"Ma"), "TWE=");
        assert_eq!(base64_std(b"M"), "TQ==");
        for n in 0..40u8 {
            let v: Vec<u8> = (0..n).map(|i| i.wrapping_mul(37)).collect();
            assert_eq!(base64_decode(&base64_std(&v)).unwrap(), v);
        }
        assert!(base64_decode("TW?u").is_none());
    }

    #[test]
    fn utc_stamps_round_trip() {
        let ms = 1_757_493_852_000;
        assert_eq!(utc_stamp(ms), "2025-09-10T08:44:12Z");
        assert_eq!(parse_utc_stamp("2025-09-10T08:44:12Z"), Some(ms));
        assert_eq!(parse_utc_stamp("2025-09-10T08:44:12.500Z"), Some(ms + 500));
        assert_eq!(parse_utc_stamp("2025-13-10T08:44:12Z"), None);
        assert_eq!(parse_utc_stamp("yesterday"), None);
    }

    #[test]
    fn private_files_are_owner_only_and_replace_atomically() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("rd-util-{}", hex(&random_bytes(4).unwrap())));
        let path = dir.join("a.json");
        write_private(&path, b"one").unwrap();
        write_private(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temporary file is left behind");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_bounded_read_keeps_the_end_on_a_whole_line_and_says_it_was_cut() {
        let dir = std::env::temp_dir().join(format!("rd-tail-{}", hex(&random_bytes(4).unwrap())));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("log");
        std::fs::write(&path, "first line\nsecond line\nthird line\n").unwrap();
        let read = bounded_tail(&path, 20, std::time::Duration::from_secs(5)).unwrap();
        assert!(read.truncated);
        assert_eq!(String::from_utf8(read.bytes).unwrap(), "third line\n");
        let whole = bounded_tail(&path, 1024, std::time::Duration::from_secs(5)).unwrap();
        assert!(!whole.truncated);
        std::os::unix::fs::symlink(&path, dir.join("link")).unwrap();
        assert!(bounded_tail(&dir.join("link"), 1024, std::time::Duration::from_secs(5)).is_err(), "a link is never followed");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
