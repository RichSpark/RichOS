//! Process inventory and physical footprint, in-process via Darwin's libproc.
use super::{Kind, KINDS};
use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::mem::{size_of, MaybeUninit};

const PROC_UID_ONLY: u32 = 4; // sys/proc_info.h
const RICH_SCRIBE_ARG: &[u8] = b"--richos-dictation";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct Identity {
    pid: i32,
    start_sec: u64,
    start_usec: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    Suite(Kind),
    Provider(Kind),
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
            // The status server and other stdio modes share this executable.
            Some(arg) if !arg.ends_with(b"-mcp") && arg != b"--system-resources" => {
                Role::Suite(Kind::RichOs)
            }
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

fn kind_for(process: &Process, inventory: &BTreeMap<i32, Process>) -> Option<Kind> {
    if let Role::Suite(kind) = process.role {
        return Some(kind);
    }
    // The nearest provider owns a descendant, but a RichOS ancestor is required. Check that
    // every edge points to a process that could have existed when its child started.
    let mut provider = None;
    let mut child = process;
    for _ in 0..inventory.len() {
        match child.role {
            Role::Suite(Kind::RichOs) => return provider,
            Role::Provider(kind) => {
                provider.get_or_insert(kind);
            }
            _ => {}
        }
        let parent = inventory.get(&child.parent)?;
        let age = |p: &Process| (p.identity.start_sec, p.identity.start_usec);
        if age(parent) > age(child) || parent.identity.pid == child.identity.pid {
            return None;
        }
        child = parent;
    }
    None
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

pub(super) fn capture() -> io::Result<Capture> {
    let uid = unsafe { libc::geteuid() };
    let pids = pids(uid)?;
    let mut path = [0; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    let mut args = arg_buffer();
    let inventory = pids
        .into_iter()
        .filter_map(|pid| {
            let (identity, parent) = process_info(pid, uid)?;
            let role = executable(pid, &mut path)
                .map(|exe| role_for(exe, || first_arg(pid, &mut args)))
                .unwrap_or(Role::Other);
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
    Ok(aggregate(&inventory, |identity| footprint(identity, uid)))
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
        assert_eq!(sampled, vec![10, 11, 13, 14, 15, 16, 23]);
        assert_eq!(capture.readings[Kind::ClaudeCode as usize].bytes, Some(27));
        assert_eq!(capture.readings[Kind::ClaudeCode as usize].count, 2);
        assert_eq!(capture.readings[Kind::Codex as usize].bytes, Some(31));
        assert_eq!(capture.readings[Kind::RichScribe as usize].bytes, Some(11));

        // A new generation with the same PID breaks the group continuity.
        inventory.get_mut(&14).unwrap().identity.start_usec += 1;
        let next = aggregate(&inventory, |identity| (identity.pid != 16).then_some(1));
        assert_ne!(
            capture.readings[Kind::ClaudeCode as usize].identity,
            next.readings[Kind::ClaudeCode as usize].identity
        );
        assert_eq!(next.readings[Kind::Codex as usize].bytes, None);
        assert_eq!(next.unreadable, 1);
    }
}
