mod archive;
mod native;
mod stream;

use std::error::Error;
use std::ffi::OsStr;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use native::{Destination, DestinationIdentity};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn request_stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

fn check_cancelled() -> Result<()> {
    if STOP.load(Ordering::Relaxed) {
        Err(io::Error::new(io::ErrorKind::Interrupted, "Archive operation cancelled.").into())
    } else {
        Ok(())
    }
}

fn invalid(message: &str) -> Box<dyn Error> {
    io::Error::new(io::ErrorKind::InvalidData, message).into()
}

fn name(value: &OsStr) -> Result<&str> {
    value
        .to_str()
        .ok_or_else(|| invalid("The name cannot be represented as text."))
}

fn published(path: PathBuf) -> Result<Value> {
    Ok(json!({"path": name(path.as_os_str())?}))
}

fn is_zip(source: &Path) -> bool {
    source
        .extension()
        .is_some_and(|value| value.eq_ignore_ascii_case("zip"))
}

fn extract(
    source: &Path,
    destination: &Destination,
    folder: &str,
    selected: &[usize],
) -> Result<PathBuf> {
    if is_zip(source) {
        archive::extract(source, destination, folder, selected)
    } else {
        stream::extract(source, destination, folder, selected)
    }
}

fn command(arguments: &[&OsStr]) -> Result<Value> {
    let (arguments, expected) = match arguments {
        [flag, identity, rest @ ..] if *flag == "--destination-identity" => {
            (rest, Some(DestinationIdentity::parse(name(identity)?)?))
        }
        _ => (arguments, None),
    };
    match arguments {
        [mode, source] if *mode == "list" && expected.is_none() => {
            let source = Path::new(source);
            let entries = if is_zip(source) {
                archive::list(source)?
            } else {
                stream::list(source)?
            };
            Ok(Value::Array(
                entries
                    .into_iter()
                    .map(|entry| {
                        json!({
                            "index": entry.index,
                            "path": entry.path,
                            "isDirectory": entry.is_directory,
                            "size": entry.size
                        })
                    })
                    .collect(),
            ))
        }
        [mode, destination, archive_name, separator, sources @ ..]
            if *mode == "create" && *separator == "--" && !sources.is_empty() =>
        {
            let sources: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
            published(archive::create(
                &Destination::open(Path::new(destination), expected)?,
                name(archive_name)?,
                &sources,
            )?)
        }
        [mode, source, destination, folder] if *mode == "extract" => published(extract(
            Path::new(source),
            &Destination::open(Path::new(destination), expected)?,
            name(folder)?,
            &[],
        )?),
        [mode, source, destination, folder, separator, selected @ ..]
            if *mode == "extract" && *separator == "--" && !selected.is_empty() =>
        {
            let indices = selected
                .iter()
                .map(|index| {
                    name(index)?
                        .parse::<usize>()
                        .map_err(|_| invalid("A selected archive index is invalid."))
                })
                .collect::<Result<Vec<_>>>()?;
            published(extract(
                Path::new(source),
                &Destination::open(Path::new(destination), expected)?,
                name(folder)?,
                &indices,
            )?)
        }
        _ => Err(invalid(
            "Usage: richfiles-archive list ARCHIVE | create DEST NAME.zip -- SOURCE... | extract ARCHIVE DEST FOLDER [-- INDEX...]",
        )),
    }
}

fn run() -> Result<()> {
    let values = std::env::args_os().skip(1).collect::<Vec<_>>();
    let arguments = values.iter().map(|arg| arg.as_os_str()).collect::<Vec<_>>();
    let value = command(&arguments)?;
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, &value)?;
    output.write_all(b"\n")?;
    Ok(())
}

fn main() -> ExitCode {
    // A cancellation signal only sets an atomic flag. The next bounded I/O
    // step exits normally so owned staging folders can clean up.
    let handler = request_stop as *const () as libc::sighandler_t;
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests;
