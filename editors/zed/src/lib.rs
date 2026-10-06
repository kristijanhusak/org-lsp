//! Zed extension for the org-lsp language server.
//!
//! Starts `org-lsp` for Org files: the one on PATH, or the nightly build,
//! downloaded once. The server's options come from Zed's `lsp.org-lsp`
//! settings. Zed cannot run a server's command from a key, so the commands
//! are in its code actions (the agenda and capture among them), and what
//! they ask for is asked with buttons; questions that want typed text (a
//! date) cannot be asked.

use std::fs;

use zed::settings::LspSettings;
use zed_extension_api::{self as zed, serde_json, LanguageServerId, Result};

const REPOSITORY: &str = "kristijanhusak/org-lsp";
const RELEASE: &str = "nightly";

struct OrgLsp {
    cached: Option<String>,
}

impl OrgLsp {
    /// The server from the nightly release, downloaded on first use.
    fn downloaded(&mut self, id: &LanguageServerId) -> Result<String> {
        if let Some(path) = &self.cached {
            if fs::metadata(path).is_ok_and(|stat| stat.is_file()) {
                return Ok(path.clone());
            }
        }
        let (os, arch) = zed::current_platform();
        let platform = match (os, arch) {
            (zed::Os::Linux, zed::Architecture::X8664) => "linux-x86_64",
            (zed::Os::Linux, zed::Architecture::Aarch64) => "linux-arm64",
            (zed::Os::Mac, zed::Architecture::Aarch64) => "macos-arm64",
            (zed::Os::Mac, zed::Architecture::X8664) => "macos-x86_64",
            (zed::Os::Windows, zed::Architecture::X8664) => "windows-x86_64",
            _ => return Err("org-lsp has no build for this platform; put `org-lsp` on PATH".to_string()),
        };
        let (archive, kind, exe) = match os {
            zed::Os::Windows => (format!("org-lsp-{platform}.zip"), zed::DownloadedFileType::Zip, "org-lsp.exe"),
            _ => (format!("org-lsp-{platform}.tar.gz"), zed::DownloadedFileType::GzipTar, "org-lsp"),
        };
        let release = zed::github_release_by_tag_name(REPOSITORY, RELEASE)?;
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == archive)
            .ok_or_else(|| format!("no {archive} in the {RELEASE} release"))?;
        // A directory per release, so that a new nightly is fetched.
        let directory = format!("org-lsp-{}", release.version.replace(['/', ' '], "-"));
        let path = format!("{directory}/{exe}");
        if !fs::metadata(&path).is_ok_and(|stat| stat.is_file()) {
            zed::set_language_server_installation_status(id, &zed::LanguageServerInstallationStatus::Downloading);
            zed::download_file(&asset.download_url, &directory, kind)?;
            zed::make_file_executable(&path)?;
            // Earlier downloads go.
            if let Ok(entries) = fs::read_dir(".") {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with("org-lsp-") && name != directory {
                        let _ = fs::remove_dir_all(entry.path());
                    }
                }
            }
        }
        self.cached = Some(path.clone());
        Ok(path)
    }
}

impl zed::Extension for OrgLsp {
    fn new() -> Self {
        OrgLsp { cached: None }
    }

    fn language_server_command(&mut self, id: &LanguageServerId, worktree: &zed::Worktree) -> Result<zed::Command> {
        let configured = LspSettings::for_worktree(id.as_ref(), worktree).ok().and_then(|settings| settings.binary);
        if let Some(path) = configured.as_ref().and_then(|binary| binary.path.clone()) {
            let args = configured.and_then(|binary| binary.arguments).unwrap_or_default();
            return Ok(zed::Command { command: path, args, env: Default::default() });
        }
        let command = match worktree.which("org-lsp") {
            Some(path) => path,
            None => self.downloaded(id).map_err(|error| {
                format!(
                    "org-lsp is not on PATH, and its nightly build could not be downloaded ({error}). \
                     Put `org-lsp` on PATH (`cargo install --path server/org-lsp` in a checkout), or name \
                     the binary in Zed's settings: \"lsp\": {{ \"org-lsp\": {{ \"binary\": {{ \"path\": \"/path/to/org-lsp\" }} }} }}"
                )
            })?,
        };
        Ok(zed::Command { command, args: Vec::new(), env: Default::default() })
    }

    /// The server's options, from `lsp.org-lsp.initialization_options`.
    /// Zed fetches no documents of the server's own: the agenda is a file.
    fn language_server_initialization_options(
        &mut self,
        id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<serde_json::Value>> {
        let configured = LspSettings::for_worktree(id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.initialization_options)
            .unwrap_or_else(|| serde_json::json!({}));
        let mut options = match configured {
            serde_json::Value::Object(options) => options,
            _ => serde_json::Map::new(),
        };
        options.insert("virtualDocuments".to_string(), serde_json::Value::Bool(false));
        // Zed runs no command from a key and does nothing with a command's
        // result: the commands come as code actions, the agenda's through
        // the command port that the extension's tasks call, and what they
        // ask for is asked with buttons.
        options.insert("commandActions".to_string(), serde_json::Value::Bool(true));
        options.insert("messagePrompts".to_string(), serde_json::Value::Bool(true));
        Ok(Some(serde_json::Value::Object(options)))
    }
}

zed::register_extension!(OrgLsp);
