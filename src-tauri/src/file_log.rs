//! The file the host's own log is written to.
//!
//! A failure a person is told about must be diagnosable afterwards, which
//! means the log has to exist somewhere they can reach: standard output does
//! not, because a release build on Windows is a `windows_subsystem = "windows"`
//! binary with no console to hold it. Every message therefore goes to standard
//! output, as it always has, and to this file once the host can resolve its
//! directory.
//!
//! Nothing here may stop the host. A log that cannot be created is simply not
//! there, and the run continues.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;

/// The log's name inside the application's log directory.
const FILE_NAME: &str = "quota.log";

/// The name the one kept earlier copy is under.
const BACKUP_NAME: &str = "quota.log.1";

/// The size at which the log is rolled over, once, before a run starts.
const MAX_BYTES: u64 = 2 * 1024 * 1024;

/// The open log file, attached once the host can resolve its directory.
///
/// Until then nothing writes: standard output carries the log, and a run that
/// never reaches setup has no file to write to.
static OPEN: Mutex<Option<File>> = Mutex::new(None);

/// The location and availability of the host's log.
///
/// This is the path a failure message names, so it is derived from the same
/// directory the file itself is opened in.
#[must_use]
pub(crate) fn location<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> impl fmt::Display {
    LogLocation(file_path(app).ok())
}

struct LogLocation(Option<PathBuf>);

impl fmt::Display for LogLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(path) = &self.0 else {
            return formatter
                .write_str("unavailable (the application log directory could not be resolved)");
        };
        if OPEN.lock().is_ok_and(|open| open.is_some()) {
            write!(formatter, "{}", path.display())
        } else {
            write!(
                formatter,
                "unavailable (Quota could not open or write its log; the log would have been at {})",
                path.display()
            )
        }
    }
}

/// The log file's path, resolved from the application's own log directory.
fn file_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<PathBuf> {
    app.path()
        .app_log_dir()
        .map(|directory| directory.join(FILE_NAME))
}

pub(crate) fn browser_failure(url: &str, reason: &str) {
    let host = tauri::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned));
    tracing::warn!(
        host = host.as_deref().unwrap_or("unknown"),
        reason,
        "browser launch failed"
    );
}

/// Opens the log file for appending, rolling an oversized one over first.
///
/// Called once, from setup, when the host can resolve its directory. Every
/// failure is swallowed: a log that cannot be opened must never stop the run.
pub(crate) fn attach(app: &tauri::AppHandle) {
    if let Ok(file) = file_path(app) {
        attach_path(&file);
    }
}

fn attach_path(file: &Path) {
    let Ok(mut open) = OPEN.lock() else {
        return;
    };
    *open = None;
    let Some(directory) = file.parent() else {
        return;
    };
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    if let Ok(meta) = fs::metadata(file)
        && meta.len() >= MAX_BYTES
    {
        drop(fs::remove_file(directory.join(BACKUP_NAME)));
        drop(fs::rename(file, directory.join(BACKUP_NAME)));
    }
    if let Ok(opened) = OpenOptions::new().create(true).append(true).open(file) {
        *open = Some(opened);
    }
}

/// Appends to the file, when there is one.
fn append(bytes: &[u8]) {
    let Ok(mut open) = OPEN.lock() else {
        return;
    };
    if let Some(file) = open.as_mut()
        && file.write_all(bytes).is_err()
    {
        *open = None;
    }
}

/// The subscriber's writer: standard output, and the file when one is open.
///
/// One type stands for both halves so the subscriber is built the same way
/// before and after [`attach`] runs.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct LogWriter;

impl Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        // Neither half may fail the write: a closed standard output, on a
        // build with no console, is the expected state, not an error.
        drop(io::stdout().write_all(bytes));
        append(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        drop(io::stdout().flush());
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogWriter {
    type Writer = LogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        *self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The writer reports every byte as handled even if a destination drops it,
    /// so logging never fails the caller's operation. Availability is reported
    /// separately through the location shown in failure messages.
    #[test]
    fn the_writer_reports_availability_and_never_fails() {
        assert_eq!(
            LogLocation(None).to_string(),
            "unavailable (the application log directory could not be resolved)"
        );
        let mut writer = LogWriter;
        let line = b"a line about what went wrong\n";
        assert_eq!(writer.write(line).unwrap(), line.len());
        assert_eq!(writer.write(b"").unwrap(), 0);
        writer.flush().unwrap();
        // With no file attached the write is dropped after standard output.
        append(b"unattached\n");

        let directory =
            std::env::temp_dir().join(format!("quota-log-availability-{}", std::process::id()));
        fs::create_dir_all(&directory).expect("a scratch directory");
        let blocker = directory.join("blocked");
        fs::write(&blocker, b"a file").expect("a blocked directory");
        let path = blocker.join(FILE_NAME);
        attach_path(&path);
        let unavailable = LogLocation(Some(path.clone())).to_string();
        assert_eq!(
            unavailable,
            format!(
                "unavailable (Quota could not open or write its log; the log would have been at {})",
                path.display()
            )
        );

        let path = directory.join("directory-instead-of-log");
        fs::create_dir(&path).expect("a log path that cannot be opened as a file");
        attach_path(&path);
        assert!(
            LogLocation(Some(path))
                .to_string()
                .starts_with("unavailable")
        );

        let path = directory.join(FILE_NAME);
        attach_path(&path);
        let location = LogLocation(Some(path.clone()));
        assert_eq!(location.to_string(), path.to_string_lossy());
        assert_eq!(writer.write(b"recorded failure\n").unwrap(), 17);
        assert_eq!(fs::read(&path).unwrap(), b"recorded failure\n");

        *OPEN.lock().unwrap() = Some(File::open(&path).expect("a read-only log handle"));
        assert_eq!(writer.write(b"discarded failure\n").unwrap(), 18);
        assert_eq!(fs::read(&path).unwrap(), b"recorded failure\n");
        assert!(location.to_string().starts_with("unavailable"));
        assert!(location.to_string().contains("the log would have been at"));
        assert!(OPEN.lock().unwrap().is_none());
        fs::remove_dir_all(directory).expect("the scratch directory can be removed");
    }

    #[test]
    fn browser_failure_logs_only_the_host_and_safe_reason() {
        let path =
            std::env::temp_dir().join(format!("quota-browser-log-{}.txt", std::process::id()));
        let file = File::create(&path).expect("a log can be created");
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_writer(move || file.try_clone().expect("the log can be shared"))
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            browser_failure(
                "https://auth.example.test/private-device?code=PRIVATE-CODE#private-fragment",
                "the system browser refused the page",
            );
        });
        let logged = fs::read_to_string(&path).expect("the failure was logged");
        assert!(logged.contains("auth.example.test"));
        assert!(logged.contains("the system browser refused the page"));
        assert!(!logged.contains("https://"));
        assert!(!logged.contains("private-device"));
        assert!(!logged.contains("PRIVATE-CODE"));
        assert!(!logged.contains("private-fragment"));
        fs::remove_file(path).expect("the log can be removed");
    }

    /// The subscriber can be built from the writer before a file exists.
    #[test]
    fn the_writer_satisfies_the_subscriber_contract() {
        fn assert_make_writer<'a, M: tracing_subscriber::fmt::MakeWriter<'a>>() {}
        assert_make_writer::<LogWriter>();
    }
}
