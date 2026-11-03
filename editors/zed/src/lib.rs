//! Zed extension for the org-lsp language server.
//!
//! Starts `org-lsp` for Org files: the one `scripts/install.sh` put beside
//! the extension, the one on PATH, or else the server of the extension's
//! own release (`RELEASE`), downloaded once. The server's options come
//! from Zed's `lsp.org-lsp` settings. Zed cannot run a server's command
//! from a key, so the commands are in its code actions (the agenda and
//! capture among them), and what they ask for is asked with buttons;
//! questions that want typed text (a date) cannot be asked.

use std::fs;
use std::io::Read;

use sha2::{Digest, Sha256};

use zed::settings::LspSettings;
use zed_extension_api::{self as zed, serde_json, LanguageServerId, Result};

const REPOSITORY: &str = "kristijanhusak/org-lsp";
/// The release whose server is downloaded: the extension's own (`v0.2.0`
/// for version 0.2.0), or the one named by `ORG_LSP_RELEASE` when it is
/// built, as the nightly build is with `nightly`.
const RELEASE: &str = match option_env!("ORG_LSP_RELEASE") {
    Some(release) => release,
    None => concat!("v", env!("CARGO_PKG_VERSION")),
};

struct OrgLsp {
    cached: Option<String>,
}

impl OrgLsp {
    /// The server from `RELEASE`, downloaded on first use.
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
        let (archive, exe) = match os {
            zed::Os::Windows => (format!("org-lsp-{platform}.zip"), "org-lsp.exe"),
            _ => (format!("org-lsp-{platform}.tar.gz"), "org-lsp"),
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
            // The archive as it is, checked against the release's
            // SHA256SUMS before it is unpacked, as scripts/install.sh
            // checks it. Nothing is kept of one that fails.
            let sums = release.assets.iter().find(|asset| asset.name == "SHA256SUMS");
            let sums = sums.map(|sums| sums.download_url.as_str());
            if let Err(error) = install(&asset.download_url, sums, &directory, &archive, exe) {
                let _ = fs::remove_dir_all(&directory);
                return Err(error);
            }
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

/// Download `archive` from `url` into `directory`, check it against the
/// release's SHA256SUMS at `sums` (a release without them is not checked,
/// as with scripts/install.sh), and unpack the server `exe` from it.
fn install(url: &str, sums: Option<&str>, directory: &str, archive: &str, exe: &str) -> Result<()> {
    fs::create_dir_all(directory).map_err(|error| format!("{directory}: {error}"))?;
    let file = format!("{directory}/{archive}");
    zed::download_file(url, &file, zed::DownloadedFileType::Uncompressed)?;
    let data = fs::read(&file).map_err(|error| format!("{file}: {error}"))?;
    match sums {
        Some(sums_url) => {
            let sums_file = format!("{directory}/SHA256SUMS");
            zed::download_file(sums_url, &sums_file, zed::DownloadedFileType::Uncompressed)?;
            let sums = fs::read_to_string(&sums_file).map_err(|error| format!("{sums_file}: {error}"))?;
            check(&data, archive, &sums, RELEASE)?;
            let _ = fs::remove_file(&sums_file);
        }
        None => eprintln!("org-lsp: the {RELEASE} release has no SHA256SUMS: {archive} is not checked"),
    }
    let binary = unpack(&data, archive, exe)?;
    let _ = fs::remove_file(&file);
    let path = format!("{directory}/{exe}");
    fs::write(&path, binary).map_err(|error| format!("{path}: {error}"))
}

/// Whether `data`, the release's file `name`, has the checksum the
/// release's SHA256SUMS (`sums`) give it: what is wrong, if not.
fn check(data: &[u8], name: &str, sums: &str, release: &str) -> Result<()> {
    let expected = sums.lines().find_map(|line| {
        let (sum, listed) = line.trim_end().split_once(char::is_whitespace)?;
        (listed.trim_start().trim_start_matches('*') == name).then(|| sum.to_ascii_lowercase())
    });
    // The files built together are all in SHA256SUMS: one that is not is
    // left from an older build (a platform whose build failed).
    let Some(expected) = expected else {
        return Err(format!(
            "{name} is not in the SHA256SUMS of the {release} release: it is not from the same build, so it is \
             not installed"
        ));
    };
    let actual: String = Sha256::digest(data).iter().map(|byte| format!("{byte:02x}")).collect();
    if actual != expected {
        return Err(format!("{name} does not match its checksum in SHA256SUMS: the download is broken"));
    }
    Ok(())
}

/// The server `exe` out of the archive `data`, a `.zip` or a `.tar.gz`.
fn unpack(data: &[u8], archive: &str, exe: &str) -> Result<Vec<u8>> {
    let broken = |error: &dyn std::fmt::Display| format!("{archive}: {error}");
    let mut binary = Vec::new();
    if archive.ends_with(".zip") {
        let mut files = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|error| broken(&error))?;
        let mut file = files.by_name(exe).map_err(|_| format!("no {exe} in {archive}"))?;
        file.read_to_end(&mut binary).map_err(|error| broken(&error))?;
        return Ok(binary);
    }
    let mut files = tar::Archive::new(flate2::read::GzDecoder::new(data));
    for entry in files.entries().map_err(|error| broken(&error))? {
        let mut entry = entry.map_err(|error| broken(&error))?;
        let path = entry.path().map_err(|error| broken(&error))?.into_owned();
        if path.to_str().is_some_and(|path| path.trim_start_matches("./") == exe) {
            entry.read_to_end(&mut binary).map_err(|error| broken(&error))?;
            return Ok(binary);
        }
    }
    Err(format!("no {exe} in {archive}"))
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
        // scripts/install.sh puts the server in the extension's directory,
        // which relative paths are in.
        let installed = match zed::current_platform().0 {
            zed::Os::Windows => "org-lsp.exe",
            _ => "org-lsp",
        };
        if fs::metadata(installed).is_ok_and(|stat| stat.is_file()) {
            return Ok(zed::Command { command: installed.to_string(), args: Vec::new(), env: Default::default() });
        }
        let command = match worktree.which("org-lsp") {
            Some(path) => path,
            None => self.downloaded(id).map_err(|error| {
                format!(
                    "org-lsp is not on PATH, and its {RELEASE} build could not be downloaded ({error}). \
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
        options.insert("virtual_documents".to_string(), serde_json::Value::Bool(false));
        // Its rules color org-lsp's own token types.
        options.insert("org_client".to_string(), serde_json::Value::Bool(true));
        // Zed runs no command from a key and does nothing with a command's
        // result: the commands come as code actions, the agenda's through
        // the command port that the extension's tasks call, and what they
        // ask for is asked with buttons.
        options.insert("command_actions".to_string(), serde_json::Value::Bool(true));
        options.insert("message_prompts".to_string(), serde_json::Value::Bool(true));
        Ok(Some(serde_json::Value::Object(options)))
    }
}

zed::register_extension!(OrgLsp);

#[cfg(test)]
mod tests {
    use super::*;

    fn tar_gz(name: &str, content: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let encoder = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
            let mut files = tar::Builder::new(encoder);
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            files.append_data(&mut header, name, content).unwrap();
            files.into_inner().unwrap().finish().unwrap();
        }
        out
    }

    fn sum(data: &[u8]) -> String {
        Sha256::digest(data).iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn checks_the_archive_against_the_release_sums() {
        let data = tar_gz("org-lsp", b"server");
        let name = "org-lsp-linux-x86_64.tar.gz";
        let sums = format!("{}  install.sh\n{}  {name}\n", "1".repeat(64), sum(&data));
        assert_eq!(check(&data, name, &sums, "nightly"), Ok(()));
        // As `sha256sum -b` writes it.
        assert_eq!(check(&data, name, &format!("{} *{name}\n", sum(&data).to_uppercase()), "nightly"), Ok(()));
        let broken = format!("{}  {name}\n", "0".repeat(64));
        assert_eq!(
            check(&data, name, &broken, "nightly"),
            Err(format!("{name} does not match its checksum in SHA256SUMS: the download is broken"))
        );
        let unlisted = format!("{}  org-lsp-other.tar.gz\n", sum(&data));
        assert!(check(&data, name, &unlisted, "v0.2.0")
            .unwrap_err()
            .starts_with(&format!("{name} is not in the SHA256SUMS of the v0.2.0 release")));
    }

    #[test]
    fn unpacks_the_server() {
        assert_eq!(unpack(&tar_gz("org-lsp", b"server"), "a.tar.gz", "org-lsp"), Ok(b"server".to_vec()));
        assert_eq!(unpack(&tar_gz("./org-lsp", b"server"), "a.tar.gz", "org-lsp"), Ok(b"server".to_vec()));
        assert_eq!(unpack(&tar_gz("other", b"x"), "a.tar.gz", "org-lsp"), Err("no org-lsp in a.tar.gz".to_string()));
        let mut zipped = std::io::Cursor::new(Vec::new());
        {
            use std::io::Write;
            let mut files = zip::ZipWriter::new(&mut zipped);
            files.start_file("org-lsp.exe", zip::write::SimpleFileOptions::default()).unwrap();
            files.write_all(b"server").unwrap();
            files.finish().unwrap();
        }
        assert_eq!(unpack(zipped.get_ref(), "a.zip", "org-lsp.exe"), Ok(b"server".to_vec()));
    }
}
