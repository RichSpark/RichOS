//! Process inventory and physical footprint, in-process via Darwin's libproc.
use super::{Kind, KINDS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::mem::{size_of, MaybeUninit};
use std::sync::OnceLock;

const PROC_UID_ONLY: u32 = 4; // sys/proc_info.h
const RICH_SCRIBE_ARG: &[u8] = b"--richos-dictation";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
struct Identity {
    pid: i32,
    start_sec: u64,
    start_usec: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    Suite(Kind),
    Provider(Kind),
    WebKit,
    Other,
}

#[derive(Clone, Copy)]
struct Process {
    identity: Identity,
    parent: i32,
    role: Role,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Reading {
    pub bytes: Option<i64>,
    pub count: i64,
    pub identity: i64,
}

pub(super) struct Capture {
    pub process_count: usize,
    pub unreadable: usize,
    pub readings: [Reading; KINDS.len()],
}

/// A local declaration from RichScribe's WebKit client. macOS charges both the
/// client and its XPC services to RichOS, so its responsible PID is ambiguous.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct ScribeWebKit {
    client: Identity,
    services: Vec<Identity>,
}

impl ScribeWebKit {
    pub fn current(pids: &[i32]) -> io::Result<Self> {
        let uid = unsafe { libc::geteuid() };
        let client = process_info(unsafe { libc::getpid() }, uid)
            .ok_or_else(|| io::Error::other("RichScribe process identity unavailable"))?
            .0;
        let mut services: Vec<_> = pids
            .iter()
            .take(12)
            .filter_map(|&pid| process_info(pid, uid).map(|row| row.0))
            .collect();
        services.sort_unstable();
        services.dedup();
        Ok(Self { client, services })
    }
}

fn pids(uid: libc::uid_t) -> io::Result<Vec<i32>> {
    // libproc returns the required byte length for a null buffer and the bytes written otherwise.
    let bytes = unsafe { libc::proc_listpids(PROC_UID_ONLY, uid, std::ptr::null_mut(), 0) };
    if bytes <= 0 {
        return Err(io::Error::last_os_error());
    }
    let mut capacity = bytes as usize / size_of::<i32>() + 64;
    for _ in 0..3 {
        let mut pids = vec![0_i32; capacity];
        let bytes = unsafe {
            libc::proc_listpids(
                PROC_UID_ONLY,
                uid,
                pids.as_mut_ptr().cast(),
                (pids.len() * size_of::<i32>()) as i32,
            )
        };
        if bytes <= 0 {
            return Err(io::Error::last_os_error());
        }
        let count = bytes as usize / size_of::<i32>();
        if count < capacity {
            pids.truncate(count);
            pids.retain(|pid| *pid > 0);
            return Ok(pids);
        }
        capacity *= 2;
    }
    Err(io::Error::other("process list grew while sampling"))
}

fn process_info(pid: i32, uid: libc::uid_t) -> Option<(Identity, i32)> {
    let mut info = MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let size = size_of::<libc::proc_bsdinfo>() as i32;
    // A full return initializes the BSD info struct. Short returns include exited processes.
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if read != size {
        return None;
    }
    let info = unsafe { info.assume_init() };
    if info.pbi_uid != uid || info.pbi_pid != pid as u32 || info.pbi_status == libc::SZOMB {
        return None;
    }
    Some((
        Identity {
            pid,
            start_sec: info.pbi_start_tvsec,
            start_usec: info.pbi_start_tvusec,
        },
        info.pbi_ppid as i32,
    ))
}

fn executable(pid: i32, buffer: &mut [u8]) -> Option<&[u8]> {
    let length =
        unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    (length > 0).then(|| {
        buffer[..length as usize]
            .split(|&byte| byte == 0)
            .next()
            .unwrap_or_default()
    })
}

fn arg_buffer() -> Vec<u8> {
    let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
    let mut length = 0_i32;
    let mut size = size_of::<i32>();
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            2,
            (&mut length as *mut i32).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc == 0 && (4096..=4 * 1024 * 1024).contains(&length) {
        vec![0; length as usize]
    } else {
        Vec::new()
    }
}

/// KERN_PROCARGS2 includes argc and the executable path before argv. Only inspect argv[1]
/// for the few shared executables whose first argument selects a program. Never store argv.
fn first_arg(pid: i32, buffer: &mut [u8]) -> Option<&[u8]> {
    if buffer.is_empty() {
        return None;
    }
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut size = buffer.len();
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0).then(|| arg_one(&buffer[..size])).flatten()
}

fn arg_one(buffer: &[u8]) -> Option<&[u8]> {
    let argc = i32::from_ne_bytes(buffer.get(..4)?.try_into().ok()?);
    if argc < 2 {
        return Some(&[]);
    }
    let end_executable = buffer[4..].iter().position(|&byte| byte == 0)? + 4;
    let argv = buffer[end_executable..]
        .iter()
        .position(|&byte| byte != 0)?
        + end_executable;
    buffer[argv..].split(|&byte| byte == 0).nth(1)
}

fn contains(path: &[u8], part: &[u8]) -> bool {
    path.windows(part.len()).any(|window| window == part)
}

fn role_for<'a>(exe: &[u8], argument: impl FnOnce() -> Option<&'a [u8]>) -> Role {
    let name = exe.rsplit(|&byte| byte == b'/').next().unwrap_or_default();
    match name {
        b"richos-tauri" => match argument() {
            Some(RICH_SCRIBE_ARG) => Role::Suite(Kind::RichScribe),
            // The main app starts without arguments or with Launch Services' process serial.
            // Any other mode is a helper, including tool-call hooks and updater processes.
            Some(arg) if arg.is_empty() || arg.starts_with(b"-psn_") => Role::Suite(Kind::RichOs),
            _ => Role::Other,
        },
        b"RichFiles" => Role::Suite(Kind::RichFiles),
        b"RichGlance" => Role::Suite(Kind::RichGlance),
        b"RichGlanceCaptureHelper" => Role::Suite(Kind::Capture),
        b"RichFilesWindowGestures" => Role::Suite(Kind::Gestures),
        b"RichDoctor" => match argument() {
            Some(arg) if arg != b"--support-adapter" => Role::Suite(Kind::RichDoctor),
            _ => Role::Other,
        },
        b"com.apple.WebKit.WebContent"
        | b"com.apple.WebKit.Networking"
        | b"com.apple.WebKit.GPU"
            if exe.starts_with(b"/System/")
                && contains(exe, b"/System/Library/Frameworks/WebKit.framework/") =>
        {
            Role::WebKit
        }
        b"claude" => Role::Provider(Kind::ClaudeCode),
        b"codex" | b"codex-aarch64-apple-darwin" | b"codex-x86_64-apple-darwin" => {
            Role::Provider(Kind::Codex)
        }
        b"node" => match argument() {
            Some(arg)
                if arg.ends_with(b"/claude") || contains(arg, b"/@anthropic-ai/claude-code/") =>
            {
                Role::Provider(Kind::ClaudeCode)
            }
            Some(arg) if arg.ends_with(b"/codex") || contains(arg, b"/@openai/codex/") => {
                Role::Provider(Kind::Codex)
            }
            _ => Role::Other,
        },
        _ if contains(exe, b"/.local/share/claude/versions/") => Role::Provider(Kind::ClaudeCode),
        _ => Role::Other,
    }
}

/// WebKit's XPC services have launchd as their parent. macOS separately tracks
/// the application responsible for each service. Resolve the optional system
/// symbol at runtime so the host processes remain measurable if it is absent.
fn responsible_pid(pid: i32) -> Option<i32> {
    type Resolve = unsafe extern "C" fn(libc::pid_t) -> libc::pid_t;
    static RESOLVE: OnceLock<Option<Resolve>> = OnceLock::new();
    let resolve = (*RESOLVE.get_or_init(|| {
        let address = unsafe {
            libc::dlsym(
                libc::RTLD_DEFAULT,
                c"responsibility_get_pid_responsible_for_pid".as_ptr(),
            )
        };
        // libSystem's responsibility API takes one pid_t and returns a pid_t.
        (!address.is_null())
            .then(|| unsafe { std::mem::transmute::<*mut std::ffi::c_void, Resolve>(address) })
    }))?;
    let owner = unsafe { resolve(pid) };
    (owner > 1 && owner != pid).then_some(owner)
}

fn parent_of<'a>(process: &Process, inventory: &'a BTreeMap<i32, Process>) -> Option<&'a Process> {
    let parent = inventory.get(&process.parent)?;
    let age = |p: &Process| (p.identity.start_sec, p.identity.start_usec);
    // A newer process with the same PID cannot be the recorded parent.
    (age(parent) <= age(process) && parent.identity.pid != process.identity.pid).then_some(parent)
}

fn kind_for(process: &Process, inventory: &BTreeMap<i32, Process>) -> Option<Kind> {
    match process.role {
        Role::Suite(kind) => Some(kind),
        Role::WebKit => match parent_of(process, inventory)?.role {
            Role::Suite(kind) => Some(kind),
            _ => None,
        },
        Role::Provider(kind) => {
            let mut child = process;
            for _ in 0..inventory.len() {
                child = parent_of(child, inventory)?;
                if child.role == Role::Suite(Kind::RichOs) {
                    return Some(kind);
                }
            }
            None
        }
        Role::Other => None,
    }
}

/// A matching generation from the WebKit client overrides the coarse OS owner.
/// Never move an unrelated service or one whose PID has been reused.
fn attach_scribe_webkit(inventory: &mut BTreeMap<i32, Process>, snapshot: &ScribeWebKit) -> bool {
    let Some(client) = inventory.get(&snapshot.client.pid) else {
        return false;
    };
    if client.role != Role::Suite(Kind::RichScribe) || client.identity != snapshot.client {
        return false;
    }
    let client = *client;
    let mut complete = !snapshot.services.is_empty();
    for identity in &snapshot.services {
        match inventory.get_mut(&identity.pid) {
            Some(service)
                if service.identity == *identity
                    && service.role == Role::WebKit
                    && (service.parent == client.parent
                        || service.parent == client.identity.pid) =>
            {
                service.parent = client.identity.pid;
            }
            _ => complete = false,
        }
    }
    complete
}

fn unassigned_webkit(inventory: &BTreeMap<i32, Process>) -> usize {
    let owners: Vec<_> = inventory
        .values()
        .filter_map(|p| {
            if p.role != Role::Suite(Kind::RichScribe) {
                return None;
            }
            let parent = parent_of(p, inventory)?;
            (parent.role == Role::Suite(Kind::RichOs)).then_some(parent.identity.pid)
        })
        .collect();
    inventory
        .values()
        .filter(|p| p.role == Role::WebKit && owners.contains(&p.parent))
        .count()
}

fn footprint(identity: Identity, uid: libc::uid_t) -> Option<i64> {
    let mut usage = MaybeUninit::<libc::rusage_info_v2>::uninit();
    // RUSAGE_INFO_V2 fills exactly rusage_info_v2 on success. The trailing BSD check rejects
    // an exit or PID reuse between enumeration and reading the footprint.
    let rc = unsafe {
        libc::proc_pid_rusage(
            identity.pid,
            libc::RUSAGE_INFO_V2,
            usage.as_mut_ptr().cast(),
        )
    };
    if rc != 0 || process_info(identity.pid, uid)?.0 != identity {
        return None;
    }
    i64::try_from(unsafe { usage.assume_init() }.ri_phys_footprint).ok()
}

/// One process's physical footprint by pid, this user's only, read the way a capture reads it.
pub(super) fn footprint_of(pid: i32) -> Option<i64> {
    let uid = unsafe { libc::geteuid() };
    let (identity, _) = process_info(pid, uid)?;
    footprint(identity, uid).filter(|bytes| *bytes > 0)
}

fn aggregate(
    inventory: &BTreeMap<i32, Process>,
    mut footprint: impl FnMut(Identity) -> Option<i64>,
) -> Capture {
    let mut readings = [Reading {
        bytes: Some(0),
        count: 0,
        identity: 0,
    }; KINDS.len()];
    // Stable order, so only an actual change in membership starts a fresh group lifetime.
    let mut identities: [DefaultHasher; KINDS.len()] =
        std::array::from_fn(|_| DefaultHasher::new());
    let mut unreadable = 0;
    for process in inventory.values() {
        let Some(kind) = kind_for(process, inventory) else {
            continue;
        };
        let index = kind as usize;
        let reading = &mut readings[index];
        let bytes = footprint(process.identity).filter(|bytes| *bytes > 0);
        if bytes.is_none() {
            unreadable += 1;
        }
        reading.bytes = reading
            .bytes
            .zip(bytes)
            .and_then(|(sum, value)| sum.checked_add(value));
        reading.count += 1;
        process.identity.hash(&mut identities[index]);
    }
    for (reading, identity) in readings.iter_mut().zip(identities) {
        reading.identity = identity.finish() as i64;
    }
    Capture {
        process_count: inventory.len(),
        unreadable,
        readings,
    }
}

pub(super) fn capture(scribe: Option<&ScribeWebKit>) -> io::Result<Capture> {
    let uid = unsafe { libc::geteuid() };
    let pids = pids(uid)?;
    let mut path = [0; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let mut args = arg_buffer();
    let mut inventory = pids
        .into_iter()
        .filter_map(|pid| {
            let (identity, parent) = process_info(pid, uid)?;
            let role = executable(pid, &mut path)
                .map(|exe| role_for(exe, || first_arg(pid, &mut args)))
                .unwrap_or(Role::Other);
            let parent = if role == Role::WebKit {
                responsible_pid(pid).unwrap_or(parent)
            } else {
                parent
            };
            Some((
                pid,
                Process {
                    identity,
                    parent,
                    role,
                },
            ))
        })
        .collect();
    let attributed = scribe.is_some_and(|scribe| attach_scribe_webkit(&mut inventory, scribe));
    let mut sample = aggregate(&inventory, |identity| footprint(identity, uid));
    if !attributed {
        let ambiguous = unassigned_webkit(&inventory);
        if ambiguous > 0 {
            sample.unreadable += ambiguous;
            sample.readings[Kind::RichOs as usize].bytes = None;
            sample.readings[Kind::RichScribe as usize].bytes = None;
        }
    }
    Ok(sample)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: i32, parent: i32, role: Role) -> Process {
        Process {
            identity: Identity {
                pid,
                start_sec: pid as u64,
                start_usec: 0,
            },
            parent,
            role,
        }
    }

    #[test]
    fn process_roles_distinguish_shared_executables_and_provider_installations() {
        assert_eq!(
            role_for(
                b"/Applications/RichOS.app/Contents/MacOS/richos-tauri",
                || Some(b"")
            ),
            Role::Suite(Kind::RichOs)
        );
        assert_eq!(
            role_for(b"richos-tauri", || Some(RICH_SCRIBE_ARG)),
            Role::Suite(Kind::RichScribe)
        );
        assert_eq!(
            role_for(b"richos-tauri", || Some(b"--status-mcp")),
            Role::Other
        );
        assert_eq!(
            role_for(b"RichDoctor", || Some(b"--support-adapter")),
            Role::Other
        );
        assert_eq!(
            role_for(b"RichDoctor", || Some(b"--quiet")),
            Role::Suite(Kind::RichDoctor)
        );
        assert_eq!(
            role_for(b"RichFilesWindowGestures", || None),
            Role::Suite(Kind::Gestures)
        );
        assert_eq!(
            role_for(b"RichGlanceCaptureHelper", || None),
            Role::Suite(Kind::Capture)
        );
        assert_eq!(
            role_for(b"/Users/test/.local/share/claude/versions/2.0.0", || None),
            Role::Provider(Kind::ClaudeCode)
        );
        assert_eq!(
            role_for(b"node", || Some(b"/opt/homebrew/bin/claude")),
            Role::Provider(Kind::ClaudeCode)
        );
        assert_eq!(
            role_for(b"codex-aarch64-apple-darwin", || None),
            Role::Provider(Kind::Codex)
        );

        let mut args = 3_i32.to_ne_bytes().to_vec();
        args.extend_from_slice(
            b"/bin/richos-tauri\0\0\0richos-tauri\0--richos-dictation\0--parent\0TOKEN=secret\0",
        );
        assert_eq!(arg_one(&args), Some(RICH_SCRIBE_ARG));
        assert_eq!(arg_one(&args[..10]), None);
    }

    #[test]
    fn sampling_groups_only_richos_owned_provider_trees() {
        let mut inventory: BTreeMap<_, _> = [
            process(10, 1, Role::Suite(Kind::RichOs)),
            process(11, 10, Role::Suite(Kind::RichScribe)),
            process(12, 10, Role::Other),
            process(13, 12, Role::Provider(Kind::ClaudeCode)),
            process(14, 13, Role::Other),
            process(15, 13, Role::Provider(Kind::Codex)),
            process(16, 15, Role::Other),
            process(17, 1, Role::Provider(Kind::ClaudeCode)),
            process(18, 17, Role::Other),
            process(19, 1, Role::Provider(Kind::Codex)),
            process(20, 22, Role::Provider(Kind::Codex)), // newer parent has a recycled PID
            process(22, 10, Role::Other),
            process(23, 1, Role::Suite(Kind::RichDoctor)),
            process(24, 23, Role::Provider(Kind::ClaudeCode)),
        ]
        .into_iter()
        .map(|process| (process.identity.pid, process))
        .collect();
        let mut sampled = Vec::new();
        let capture = aggregate(&inventory, |identity| {
            sampled.push(identity.pid);
            Some(i64::from(identity.pid))
        });
        assert_eq!(sampled, vec![10, 11, 13, 15, 23]);
        assert_eq!(capture.readings[Kind::ClaudeCode as usize].bytes, Some(13));
        assert_eq!(capture.readings[Kind::ClaudeCode as usize].count, 1);
        assert_eq!(capture.readings[Kind::Codex as usize].bytes, Some(15));
        assert_eq!(capture.readings[Kind::RichScribe as usize].bytes, Some(11));

        // A new generation with the same PID breaks the group continuity.
        inventory.get_mut(&13).unwrap().identity.start_usec += 1;
        let next = aggregate(&inventory, |identity| (identity.pid != 15).then_some(1));
        assert_ne!(
            capture.readings[Kind::ClaudeCode as usize].identity,
            next.readings[Kind::ClaudeCode as usize].identity
        );
        assert_eq!(next.readings[Kind::Codex as usize].bytes, None);
        assert_eq!(next.unreadable, 1);
    }

    #[test]
    fn short_lived_richos_modes_do_not_break_the_gui_lifetime() {
        let gui = process(10, 1, role_for(b"richos-tauri", || Some(b"")));
        let mut inventory = BTreeMap::from([(10, gui)]);
        let idle = aggregate(&inventory, |_| Some(100)).readings[Kind::RichOs as usize];
        for (pid, arg) in [
            (11, b"--claude-quota-gate".as_slice()),
            (12, b"--git-name-wait".as_slice()),
            (13, b"--richos-connect-guard".as_slice()),
            (14, b"--richos-quota-v1".as_slice()),
            (15, b"--status-mcp".as_slice()),
        ] {
            inventory.insert(
                pid,
                process(pid, 10, role_for(b"richos-tauri", || Some(arg))),
            );
        }
        let busy = aggregate(&inventory, |_| Some(100)).readings[Kind::RichOs as usize];
        assert_eq!(
            (busy.bytes, busy.count, busy.identity),
            (idle.bytes, 1, idle.identity)
        );
    }

    #[test]
    fn only_suite_owned_system_webkit_services_count() {
        const CONTENT: &[u8] = b"/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent";
        const GPU: &[u8] = b"/System/Volumes/Preboot/Cryptexes/OS/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.GPU.xpc/Contents/MacOS/com.apple.WebKit.GPU";
        // The second argument is the effective parent. For these XPC services macOS
        // identifies the owning app separately from launchd, their actual parent.
        let processes = [
            process(10, 1, Role::Suite(Kind::RichOs)),
            process(11, 1, Role::Suite(Kind::RichDoctor)),
            process(12, 1, Role::Other),
            process(13, 10, role_for(CONTENT, || None)),
            process(14, 10, role_for(GPU, || None)),
            process(15, 11, role_for(CONTENT, || None)),
            process(16, 12, role_for(CONTENT, || None)),
            process(
                17,
                10,
                role_for(b"/Users/test/com.apple.WebKit.WebContent", || None),
            ),
            process(18, 20, role_for(CONTENT, || None)), // owner PID now belongs to a newer process
            process(20, 1, Role::Suite(Kind::RichScribe)),
        ];
        let inventory = processes.into_iter().map(|p| (p.identity.pid, p)).collect();
        let capture = aggregate(&inventory, |id| Some(i64::from(id.pid)));
        for (kind, bytes, count) in [
            (Kind::RichOs, 37, 3),
            (Kind::RichDoctor, 26, 2),
            (Kind::RichScribe, 20, 1),
        ] {
            let reading = capture.readings[kind as usize];
            assert_eq!(
                (reading.bytes, reading.count),
                (Some(bytes), count),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn agent_commands_do_not_inflate_the_provider_or_reset_its_lifetime() {
        let mut inventory = BTreeMap::from([
            (10, process(10, 1, Role::Suite(Kind::RichOs))),
            (11, process(11, 10, Role::Other)), // supervisor
            (12, process(12, 11, Role::Provider(Kind::ClaudeCode))),
            (13, process(13, 11, Role::Provider(Kind::Codex))),
        ]);
        let idle = aggregate(&inventory, |_| Some(100));
        inventory.extend([
            (14, process(14, 12, Role::Other)), // Bash performing a tool call
            (15, process(15, 14, Role::Other)), // cargo
            (16, process(16, 13, Role::Other)), // test runner
            (17, process(17, 1, Role::Provider(Kind::Codex))), // unrelated instance
        ]);
        let busy = aggregate(&inventory, |_| Some(100));
        for kind in [Kind::ClaudeCode, Kind::Codex] {
            let a = idle.readings[kind as usize];
            let b = busy.readings[kind as usize];
            assert_eq!(
                (b.bytes, b.count, b.identity),
                (a.bytes, 1, a.identity),
                "{kind:?}"
            );
        }
    }
    #[test]
    fn richscribe_can_disambiguate_webkit_charged_to_its_richos_parent() {
        let gui = process(10, 1, Role::Suite(Kind::RichOs));
        let client = process(11, 10, Role::Suite(Kind::RichScribe));
        let gui_web = process(12, 10, Role::WebKit);
        let scribe_web = process(13, 10, Role::WebKit);
        let stale = process(14, 10, Role::WebKit);
        let foreign = process(15, 1, Role::WebKit);
        let mut inventory = [gui, client, gui_web, scribe_web, stale, foreign]
            .into_iter()
            .map(|p| (p.identity.pid, p))
            .collect();
        assert_eq!(unassigned_webkit(&inventory), 3);
        let before = aggregate(&inventory, |_| Some(100));
        assert_eq!(before.readings[Kind::RichScribe as usize].count, 1);
        let previous_generation = Identity {
            start_usec: 1,
            ..stale.identity
        };
        attach_scribe_webkit(
            &mut inventory,
            &ScribeWebKit {
                client: client.identity,
                services: vec![scribe_web.identity, previous_generation, foreign.identity],
            },
        );
        let capture = aggregate(&inventory, |_| Some(100));
        assert_eq!(capture.readings[Kind::RichScribe as usize].count, 2);
        assert_eq!(capture.readings[Kind::RichOs as usize].count, 3);
        assert_eq!(inventory[&12].parent, 10);
        assert_eq!(inventory[&14].parent, 10);
        assert_eq!(inventory[&15].parent, 1);
    }
}
