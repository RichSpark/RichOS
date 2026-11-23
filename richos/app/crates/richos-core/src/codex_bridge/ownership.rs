//! Ownership of automatic Codex conversations and repair of the original cutover.
use super::{component, AUTOMATIC};
use crate::entity::{EntityId, EntityRegistry, EntityResolveError, EntityStatus, ThreadBinding};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MOVE: &str = "legacy-company-move.json";
const OLD_TARGET: &str = "automatic.json";
const LEGACY_TARGET: &str = "legacy-target.json";

fn invalid(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

fn read_record<T: serde::de::DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    use std::os::unix::fs::OpenOptionsExt;
    match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) if file.metadata()?.is_file() => {
            Ok(Some(serde_json::from_reader(file.take(256 * 1024))?))
        }
        Ok(_) => Err(invalid("Codex record is not an ordinary file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn atomic(path: &Path, write: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let temporary = path.with_extension(format!("{}.incoming", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    let result = (|| {
        write(&mut file)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(path.parent().unwrap())?.sync_all()
    })();
    if result.is_err() {
        drop(fs::remove_file(temporary));
    }
    result
}

fn save(path: &Path, value: &impl Serialize) -> io::Result<()> {
    atomic(path, |file| {
        serde_json::to_writer(&mut *file, value)?;
        file.write_all(b"\n")
    })
}

fn git(folder: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("/usr/bin/git")
        .arg("-C")
        .arg(folder)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output.status.success().then_some(())?;
    Some(String::from_utf8(output.stdout).ok()?.trim_end().to_owned())
}

/// The conventional Markdown channels predate project metadata and were created
/// for RichOS work. Identify the registered owner of the actual upstream repo.
/// Missing roots and roots registered to competing companies leave them held.
pub fn legacy_company(registry: &EntityRegistry) -> Option<EntityId> {
    let mut owners = registry.entities().iter().filter(|company| {
        company.status == EntityStatus::Active
            && company.roots.iter().any(|root| {
                git(root, &["config", "--local", "--get", "remote.origin.url"]).is_some_and(
                    |remote| {
                        matches!(
                            remote.to_ascii_lowercase().trim_end_matches(".git"),
                            "https://github.com/richspark/richos"
                                | "git@github.com:richspark/richos"
                                | "ssh://git@github.com/richspark/richos"
                        )
                    },
                ) && registry
                    .resolve_root(root)
                    .is_ok_and(|owner| owner.id == company.id)
            })
    });
    let owner = owners.next()?.id.clone();
    owners.next().is_none().then_some(owner)
}

pub fn publish_legacy_target(coordination: &Path, binding: &ThreadBinding) -> io::Result<()> {
    save(
        &coordination.join(LEGACY_TARGET),
        &Target {
            version: 1,
            entity_id: binding.entity_id().clone(),
            thread_id: binding.thread_id().into(),
        },
    )
}

/// Identity of a waiting pair is independent of the company it eventually gets.
pub struct AutomaticRequest {
    path: PathBuf,
    record: Value,
}

impl AutomaticRequest {
    pub fn read(coordination: &Path, locator: &Path) -> io::Result<Option<Self>> {
        let id = locator
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(invalid("Missing Codex pair"))?;
        component(id).map_err(|_| invalid("Invalid Codex pair"))?;
        if locator
            != coordination
                .join("connections")
                .join(AUTOMATIC)
                .join("codex")
                .join(id)
            || locator.canonicalize()? != locator
        {
            return Err(invalid("Invalid automatic Codex locator"));
        }
        let record: Value =
            read_record(&locator.join("connection.json"))?.ok_or(invalid("Missing Codex pair"))?;
        if record["version"] != 1
            || record["entity_id"] != AUTOMATIC
            || record["thread_id"] != "codex"
            || record["connection_id"] != id
        {
            return Err(invalid("Invalid automatic Codex pair"));
        }
        Ok(record.get("binding").is_none().then(|| Self {
            path: locator.into(),
            record,
        }))
    }

    pub fn company(&self, registry: &EntityRegistry) -> Option<EntityId> {
        let directory = Path::new(self.record["project_dir"].as_str()?)
            .canonicalize()
            .ok()?;
        let owner = match registry.resolve_root(&directory) {
            Ok(owner) => owner,
            Err(EntityResolveError::UnknownRoot(_)) => {
                let common = git(
                    &directory,
                    &["rev-parse", "--path-format=absolute", "--git-common-dir"],
                )?;
                let common = Path::new(&common);
                if common.file_name()? != ".git" {
                    return None;
                }
                registry.resolve_root(common.parent()?).ok()?
            }
            Err(_) => return None,
        };
        (owner.status == EntityStatus::Active).then(|| owner.id.clone())
    }

    pub fn bind(mut self, binding: &ThreadBinding) -> io::Result<()> {
        self.record["binding"] = json!({
            "entity_id": binding.entity_id(), "thread_id": binding.thread_id(),
        });
        save(&self.path.join("connection.json"), &self.record)
    }
}

#[derive(Clone, Deserialize, Serialize)]
struct Target {
    version: u32,
    entity_id: EntityId,
    thread_id: String,
}

impl Target {
    fn valid(&self) -> io::Result<()> {
        if self.version != 1 || component(&self.thread_id).is_err() {
            return Err(invalid("Invalid cutover target"));
        }
        Ok(())
    }

    fn owns(&self, value: &Value) -> bool {
        value["entity_id"] == self.entity_id.as_str() && value["thread_id"] == self.thread_id
    }
}

#[derive(Deserialize, Serialize)]
struct Move {
    version: u32,
    from: EntityId,
    to: EntityId,
    thread_id: String,
    data_dir: PathBuf,
    complete: bool,
}

impl Move {
    fn target(&self, entity_id: &EntityId) -> Target {
        Target {
            version: self.version,
            entity_id: entity_id.clone(),
            thread_id: self.thread_id.clone(),
        }
    }
}

fn lock(path: &Path) -> io::Result<File> {
    use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
    let file = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    // The Python receiver and coordinator use the same flock locks.
    // lock-busy: defer the repair to the next launch if another receiver owns the state.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(file)
}

fn conventional_legacy(coordination: &Path, endpoint: &Value) -> bool {
    endpoint["legacy_folder"].as_str().is_some_and(|folder| {
        ["rich-codex", "rich-codex-questions"]
            .iter()
            .any(|name| coordination.join(name) == Path::new(folder))
    })
}

fn directories(path: &Path) -> io::Result<Vec<PathBuf>> {
    let files = match fs::read_dir(path) {
        Ok(files) => files,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut dirs = Vec::new();
    for file in files {
        let file = file?;
        if file.file_type()?.is_dir() {
            dirs.push(file.path());
        }
    }
    Ok(dirs)
}

fn move_dir(from: &Path, to: &Path) -> io::Result<()> {
    if !from.try_exists()? {
        return Ok(());
    }
    if to.try_exists()? {
        return Err(invalid("Codex repair destination already exists"));
    }
    fs::create_dir_all(to.parent().unwrap())?;
    fs::rename(from, to)?;
    for directory in [
        from.parent().unwrap(),
        to.parent().unwrap(),
        to.parent().unwrap().parent().unwrap(),
    ] {
        File::open(directory)?.sync_all()?;
    }
    Ok(())
}

fn copy_ledger(
    ledger: &Path,
    from: &Target,
    to: &Target,
    mut output: impl Write,
) -> io::Result<()> {
    let mut found = false;
    let mut line = Vec::new();
    let mut input = BufReader::new(File::open(ledger)?);
    while input.read_until(b'\n', &mut line)? != 0 {
        if let Ok(mut event) = serde_json::from_slice::<Value>(&line) {
            if event["thread_id"] == from.thread_id {
                if event["event"] == "ThreadCreated" {
                    if found || event["title"] != "Codex" || !(from.owns(&event) || to.owns(&event))
                    {
                        return Err(invalid("Cutover target is not the original Codex thread"));
                    }
                    found = true;
                }
                if from.owns(&event) {
                    event["entity_id"] = json!(to.entity_id);
                    serde_json::to_writer(&mut output, &event)?;
                    output.write_all(b"\n")?;
                    line.clear();
                    continue;
                }
            }
        }
        output.write_all(&line)?;
        line.clear();
    }
    if found {
        Ok(())
    } else {
        Err(invalid("Cutover thread is missing from the ledger"))
    }
}

fn archive_unfinished_turns(path: &Path, thread_id: &str) -> io::Result<()> {
    let mut ledger = crate::ledger::Ledger::open(path).map_err(io::Error::other)?;
    let unfinished: Vec<_> = ledger
        .pending_turns()
        .into_iter()
        .filter(|turn| turn.thread_id == thread_id)
        .map(|turn| turn.id.clone())
        .collect();
    for turn in unfinished {
        ledger
            .interrupt_turn_after_restart(
                &turn,
                "Preserved as legacy Codex history during the company repair.",
            )
            .map_err(io::Error::other)?;
    }
    Ok(())
}

/// One-time offline data correction, before opening the ledger or starting its
/// readers. The intent also lets old locators continue to resolve. Directory
/// renames preserve inbox and journal inodes, so their cursors stay valid.
pub fn repair_legacy_company(
    data: &Path,
    coordination: &Path,
    registry: &EntityRegistry,
) -> io::Result<bool> {
    let result = repair(data, coordination, registry);
    let outcome = match &result {
        Ok(true) => "moved",
        Err(_) => "held",
        Ok(false) => return result,
    };
    let directory = data.join("engine-state/codex-watch");
    drop(fs::create_dir_all(&directory).and_then(|()| {
        save(
            &directory.join("ownership-health.json"),
            &json!({
                "at_ms": crate::util::now_millis(), "outcome": outcome,
            }),
        )
    }));
    result
}

fn repair(data: &Path, coordination: &Path, registry: &EntityRegistry) -> io::Result<bool> {
    let intent = coordination.join(MOVE);
    let old_file = coordination.join(OLD_TARGET);
    let saved: Option<Move> = read_record(&intent)?;
    if saved
        .as_ref()
        .is_some_and(|saved| saved.complete || saved.data_dir != data)
        || (saved.is_none() && !old_file.try_exists()?)
    {
        return Ok(false);
    }
    let _receiver = lock(&coordination.join("receiver.lock"))?;
    let _coordinator = lock(&coordination.join("coordinator.lock"))?;
    let ledger = data.join("conversation-ledger.jsonl");
    let connections = coordination.join("connections");
    let thread_dir =
        |root: &Path, target: &Target| root.join(target.entity_id.as_str()).join(&target.thread_id);
    let new_target = coordination.join(LEGACY_TARGET);
    if !ledger.try_exists()? {
        return Ok(false);
    }
    let mut repair = match read_record::<Move>(&intent)? {
        Some(repair) if repair.complete || repair.data_dir != data => return Ok(false),
        Some(repair) => repair,
        None => {
            let Some(old) = read_record::<Target>(&old_file)? else {
                return Ok(false);
            };
            old.valid()?;
            let mut conventional = false;
            for pair in directories(&thread_dir(&connections, &old))? {
                let Some(record) = read_record::<Value>(&pair.join("connection.json"))? else {
                    continue;
                };
                if record.get("legacy_folder").is_some() {
                    if !old.owns(&record)
                        || record["version"] != 1
                        || record["legacy_live"] != true
                        || !conventional_legacy(coordination, &record)
                    {
                        return Ok(false);
                    }
                    conventional = true;
                }
            }
            if !conventional {
                return Ok(false);
            }
            let Some(company) = legacy_company(registry) else {
                return Ok(false);
            };
            if old.entity_id == company {
                save(&new_target, &old)?;
                fs::remove_file(&old_file)?;
                File::open(coordination)?.sync_all()?;
                return Ok(false);
            }
            let repair = Move {
                version: 1,
                from: old.entity_id,
                to: company,
                thread_id: old.thread_id,
                data_dir: data.to_path_buf(),
                complete: false,
            };
            let to = repair.target(&repair.to);
            if thread_dir(&connections, &to).try_exists()?
                || thread_dir(&data.join("engine-state/messages"), &to).try_exists()?
            {
                return Err(invalid("Codex repair destination already exists"));
            }
            copy_ledger(&ledger, &repair.target(&repair.from), &to, io::sink())?;
            save(&intent, &repair)?;
            repair
        }
    };
    let from = repair.target(&repair.from);
    let to = repair.target(&repair.to);
    from.valid()?;
    if from.entity_id == to.entity_id || !registry.contains(&to.entity_id) {
        return Err(invalid("Codex repair requires its registered destination"));
    }
    let inboxes = data.join("engine-state/messages");
    move_dir(
        &thread_dir(&connections, &from),
        &thread_dir(&connections, &to),
    )?;
    for pair in directories(&thread_dir(&connections, &to))? {
        let path = pair.join("connection.json");
        let Some(mut record) = read_record::<Value>(&path)? else {
            continue;
        };
        if from.owns(&record) {
            record["entity_id"] = json!(to.entity_id);
            record["previous_entity_id"] = json!(from.entity_id);
            save(&path, &record)?;
        }
    }
    for pair in directories(&connections.join(AUTOMATIC).join("codex"))? {
        let path = pair.join("connection.json");
        let Some(mut record) = read_record::<Value>(&path)? else {
            continue;
        };
        if from.owns(&record["binding"]) {
            record["binding"]["entity_id"] = json!(to.entity_id);
            save(&path, &record)?;
        }
    }
    move_dir(&thread_dir(&inboxes, &from), &thread_dir(&inboxes, &to))?;
    // Nightly .115 left some historical inputs without a terminal ledger row.
    // Close those turns before startup recovery can dispatch them to a provider.
    // The existing intake IDs and all original prompt and reply records survive.
    archive_unfinished_turns(&ledger, &from.thread_id)?;
    atomic(&ledger, |output| copy_ledger(&ledger, &from, &to, output))?;
    save(&new_target, &to)?;
    if old_file.try_exists()? {
        fs::remove_file(old_file)?;
    }
    repair.complete = true;
    save(&intent, &repair)?;
    Ok(true)
}

/// A restart can continue with the rest of the profile even if this move has
/// crossed a persistence boundary. Hold its deliveries until a launch finishes it.
pub fn pending_legacy_repair(data: &Path, coordination: &Path) -> Option<(EntityId, String)> {
    let repair = read_record::<Move>(&coordination.join(MOVE)).ok().flatten()?;
    (!repair.complete && repair.data_dir == data).then_some((repair.to, repair.thread_id))
}

pub(super) fn unresolved_legacy(coordination: &Path, endpoint: &Value) -> io::Result<bool> {
    Ok(conventional_legacy(coordination, endpoint)
        && read_record::<Target>(&coordination.join(OLD_TARGET))?
            .is_some_and(|old| old.owns(endpoint)))
}

pub(super) fn historic_locator(
    coordination: &Path,
    direct: &Path,
    target: &Value,
) -> io::Result<PathBuf> {
    let Some(moved) = read_record::<Move>(&coordination.join(MOVE))? else {
        return Ok(direct.into());
    };
    if moved.complete && moved.target(&moved.to).owns(target) {
        let saved: Value =
            read_record(&direct.join("connection.json"))?.ok_or(invalid("Missing Codex pair"))?;
        if saved["previous_entity_id"] == moved.from.as_str()
            && saved["connection_id"] == target["connection_id"]
        {
            return Ok(coordination
                .join("connections")
                .join(moved.from.as_str())
                .join(moved.thread_id)
                .join(direct.file_name().unwrap()));
        }
    }
    Ok(direct.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Entity;
    use crate::ledger::{Ledger, Source, TurnState};
    use crate::session_tools::{self, BridgeReference};
    use crate::status_tools::StatusToolScope;
    use std::os::unix::fs::MetadataExt;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .canonicalize()
                .unwrap()
                .join(format!("codex-ownership-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn folder(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(&path).unwrap();
            path
        }

        fn registry(&self) -> EntityRegistry {
            let product = self.folder("project");
            assert!(git(&product, &["init", "-q"]).is_some());
            assert!(git(
                &product,
                &[
                    "remote",
                    "add",
                    "origin",
                    "git@github.com:RichSpark/RichOS.git"
                ]
            )
            .is_some());
            EntityRegistry::new(vec![
                Entity::try_new("product", "Product", vec![product]).unwrap(),
                Entity::try_new("exporter", "Exporter", vec![self.folder("exporter")]).unwrap(),
            ])
            .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(&self.0));
        }
    }

    #[test]
    fn automatic_requests_resolve_independently_from_their_work_roots() {
        let fixture = Fixture::new();
        let registry = fixture.registry();
        let coordination = fixture.folder("coord");
        let data = fixture.folder("app");
        let mut spine =
            crate::spine::Spine::new(Ledger::open(data.join("conversation-ledger.jsonl")).unwrap());
        spine.set_entity_registry(registry.clone());
        let exporter = EntityId::parse("exporter").unwrap();
        let desk = spine.create_thread("Main", &exporter).unwrap();
        let product = EntityId::parse("product").unwrap();
        assert_eq!(legacy_company(&registry), Some(product.clone()));
        let worktree = fixture.0.join("linked");
        let project = fixture.0.join("project");
        assert!(git(
            &project,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "fixture"
            ]
        )
        .is_some());
        assert!(git(
            &project,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                worktree.to_str().unwrap()
            ]
        )
        .is_some());
        for (id, directory, expected) in [
            (
                "first",
                Some(fixture.folder("project/subdir")),
                Some(product.clone()),
            ),
            (
                "second",
                Some(fixture.folder("exporter/subdir")),
                Some(exporter),
            ),
            ("third", Some(fixture.folder("unregistered")), None),
            ("fourth", None, None),
            ("linked", Some(worktree), Some(product)),
        ] {
            let path = fixture.folder(&format!("coord/connections/_automatic/codex/{id}"));
            save(
                &path.join("connection.json"),
                &json!({
                    "version": 1, "entity_id": AUTOMATIC, "thread_id": "codex",
                    "connection_id": id, "project_dir": directory,
                }),
            )
            .unwrap();
            let request = AutomaticRequest::read(&coordination, &path)
                .unwrap()
                .unwrap();
            assert_eq!(request.company(&registry), expected);
            if let Some(expected) = expected {
                let (binding, _) = spine.codex_conversation(&expected).unwrap();
                request.bind(&binding).unwrap();
                let value: Value = read_record(&path.join("connection.json")).unwrap().unwrap();
                assert_eq!(value["binding"]["entity_id"], expected.as_str());
                assert!(AutomaticRequest::read(&coordination, &path)
                    .unwrap()
                    .is_none());
            }
        }
        assert_eq!(spine.active_thread(), Some(desk.as_str()));
        assert_eq!(spine.ledger().threads().len(), 3);
    }

    #[test]
    fn nightly_115_cutover_rehomes_the_thread_without_replaying_its_two_legacy_pairs() {
        for interrupted in [false, true] {
            let fixture = Fixture::new();
            let registry = fixture.registry();
            let data = fixture.folder("app");
            let coordination = fixture.folder("coord");
            let state = fixture.folder("app/engine-state");
            let accepted = fixture.folder("app/engine-state/codex-watch/accepted");
            let wrong = EntityId::parse("exporter").unwrap();
            let right = EntityId::parse("product").unwrap();
            let path = data.join("conversation-ledger.jsonl");
            let mut ledger = Ledger::open(&path).unwrap();
            let conversation = ledger.create_thread("Codex", &wrong).unwrap();
            let original = ledger.thread_binding(&conversation).unwrap();
            let turn = ledger
                .record_prompt_received(&original, "Saved Codex request", Source::Text)
                .unwrap();
            ledger
                .append_assistant_delta(&turn, "Saved Codex reply", 1)
                .unwrap();
            let unrelated = ledger.create_thread("Codex", &wrong).unwrap();
            let unrelated_binding = ledger.thread_binding(&unrelated).unwrap();
            let unrelated_turn = ledger
                .record_prompt_received(
                    &unrelated_binding,
                    "Current request elsewhere",
                    Source::Text,
                )
                .unwrap();
            drop(ledger);
            let old = Target {
                version: 1,
                entity_id: wrong.clone(),
                thread_id: conversation.clone(),
            };
            save(&coordination.join(OLD_TARGET), &old).unwrap();
            let archived = state.join("codex-watch/pending.jsonl");
            fs::write(&archived, b"\"prior-nightly data\"\n").unwrap();
            let live = state.join("codex-watch/live.jsonl");
            let payload = b"\"live Codex reply\"\n";
            fs::write(&live, payload).unwrap();
            let stat = fs::metadata(&live).unwrap();
            let raw_watch = fixture
                .folder("app/engine-state/codex-watch/watch/app-host")
                .join("live-offsets.json");
            fs::write(&raw_watch, b"{\"saved\":1600}\n").unwrap();
            let scope = StatusToolScope {
                version: 1,
                state_root: state.clone(),
                entity_id: wrong.to_string(),
                thread_id: conversation.clone(),
            };
            let mut first_event = None;
            let mut pairs = Vec::new();
            for (id, folder, imports, boundary) in [
                ("legacy-first", "rich-codex", true, 1600),
                ("legacy-second", "rich-codex-questions", false, 0),
            ] {
                let old_pair =
                    fixture.folder(&format!("coord/connections/{wrong}/{conversation}/{id}"));
                let raw = fixture
                    .folder(&format!("coord/{folder}"))
                    .join("to-rich.md");
                fs::write(&raw, vec![b'x'; boundary]).unwrap();
                let endpoint = json!({
                    "version": 1, "entity_id": wrong, "thread_id": conversation,
                    "connection_id": id, "legacy_folder": raw.parent().unwrap(),
                    "legacy_live": true, "legacy_boundary": boundary,
                    "imports_legacy_spool": imports, "preserves_legacy_outbox": imports,
                });
                save(&old_pair.join("connection.json"), &endpoint).unwrap();
                let record = json!({"version": 1, "message_id": id, "kind": "legacy_live", "text": "live Codex reply"});
                let event = json!({
                    "locator": old_pair, "connection": endpoint, "record": record,
                    "source_identity": format!("{}:{}", stat.dev(), stat.ino()), "through": payload.len(),
                });
                session_tools::observe_channel(&scope,
                    "Legacy Markdown observation; verify the complete original request before acting.\nlive Codex reply",
                    BridgeReference {
                        connection_id: id.into(), message_id: id.into(),
                        locator: old_pair.to_string_lossy().into_owned(),
                        payload_digest: super::super::digest(&record), reply_to: None,
                    }).unwrap();
                if imports {
                    first_event = Some(event);
                }
                pairs.push((id, endpoint));
            }
            let first_receipt = accepted.join("legacy-first.live.json");
            let mut event = first_event.unwrap();
            save(
                &first_receipt,
                &json!({"through": payload.len(), "source_identity": event["source_identity"]}),
            )
            .unwrap();
            let receipt_bytes = fs::read(&first_receipt).unwrap();
            let inbox = session_tools::inbox(&scope, "stu").unwrap();
            let cursor = inbox.with_extension("cursor");
            fs::write(&cursor, b"{\"through\":200}\n").unwrap();
            let original_inbox = fs::read(&inbox).unwrap();
            let inbox_inode = fs::metadata(&inbox).unwrap().ino();
            let old_connections = coordination
                .join("connections")
                .join(wrong.as_str())
                .join(&conversation);
            let new_connections = coordination
                .join("connections")
                .join(right.as_str())
                .join(&conversation);
            if interrupted {
                save(
                    &coordination.join(MOVE),
                    &Move {
                        version: 1,
                        from: wrong.clone(),
                        to: right.clone(),
                        thread_id: conversation.clone(),
                        data_dir: data.clone(),
                        complete: false,
                    },
                )
                .unwrap();
                move_dir(&old_connections, &new_connections).unwrap();
                let busy = lock(&coordination.join("coordinator.lock")).unwrap();
                assert_eq!(
                    repair_legacy_company(&data, &coordination, &registry)
                        .unwrap_err()
                        .kind(),
                    io::ErrorKind::WouldBlock
                );
                assert_eq!(
                    pending_legacy_repair(&data, &coordination),
                    Some((right.clone(), conversation.clone()))
                );
                let still_readable = Ledger::open(&path).unwrap();
                assert_eq!(
                    still_readable.thread_binding(&conversation).unwrap(),
                    original
                );
                assert_eq!(
                    still_readable.messages_scoped(&original).unwrap()[0].text,
                    "Saved Codex request"
                );
                drop(still_readable);
                drop(busy);
            }
            assert!(repair_legacy_company(&data, &coordination, &registry).unwrap());
            assert!(!repair_legacy_company(&data, &coordination, &registry).unwrap());
            let health: Value = read_record(&state.join("codex-watch/ownership-health.json"))
                .unwrap()
                .unwrap();
            assert_eq!(health["outcome"], "moved");
            assert!(!coordination.join(OLD_TARGET).exists());
            assert!(!old_connections.exists());
            assert!(!inbox.exists());
            assert_eq!(fs::read(&first_receipt).unwrap(), receipt_bytes);
            assert_eq!(fs::read(&archived).unwrap(), b"\"prior-nightly data\"\n");
            assert_eq!(fs::metadata(&live).unwrap().ino(), stat.ino());
            assert_eq!(fs::read(&raw_watch).unwrap(), b"{\"saved\":1600}\n");
            for (id, mut expected) in pairs {
                expected["entity_id"] = json!(right);
                expected["previous_entity_id"] = json!(wrong);
                assert_eq!(
                    read_record::<Value>(&new_connections.join(id).join("connection.json"))
                        .unwrap(),
                    Some(expected)
                );
            }
            let ledger = Ledger::open(&path).unwrap();
            let binding = ledger.thread_binding(&conversation).unwrap();
            assert_eq!(binding.entity_id(), &right);
            assert_eq!(binding.binding_revision(), original.binding_revision());
            assert_eq!(
                ledger.thread_binding(&unrelated).unwrap().entity_id(),
                &wrong
            );
            assert!(ledger.messages_scoped(&original).is_err());
            assert_eq!(ledger.turn(&turn).unwrap().state, TurnState::Interrupted);
            assert_eq!(
                ledger
                    .pending_turns()
                    .iter()
                    .map(|turn| turn.id.as_str())
                    .collect::<Vec<_>>(),
                [unrelated_turn.as_str()]
            );
            let messages = ledger.messages_scoped(&binding).unwrap();
            assert_eq!(
                messages
                    .iter()
                    .map(|message| message.text.as_str())
                    .collect::<Vec<_>>(),
                ["Saved Codex request", "Saved Codex reply"]
            );
            let scope = StatusToolScope {
                entity_id: right.to_string(),
                ..scope
            };
            let inbox = session_tools::inbox(&scope, "stu").unwrap();
            assert_eq!(fs::metadata(&inbox).unwrap().ino(), inbox_inode);
            assert_eq!(
                fs::read(inbox.with_extension("cursor")).unwrap(),
                b"{\"through\":200}\n"
            );
            event["locator"] = json!(new_connections.join("legacy-first"));
            event["connection"]["entity_id"] = json!(right);
            super::super::accept(&state, &binding, &event, &coordination).unwrap();
            assert_eq!(
                fs::read(inbox).unwrap(),
                original_inbox,
                "retry matched the original receipt"
            );
        }
    }
}
