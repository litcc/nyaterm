use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime};

use nyaterm_transport::{
    RemoteFilePath, RemoteFileService, SftpTransferControl, SftpTransferOptions,
};

use crate::models::{TransferJobEvent, TransferJobOutput, TransferJobResult};

use super::{
    EXTERNAL_EDITOR_STARTUP_SUPPRESSION, EXTERNAL_EDITOR_UPLOAD_SETTLE,
    EXTERNAL_EDITOR_WATCH_INTERVAL,
};

pub(super) fn sanitize_local_open_segment(input: &str) -> String {
    let sanitized: String = input
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();
    let sanitized = sanitized.trim_matches(['.', ' ']).trim();
    if sanitized.is_empty() {
        "remote-file".to_string()
    } else {
        sanitized.to_string()
    }
}

pub(super) fn open_local_path_with_editor(path: &Path, editor_command: &str) -> Result<(), String> {
    let command = editor_command.trim();
    if command.is_empty() {
        open_local_path_with_system_default(path)
    } else {
        let mut parts = command.split_whitespace();
        let Some(program) = parts.next() else {
            return open_local_path_with_system_default(path);
        };
        Command::new(program)
            .args(parts)
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("failed to open {} with {program}: {error}", path.display()))
    }
}

pub(super) fn open_local_path_with_system_default(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(path);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", ""]).arg(path);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("failed to open {}: {error}", path.display()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemoteFileTextKind {
    Text,
    Binary,
    Unknown,
}

pub(super) fn remote_file_text_kind(name: &str) -> RemoteFileTextKind {
    if is_known_text_file(name) {
        RemoteFileTextKind::Text
    } else if is_known_binary_file(name) {
        RemoteFileTextKind::Binary
    } else {
        RemoteFileTextKind::Unknown
    }
}

pub(super) fn remote_file_extension(name: &str) -> String {
    let normalized = name.trim().to_ascii_lowercase();
    let base = normalized
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(normalized.as_str());
    let Some(index) = base.rfind('.') else {
        return String::new();
    };
    if index == 0 {
        String::new()
    } else {
        base[index + 1..].to_string()
    }
}

pub(super) fn remote_file_basename(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase()
}

pub(super) fn is_known_binary_file(name: &str) -> bool {
    matches!(
        remote_file_extension(name).as_str(),
        "jpg"
            | "jpeg"
            | "png"
            | "gif"
            | "bmp"
            | "webp"
            | "ico"
            | "tiff"
            | "tif"
            | "heic"
            | "heif"
            | "avif"
            | "mp3"
            | "wav"
            | "flac"
            | "aac"
            | "ogg"
            | "wma"
            | "m4a"
            | "mp4"
            | "avi"
            | "mkv"
            | "mov"
            | "wmv"
            | "flv"
            | "webm"
            | "zip"
            | "rar"
            | "7z"
            | "tar"
            | "gz"
            | "bz2"
            | "xz"
            | "zst"
            | "tgz"
            | "tbz2"
            | "txz"
            | "iso"
            | "dmg"
            | "exe"
            | "dll"
            | "so"
            | "dylib"
            | "bin"
            | "msi"
            | "deb"
            | "rpm"
            | "apk"
            | "jar"
            | "war"
            | "ear"
            | "pdf"
            | "doc"
            | "docx"
            | "xls"
            | "xlsx"
            | "ppt"
            | "pptx"
            | "ttf"
            | "otf"
            | "woff"
            | "woff2"
            | "db"
            | "sqlite"
            | "sqlite3"
            | "o"
            | "obj"
            | "pyc"
            | "pyo"
            | "class"
    )
}

pub(super) fn is_known_text_file(name: &str) -> bool {
    let base_name = remote_file_basename(name);
    let normalized = base_name.trim_start_matches('.');
    let extension = remote_file_extension(name);
    matches!(
        extension.as_str(),
        "asc"
            | "bash"
            | "bat"
            | "c"
            | "cfg"
            | "cc"
            | "cjs"
            | "cmd"
            | "conf"
            | "cpp"
            | "cs"
            | "css"
            | "cxx"
            | "csv"
            | "dart"
            | "diff"
            | "env"
            | "fish"
            | "go"
            | "h"
            | "hpp"
            | "htm"
            | "html"
            | "ini"
            | "java"
            | "js"
            | "json"
            | "json5"
            | "jsonc"
            | "jsx"
            | "log"
            | "lua"
            | "markdown"
            | "md"
            | "mjs"
            | "patch"
            | "pem"
            | "php"
            | "pl"
            | "properties"
            | "proto"
            | "ps1"
            | "py"
            | "r"
            | "rb"
            | "rs"
            | "sass"
            | "scss"
            | "service"
            | "sh"
            | "socket"
            | "sql"
            | "swift"
            | "timer"
            | "toml"
            | "ts"
            | "tsx"
            | "txt"
            | "vue"
            | "xml"
            | "yaml"
            | "yml"
            | "zsh"
    ) || matches!(
        normalized,
        "bash_profile"
            | "bash_login"
            | "bash_logout"
            | "bashrc"
            | "cmakelists.txt"
            | "dockerfile"
            | "editorconfig"
            | "env"
            | "env.local"
            | "gitconfig"
            | "gitignore"
            | "gitmodules"
            | "gitattributes"
            | "makefile"
            | "gnumakefile"
            | "npmrc"
            | "profile"
            | "zprofile"
            | "zshenv"
            | "zshrc"
    ) || base_name.ends_with(".dockerfile")
        || base_name.ends_with(".nginx.conf")
        || base_name == "docker-compose.yml"
        || base_name == "docker-compose.yaml"
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LocalFileFingerprint {
    len: u64,
    modified: Option<SystemTime>,
}

impl LocalFileFingerprint {
    fn from_path(path: &Path) -> std::io::Result<Self> {
        let metadata = fs::metadata(path)?;
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }

    fn is_content_change_from(&self, previous: &Self, within_startup_window: bool) -> bool {
        if self.len != previous.len {
            return true;
        }
        self.modified != previous.modified && !within_startup_window
    }
}

pub(super) fn watch_external_editor_file(
    job_id: String,
    remote_path: RemoteFilePath,
    local_path: PathBuf,
    transfer_tx: futures::channel::mpsc::UnboundedSender<TransferJobResult>,
) {
    let watch_started = Instant::now();
    let mut baseline = match LocalFileFingerprint::from_path(&local_path) {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            let _ = transfer_tx.unbounded_send(TransferJobResult {
                id: job_id,
                event: TransferJobEvent::Finished(Err(format!(
                    "external editor watch failed for {}: {error}",
                    local_path.display()
                ))),
            });
            return;
        }
    };

    loop {
        std::thread::sleep(EXTERNAL_EDITOR_WATCH_INTERVAL);
        let current = match LocalFileFingerprint::from_path(&local_path) {
            Ok(fingerprint) => fingerprint,
            Err(_) => break,
        };
        if !current.is_content_change_from(
            &baseline,
            watch_started.elapsed() <= EXTERNAL_EDITOR_STARTUP_SUPPRESSION,
        ) {
            if current != baseline {
                baseline = current;
            }
            continue;
        }

        std::thread::sleep(EXTERNAL_EDITOR_UPLOAD_SETTLE);
        let settled = LocalFileFingerprint::from_path(&local_path).unwrap_or(current);
        baseline = settled;
        let _ = transfer_tx.unbounded_send(TransferJobResult {
            id: job_id.clone(),
            event: TransferJobEvent::ExternalModified {
                remote_path: remote_path.display_path.clone(),
                raw_path_token: remote_path.raw_path_token.clone(),
                local_path: local_path.clone(),
            },
        });
        if let Ok(after_upload) = LocalFileFingerprint::from_path(&local_path) {
            baseline = after_upload;
        }
    }
}

pub(super) fn external_editor_watch_key(remote_path: &str, local_path: &Path) -> String {
    format!("{remote_path}\n{}", local_path.display())
}

pub(super) fn upload_external_editor_file(
    service: RemoteFileService,
    job_id: &str,
    remote_path: &str,
    raw_path_token: Option<String>,
    local_path: &Path,
    transfer_options: SftpTransferOptions,
    transfer_tx: &futures::channel::mpsc::UnboundedSender<TransferJobResult>,
) {
    let _ = transfer_tx.unbounded_send(TransferJobResult {
        id: job_id.to_string(),
        event: TransferJobEvent::Started {
            detail: format!("Syncing external edit {remote_path}"),
        },
    });
    let control = SftpTransferControl::new();
    let progress_id = job_id.to_string();
    let progress_tx = transfer_tx.clone();
    let remote_file_path = RemoteFilePath {
        display_path: remote_path.to_string(),
        raw_path_token,
    };
    let result = service
        .upload_remote_file_with_progress_and_control_options(
            local_path.to_path_buf(),
            &remote_file_path,
            control,
            transfer_options,
            move |progress| {
                let _ = progress_tx.unbounded_send(TransferJobResult {
                    id: progress_id.clone(),
                    event: TransferJobEvent::Progress(progress),
                });
            },
        )
        .map(TransferJobOutput::Summary)
        .map_err(|error| error.to_string());
    let _ = transfer_tx.unbounded_send(TransferJobResult {
        id: job_id.to_string(),
        event: TransferJobEvent::Finished(result),
    });
}
