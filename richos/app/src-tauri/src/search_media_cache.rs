//! Memoize portable metadata without confusing it with the RichCleaner catalog.
use std::fs::{DirBuilder, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::Path;

const MAX_RECORD_BYTES: usize = 256 * 1024;

fn version(meta: &Metadata) -> String {
    format!("{}:{}:{}:{}:{}:{}:{}", meta.dev(), meta.ino(), meta.len(),
        meta.mtime(), meta.mtime_nsec(), meta.ctime(), meta.ctime_nsec())
}

pub(crate) fn read(
    path: &Path, metadata: &Metadata, cache: Option<&Path>, source: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    let Some(cache) = cache else { return source() };
    // Bump the cache format when the set of extracted metadata fields changes.
    let directory = cache.join("portable-metadata-v1");
    let key = richos_search::corpus::content_hash(path.as_os_str().as_encoded_bytes());
    let record = directory.join(key);
    let stamp = version(metadata);
    if let Some(text) = read_record(&record, &stamp) {
        return Ok(text);
    }
    let text = source()?;
    if text.len() + stamp.len() < MAX_RECORD_BYTES
        && std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file() && version(&meta) == stamp)
    {
        let result = write_record(&record, &stamp, &text);
        if result.is_err() {
            eprintln!("[richos] document search: could not cache portable metadata");
        }
    }
    Ok(text)
}

fn read_record(path: &Path, stamp: &str) -> Option<String> {
    let mut contents = String::new();
    let file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path).ok()?;
    if !file.metadata().ok()?.is_file() { return None; }
    file.take((MAX_RECORD_BYTES + 1) as u64).read_to_string(&mut contents).ok()?;
    let (version, text) = contents.split_once('\n')?;
    (version == stamp && contents.len() <= MAX_RECORD_BYTES).then(|| text.to_owned())
}

fn write_record(path: &Path, stamp: &str, text: &str) -> std::io::Result<()> {
    let directory = path.parent().expect("portable metadata records have a cache directory");
    DirBuilder::new().recursive(true).mode(0o700).create(directory)?;
    let nonce = richdoctor_core::util::base64url(&richdoctor_core::util::random_bytes(12)?);
    let staged = path.with_extension(format!("{nonce}.tmp"));
    let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&staged)?;
    let result = (|| {
        writeln!(file, "{stamp}")?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&staged, path)
    })();
    if result.is_err() {
        let _removed = std::fs::remove_file(staged);
    }
    result
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_successful_extractions_of_the_current_file_version_are_reused() {
        let nonce = richdoctor_core::util::base64url(&richdoctor_core::util::random_bytes(12).unwrap());
        let base = std::env::temp_dir().join(format!("richfinder-metadata-test-{nonce}"));
        std::fs::create_dir(&base).unwrap();
        let source = base.join("example.png");
        let cache = base.join("cache");
        let lookup = |answer: Option<Result<String, String>>| {
            read(&source, &std::fs::metadata(&source).unwrap(), Some(&cache), || {
                answer.expect("a cached file must not be reopened")
            })
        };
        std::fs::write(&source, b"first").unwrap();
        assert!(lookup(Some(Err("failed".into()))).is_err());
        assert_eq!(lookup(Some(Ok("first note".into()))).unwrap(), "first note");
        assert_eq!(lookup(None).unwrap(), "first note");

        std::fs::write(&source, b"changed file size").unwrap();
        assert_eq!(lookup(Some(Ok("changed note".into()))).unwrap(), "changed note");
        assert_eq!(lookup(None).unwrap(), "changed note");
        std::fs::remove_dir_all(base).unwrap();
    }
}
