//! One native adapter, supplied inside the signed app. No shell, clipboard or UI automation.
use richos_core::{
    cleaner::{Entry, Files, Identity, Result, Staged},
    owned_process::OwnedChild,
};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct Native {
    pub directory: PathBuf,
}
impl Native {
    pub fn bundled() -> Result<Self> {
        #[cfg(debug_assertions)]
        if let Some(path) = std::env::var_os("RICHOS_CLEANER_RESOURCES") {
            return Ok(Self {
                directory: PathBuf::from(path),
            });
        }
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let resources = exe
            .parent()
            .and_then(Path::parent)
            .ok_or("RichCleaner's app resources are unavailable")?
            .join("Resources/RichCleaner");
        if !resources.join("native").is_file() {
            return Err(
                "This RichOS build does not contain RichCleaner. Install the complete app".into(),
            );
        }
        Ok(Self {
            directory: resources,
        })
    }
    pub fn call(&self, request: Value, bound: Duration, cancel: &AtomicBool) -> Result<Value> {
        let mut command = Command::new(self.directory.join("native"));
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        OwnedChild::configure(&mut command);
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let mut stdin = child.stdin.take().ok_or("Native input unavailable")?;
        let output = child.stdout.take().ok_or("Native output unavailable")?;
        let reader = std::thread::spawn(move || {
            let mut bytes = vec![];
            output
                .take(256 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
                .map_err(|e| e.to_string())
        });
        let mut child = OwnedChild::new(child);
        let start = Instant::now();
        let bytes = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        // A stalled helper must not hold the one-second focus lookup inside a
        // blocking pipe write. Cancellation kills the owned group and joins both pipes.
        let sender = std::thread::spawn(move || stdin.write_all(&bytes).map_err(|e| e.to_string()));
        let status = loop {
            if cancel.load(Ordering::Acquire) || start.elapsed() >= bound {
                let stopped = child.kill();
                let reaped = child.wait();
                let read = reader.join();
                let _sent = sender.join();
                return Err(format!("Native operation canceled or timed out (stop: {stopped:?}, reap: {reaped:?}, reader: {})",read.is_ok()));
            }
            match child.try_wait().map_err(|e| e.to_string())? {
                Some(status) => break status,
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        };
        sender.join().map_err(|_| "Native sender failed")??;
        let bytes = reader.join().map_err(|_| "Native reader failed")??;
        if bytes.len() > 256 * 1024 {
            return Err("Native answer exceeded its size limit".into());
        }
        let answer: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if !status.success() || answer.get("error").is_some() {
            return Err(answer["error"]
                .as_str()
                .unwrap_or("Native operation failed")
                .into());
        }
        answer
            .get("ok")
            .cloned()
            .ok_or("Native answer is missing".into())
    }
    pub fn request(&self, value: Value) -> Result<Value> {
        self.call(value, Duration::from_secs(30), &AtomicBool::new(false))
    }
    pub fn context(&self, cancel: &AtomicBool) -> Result<Value> {
        self.call(json!({"action":"context"}), Duration::from_secs(1), cancel)
    }
    pub fn open_location(
        &self, path: &Path, identity: &Identity, reveal: bool, downloading: impl FnOnce(),
    ) -> Result<PathBuf> {
        let action = if reveal { "resolve" } else { "open-check" };
        let mut answer = self.request(json!({"action":action,"path":path,"identity":identity}))?;
        if answer["download"] == true {
            downloading();
            answer = self.request(json!({"action":"open-location","path":path,"identity":identity}))?;
        }
        Ok(PathBuf::from(
            answer["path"].as_str().ok_or("Locate the original file")?,
        ))
    }
    pub fn stage_copy(&self, e: &Entry) -> Result<Staged> {
        let value =
            self.request(json!({"action":"reserve","path":e.path,"identity":e.identity}))?;
        Ok(Staged {
            path: PathBuf::from(value["path"].as_str().ok_or("No staged file")?),
            identity: serde_json::from_value(value["identity"].clone())
                .map_err(|e| e.to_string())?,
            projection: e.projection.clone(),
        })
    }
}
impl Files for Native {
    fn inspect(&self, path: &Path) -> Result<Identity> {
        serde_json::from_value(self.request(json!({"action":"inspect","path":path}))?)
            .map_err(|e| e.to_string())
    }
    fn resolve(&self, path: &Path, identity: &Identity) -> Result<PathBuf> {
        self.open_location(path, identity, true, || {})
    }
    fn destination(
        &self,
        root: &Path,
        parts: &[String],
        name: &std::ffi::OsStr,
    ) -> Result<PathBuf> {
        let v=self.request(json!({"action":"destination","root":root,"parts":parts,"name":name.to_str().ok_or("Invalid filename")?}))?;
        Ok(PathBuf::from(
            v["path"].as_str().ok_or("Missing destination")?,
        ))
    }
    fn stage(&self, e: &Entry) -> Result<Option<Staged>> {
        let ext = e
            .path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !richos_core::cleaner::EXTENSIONS.contains(&ext.as_str()) {
            return Ok(None);
        }
        self.stage_copy(e).map(Some)
    }
    fn annotate(&self, e: &Entry, stage: &Staged) -> Result<Staged> {
        let mut staged = stage.clone();
        self.request(json!({"action":"materialize","path":e.path,"identity":e.identity,"staged":staged.path}))?;
        if e.path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            let protection = self.request(json!({"action":"pdf-protection","path":staged.path}))?;
            if protection["protected"] == true {
                staged.projection = richos_core::cleaner::Projection {
                    reason: protection["reason"]
                        .as_str()
                        .unwrap_or("Protected PDF")
                        .into(),
                    ..Default::default()
                };
                staged.identity = self.inspect(&staged.path)?;
                return Ok(staged);
            }
        }
        staged.projection = crate::cleaner_metadata::write(&staged.path, e, &self.directory)?;
        if staged.projection.mode == "embedded" {
            let ext = e
                .path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            crate::cleaner_metadata::verify_note(&staged.path, &staged.projection, &ext)?;
            crate::cleaner_metadata::verify_content(&e.path, &staged.path, &ext, &self.directory)?;
        }
        staged.identity = serde_json::from_value(self.request(
            json!({"action":"attributes","path":e.path,"identity":e.identity,"staged":staged.path}),
        )?)
        .map_err(|e| e.to_string())?;
        Ok(staged)
    }
    fn replace(&self, e: &Entry, staged: &Staged) -> Result<Identity> {
        serde_json::from_value(self.request(json!({"action":"replace","path":e.path,"identity":e.identity,"staged":staged.path,"stagedIdentity":staged.identity}))?).map_err(|e|e.to_string())
    }
    fn rename(&self, source: &Path, destination: &Path, identity: &Identity) -> Result<Identity> {
        serde_json::from_value(self.request(
            json!({"action":"rename","path":source,"destination":destination,"identity":identity}),
        )?)
        .map_err(|e| e.to_string())
    }
    fn cleanup(&self, staged: &Staged) -> Result<()> {
        self.request(json!({"action":"cleanup","path":staged.path,"identity":staged.identity}))
            .map(|_| ())
    }
}

/// Bounded, owned metadata helper. Only staged files are passed to it, never user paths
/// interpreted as command syntax. A failed/timed-out writer cannot replace the original.
pub fn tool_output(mut command: Command, bound: Duration) -> Result<Vec<u8>> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    OwnedChild::configure(&mut command);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("Writer output unavailable")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = vec![];
        stdout
            .take(256 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
            .map_err(|e| e.to_string())
    });
    let mut child = OwnedChild::new(child);
    let start = Instant::now();
    let status = loop {
        if start.elapsed() >= bound {
            let stop = child.kill();
            let reap = child.wait();
            let read = reader.join();
            return Err(format!(
                "Metadata writing timed out (stop: {stop:?}, reap: {reap:?}, reader: {})",
                read.is_ok()
            ));
        }
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(s) => break s,
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    let bytes = reader.join().map_err(|_| "Writer reader failed")??;
    if !status.success() {
        return Err("This file could not be annotated safely. Your note is saved in RichOS".into());
    }
    if bytes.len() > 256 * 1024 {
        return Err("Metadata answer exceeds the safety limit".into());
    }
    Ok(bytes)
}
