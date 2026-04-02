//! Read-only workspace introspection, ported from the DevBridge `introspection`
//! and `workspace` crates into the HyprBridge platform.
//!
//! DevBridge is GPL-3.0 like this project, so its detection logic is reused
//! here. Types are HyprBridge-owned: no dependency on DevBridge, no second
//! daemon and no shared mutable configuration.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Host operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostOs {
    /// Linux.
    Linux,
    /// macOS.
    Macos,
    /// Windows.
    Windows,
    /// Unrecognised.
    Unknown,
}

impl HostOs {
    /// Detect the current host OS.
    #[must_use]
    pub fn current() -> Self {
        if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Unknown
        }
    }
}

/// Linux graphical session type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinuxSessionType {
    /// Wayland compositor.
    Wayland,
    /// X11 display server.
    X11,
    /// Text console.
    Console,
    /// No graphical session.
    Headless,
    /// Unknown.
    Unknown,
}

/// Linux-specific platform details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinuxSessionInfo {
    /// Distribution id from `/etc/os-release`.
    pub distro_id: Option<String>,
    /// Distribution version from `/etc/os-release`.
    pub version_id: Option<String>,
    /// Desktop environment or compositor name.
    pub desktop_environment: Option<String>,
    /// Graphical session type.
    pub session_type: LinuxSessionType,
}

/// Platform snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostPlatform {
    /// Host OS.
    pub os: HostOs,
    /// CPU architecture.
    pub architecture: String,
    /// Hostname.
    pub hostname: String,
    /// Linux session details, when applicable.
    pub linux: Option<LinuxSessionInfo>,
}

/// A tool found on `PATH`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainInfo {
    /// Tool name.
    pub name: String,
    /// Whether it is on `PATH`.
    pub available: bool,
}

/// A tracked environment variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyValueSummary {
    /// Variable name.
    pub key: String,
    /// Variable value.
    pub value: String,
}

/// Environment snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentSummary {
    /// Current user.
    pub user: Option<String>,
    /// Home directory.
    pub home_dir: Option<String>,
    /// Login shell.
    pub shell: Option<String>,
    /// Current working directory.
    pub current_dir: String,
    /// Detected toolchains.
    pub toolchains: Vec<ToolchainInfo>,
    /// Available container runtimes.
    pub container_runtimes: Vec<String>,
    /// Selected environment variables.
    pub variables: Vec<KeyValueSummary>,
}

/// Complete local inspection result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectionSnapshot {
    /// Platform details.
    pub platform: HostPlatform,
    /// Environment details.
    pub environment: EnvironmentSummary,
    /// Detection time (UTC, ISO 8601).
    pub detected_at: DateTime<Utc>,
    /// Human-readable notes.
    pub notes: Vec<String>,
}

/// Assessment of remote-desktop capability on this host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopAssessment {
    /// Suggested backend strategy.
    pub backend: String,
    /// Capability state: supported, partial or unsupported.
    pub state: String,
    /// Short explanation.
    pub summary: String,
    /// Caveats and next steps.
    pub notes: Vec<String>,
}

/// What workspace mode can do on this host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCapability {
    /// Files can be listed and read.
    pub files: bool,
    /// Logs can be inspected.
    pub logs: bool,
    /// Processes can be inspected.
    pub processes: bool,
    /// Ports can be inspected.
    pub ports: bool,
    /// A shell is available.
    pub terminal: bool,
    /// Summary sentence.
    pub summary: String,
}

/// One directory entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryEntry {
    /// File name.
    pub name: String,
    /// Absolute path.
    pub path: String,
    /// Whether this entry is a directory.
    pub is_dir: bool,
    /// File size in bytes.
    pub size_bytes: Option<u64>,
}

/// One text file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDocument {
    /// Absolute path.
    pub path: String,
    /// File contents.
    pub content: String,
    /// Whether the file is read-only on disk.
    pub read_only: bool,
}

/// Query parameters for file endpoints.
#[derive(Debug, Clone, Deserialize)]
pub struct PathQuery {
    /// Path relative to the workspace root.
    pub path: Option<String>,
}

const TRACKED_ENV_KEYS: &[&str] = &[
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_TYPE",
    "WAYLAND_DISPLAY",
    "DISPLAY",
    "HYPRLAND_INSTANCE_SIGNATURE",
    "LANG",
    "TERM",
];

/// Inspect the local host without modifying anything.
#[must_use]
pub fn inspect_local() -> InspectionSnapshot {
    InspectionSnapshot {
        platform: detect_platform(),
        environment: collect_environment(),
        detected_at: Utc::now(),
        notes: vec![
            "Read-only workspace introspection from HyprBridge.".into(),
            "Workspace file access is confined to the configured root.".into(),
        ],
    }
}

fn detect_platform() -> HostPlatform {
    let linux = if HostOs::current() == HostOs::Linux {
        Some(detect_linux_session())
    } else {
        None
    };
    HostPlatform {
        os: HostOs::current(),
        architecture: env::consts::ARCH.to_string(),
        hostname: fs::read_to_string("/etc/hostname")
            .map(|text| text.trim().to_string())
            .or_else(|_| env::var("HOSTNAME"))
            .unwrap_or_else(|_| "unknown".into()),
        linux,
    }
}

fn detect_linux_session() -> LinuxSessionInfo {
    let mut distro_id = None;
    let mut version_id = None;
    if let Ok(text) = fs::read_to_string("/etc/os-release") {
        for line in text.lines() {
            if let Some(value) = line.strip_prefix("ID=") {
                distro_id = Some(trim_quotes(value));
            }
            if let Some(value) = line.strip_prefix("VERSION_ID=") {
                version_id = Some(trim_quotes(value));
            }
        }
    }
    let session_type = env::var("XDG_SESSION_TYPE")
        .ok()
        .and_then(|value| match value.as_str() {
            "wayland" => Some(LinuxSessionType::Wayland),
            "x11" => Some(LinuxSessionType::X11),
            "tty" => Some(LinuxSessionType::Console),
            _ => None,
        })
        .or_else(|| {
            if env::var_os("WAYLAND_DISPLAY").is_some() {
                Some(LinuxSessionType::Wayland)
            } else if env::var_os("DISPLAY").is_some() {
                Some(LinuxSessionType::X11)
            } else {
                Some(LinuxSessionType::Headless)
            }
        })
        .unwrap_or(LinuxSessionType::Unknown);
    LinuxSessionInfo {
        distro_id,
        version_id,
        desktop_environment: env::var("XDG_CURRENT_DESKTOP")
            .ok()
            .or_else(|| env::var("DESKTOP_SESSION").ok()),
        session_type,
    }
}

fn trim_quotes(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

fn collect_environment() -> EnvironmentSummary {
    let tool_names = [
        "cargo", "rustc", "git", "node", "pnpm", "python3", "go", "docker", "podman",
    ];
    let toolchains: Vec<ToolchainInfo> = tool_names
        .iter()
        .map(|name| ToolchainInfo {
            name: (*name).to_string(),
            available: tool_available(name),
        })
        .collect();
    let container_runtimes = toolchains
        .iter()
        .filter(|tool| tool.available && (tool.name == "docker" || tool.name == "podman"))
        .map(|tool| tool.name.clone())
        .collect();
    EnvironmentSummary {
        user: env::var("USER").ok().or_else(|| env::var("USERNAME").ok()),
        home_dir: env::var("HOME")
            .ok()
            .or_else(|| env::var("USERPROFILE").ok()),
        shell: env::var("SHELL").ok().or_else(|| env::var("COMSPEC").ok()),
        current_dir: env::current_dir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| ".".into()),
        toolchains,
        container_runtimes,
        variables: TRACKED_ENV_KEYS
            .iter()
            .filter_map(|key| env::var(key).ok().map(|value| (*key, value)))
            .map(|(key, value)| KeyValueSummary {
                key: key.to_string(),
                value,
            })
            .collect(),
    }
}

fn tool_available(name: &str) -> bool {
    Command::new(if cfg!(windows) { "where" } else { "which" })
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Assess remote-desktop capability using the detected session type.
#[must_use]
pub fn assess_desktop(snapshot: &InspectionSnapshot) -> DesktopAssessment {
    let linux = snapshot.platform.linux.as_ref();
    let session = linux.map(|info| info.session_type.clone());
    let desktop = linux.and_then(|info| info.desktop_environment.as_deref());
    match snapshot.platform.os {
        HostOs::Linux => match session {
            Some(LinuxSessionType::Wayland) => DesktopAssessment {
                backend: if desktop.is_some_and(|value| value.to_lowercase().contains("hyprland")) {
                    "wayland-hyprland"
                } else {
                    "wayland-portal"
                }
                .into(),
                state: "partial".into(),
                summary: "Wayland inspectable; injection depends on compositor support and uinput permissions."
                    .into(),
                notes: vec![
                    "X11-only tools such as xinput and xbindkeys do nothing here.".into(),
                    "HyprBridge uses compositor IPC plus evdev/uinput for capture and injection.".into(),
                ],
            },
            Some(LinuxSessionType::X11) => DesktopAssessment {
                backend: "x11-native".into(),
                state: "supported".into(),
                summary: "X11 supports native pointer and key injection via XTEST.".into(),
                notes: vec!["X11 remains the most complete control path.".into()],
            },
            _ => DesktopAssessment {
                backend: "none".into(),
                state: "unsupported".into(),
                summary: "No usable graphical session detected.".into(),
                notes: vec!["Workspace mode remains available without a desktop session.".into()],
            },
        },
        HostOs::Macos => DesktopAssessment {
            backend: "macos-native".into(),
            state: "partial".into(),
            summary: "macOS requires Screen Recording and Accessibility permissions.".into(),
            notes: Vec::new(),
        },
        HostOs::Windows => DesktopAssessment {
            backend: "windows-native".into(),
            state: "partial".into(),
            summary: "Windows requires an interactive desktop session.".into(),
            notes: Vec::new(),
        },
        HostOs::Unknown => DesktopAssessment {
            backend: "none".into(),
            state: "unsupported".into(),
            summary: "Unsupported host operating system.".into(),
            notes: Vec::new(),
        },
    }
}

/// Capabilities exposed by workspace mode.
#[must_use]
pub fn workspace_capability(environment: &EnvironmentSummary) -> WorkspaceCapability {
    WorkspaceCapability {
        files: true,
        logs: true,
        processes: true,
        ports: true,
        terminal: environment.shell.is_some(),
        summary: "Read-only workspace introspection; file access stays inside the configured root."
            .into(),
    }
}

/// A file service confined to one root directory.
#[derive(Debug, Clone)]
pub struct WorkspaceService {
    root: PathBuf,
}

impl WorkspaceService {
    /// Create a service rooted at the given directory.
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root
            .as_ref()
            .canonicalize()
            .with_context(|| format!("workspace root {:?} does not exist", root.as_ref()))?;
        Ok(Self { root })
    }

    /// The canonical root path.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// List a directory inside the root.
    pub fn list_dir(&self, input: Option<&str>) -> Result<Vec<DirectoryEntry>> {
        let target = self.resolve(input.unwrap_or("."))?;
        let mut entries = Vec::new();
        for entry in fs::read_dir(&target)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            entries.push(DirectoryEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry.path().display().to_string(),
                is_dir: metadata.is_dir(),
                size_bytes: if metadata.is_file() {
                    Some(metadata.len())
                } else {
                    None
                },
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    /// Read a UTF-8 text file inside the root (maximum 1 MiB).
    pub fn read_text(&self, input: &str) -> Result<FileDocument> {
        let path = self.resolve(input)?;
        let metadata = fs::metadata(&path)?;
        if metadata.len() > 1024 * 1024 {
            return Err(anyhow!("file exceeds the 1 MiB read limit"));
        }
        Ok(FileDocument {
            path: path.display().to_string(),
            content: fs::read_to_string(&path)?,
            read_only: metadata.permissions().readonly(),
        })
    }

    fn resolve(&self, input: &str) -> Result<PathBuf> {
        let candidate = PathBuf::from(input);
        let joined = if candidate.is_absolute() {
            candidate
        } else {
            self.root.join(candidate)
        };
        let resolved = joined.canonicalize()?;
        if !resolved.starts_with(&self.root) {
            return Err(anyhow!("path escapes the workspace root"));
        }
        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> (tempfile::TempDir, WorkspaceService) {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("note.txt"), "hello").expect("write fixture");
        let service = WorkspaceService::new(dir.path()).expect("service");
        (dir, service)
    }

    #[test]
    fn lists_and_reads_inside_root() {
        let (_dir, service) = service();
        let entries = service.list_dir(None).expect("list");
        assert!(entries.iter().any(|entry| entry.name == "note.txt"));
        let document = service.read_text("note.txt").expect("read");
        assert_eq!(document.content, "hello");
    }

    #[test]
    fn rejects_paths_outside_root() {
        let (_dir, service) = service();
        assert!(service.read_text("../../etc/passwd").is_err());
        assert!(service.list_dir(Some("/etc")).is_err());
    }

    #[test]
    fn inspection_and_assessment_are_consistent() {
        let snapshot = inspect_local();
        let assessment = assess_desktop(&snapshot);
        assert!(!assessment.backend.is_empty());
        assert!(!assessment.summary.is_empty());
        assert!(workspace_capability(&snapshot.environment).files);
    }
}
