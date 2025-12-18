use zed_extension_api::{self as zed, LanguageServerId, Result, Worktree};

struct I18nExtension {
    cached_binary_path: Option<String>,
}

impl zed::Extension for I18nExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        let binary_path = self.get_server_binary_path(language_server_id, worktree)?;

        Ok(zed::Command {
            command: binary_path,
            args: vec![],
            env: vec![],
        })
    }
}

impl I18nExtension {
    fn get_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<String> {
        if let Some(path) = &self.cached_binary_path {
            if std::fs::metadata(path).is_ok() {
                return Ok(path.clone());
            }
        }

        if let Some(path) = worktree.which("i18n-lsp") {
            self.cached_binary_path = Some(path.clone());
            return Ok(path);
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = zed::latest_github_release(
            "user/zed-i18n",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (platform, arch) = zed::current_platform();
        let asset_name = format!(
            "i18n-lsp-{}-{}.{}",
            match arch {
                zed::Architecture::Aarch64 => "aarch64",
                zed::Architecture::X8664 => "x86_64",
                zed::Architecture::X86 => "x86",
            },
            match platform {
                zed::Os::Mac => "apple-darwin",
                zed::Os::Linux => "unknown-linux-gnu",
                zed::Os::Windows => "pc-windows-msvc",
            },
            match platform {
                zed::Os::Windows => "zip",
                _ => "tar.gz",
            }
        );

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {asset_name}"))?;

        let version_dir = format!("i18n-lsp-{}", release.version);
        let binary_path = format!(
            "{version_dir}/i18n-lsp{}",
            match platform {
                zed::Os::Windows => ".exe",
                _ => "",
            }
        );

        if !std::fs::metadata(&binary_path).is_ok() {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            let file_type = match platform {
                zed::Os::Windows => zed::DownloadedFileType::Zip,
                _ => zed::DownloadedFileType::GzipTar,
            };

            zed::download_file(&asset.download_url, &version_dir, file_type)?;

            zed::make_file_executable(&binary_path)?;
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

zed::register_extension!(I18nExtension);
