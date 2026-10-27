use anyhow::Result;
use std::path::Path;
use std::process::{Command, Stdio};

/// Reveal a file in the desktop's default file manager.
///
/// Tries the standard `org.freedesktop.FileManager1` interface first so the
/// file is selected, then falls back to opening its parent directory.
pub fn reveal(path: &Path) -> Result<()> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    let opened = Command::new("dbus-send")
        .args([
            "--session",
            "--dest=org.freedesktop.FileManager1",
            "--type=method_call",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
            &format!("array:string:{}", file_uri(&path)),
            "string:",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);

    if opened {
        return Ok(());
    }

    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    Command::new("xdg-open")
        .arg(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    Ok(())
}

fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b if b.is_ascii_alphanumeric() => uri.push(b as char),
            b'/' | b'-' | b'_' | b'.' | b'~' => uri.push(byte as char),
            b => uri.push_str(&format!("%{b:02X}")),
        }
    }
    uri
}
