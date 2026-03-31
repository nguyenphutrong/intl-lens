use serde::{Deserialize, Serialize};
use serde_json::json;
use zed_extension_api::{
    self as zed, settings::ExtensionSettings, LanguageServerId, Result, Worktree,
};

const EXTENSION_ID: &str = "intl-lens";

struct IntlLensExtension {
    cached_binary_path: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct IntlLensSettings {
    #[serde(rename = "binaryPath")]
    binary_path: Option<String>,
    locale_paths: Option<Vec<String>>,
    source_locale: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct IntlLensServerSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    locale_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_locale: Option<String>,
}

impl zed::Extension for IntlLensExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn settings_contribution(&mut self) -> Option<zed::ExtensionSettingsContribution> {
        Some(zed::ExtensionSettingsContribution {
            settings_schema: json!({
                "type": "object",
                "properties": {
                    "localePaths": {
                        "type": "array",
                        "description": "Override the locale file paths used by Intl Lens for this worktree.",
                        "items": {
                            "type": "string"
                        }
                    },
                    "sourceLocale": {
                        "type": "string",
                        "description": "Override the source locale used for hover previews, inlay hints, and completions. Leave empty to use the project config."
                    }
                },
                "additionalProperties": false
            }),
            default_settings: json!({
                "localePaths": [],
                "sourceLocale": ""
            }),
        })
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

    fn language_server_initialization_options(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<serde_json::Value>> {
        Ok(Some(Self::load_settings(worktree).server_settings_json()))
    }

    fn language_server_workspace_configuration(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<serde_json::Value>> {
        Ok(Some(Self::load_settings(worktree).server_settings_json()))
    }
}

impl IntlLensExtension {
    fn load_settings(worktree: &Worktree) -> IntlLensSettings {
        match ExtensionSettings::for_worktree(EXTENSION_ID, worktree) {
            Ok(settings) => settings,
            Err(err) => {
                eprintln!("Failed to load Intl Lens settings for {EXTENSION_ID}: {err}");
                IntlLensSettings::default()
            }
        }
    }

    fn get_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<String> {
        let settings = Self::load_settings(worktree);

        if let Some(path) = settings.normalized_binary_path() {
            if std::fs::metadata(&path).is_ok() {
                return Ok(path);
            }

            if let Some(resolved_path) = worktree.which(&path) {
                self.cached_binary_path = Some(resolved_path.clone());
                return Ok(resolved_path);
            }

            return Err(format!(
                "Configured binaryPath '{path}' does not exist or is not accessible"
            ));
        }

        if let Some(path) = worktree.which("intl-lens") {
            self.cached_binary_path = Some(path.clone());
            return Ok(path);
        }

        if let Some(path) = &self.cached_binary_path {
            if std::fs::metadata(path).is_ok() {
                return Ok(path.clone());
            }
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = zed::latest_github_release(
            "nguyenphutrong/intl-lens",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (platform, arch) = zed::current_platform();
        let asset_name = format!(
            "intl-lens-{}-{}.{}",
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

        let version_dir = format!("intl-lens-{}", release.version);
        let binary_path = format!(
            "{version_dir}/intl-lens{}",
            match platform {
                zed::Os::Windows => ".exe",
                _ => "",
            }
        );

        if std::fs::metadata(&binary_path).is_err() {
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

impl IntlLensSettings {
    fn normalized_binary_path(&self) -> Option<String> {
        self.binary_path
            .as_deref()
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(ToOwned::to_owned)
    }

    fn normalized_server_settings(&self) -> IntlLensServerSettings {
        let locale_paths = self.locale_paths.as_ref().and_then(|paths| {
            let sanitized: Vec<String> = paths
                .iter()
                .map(|path| path.trim())
                .filter(|path| !path.is_empty())
                .map(ToOwned::to_owned)
                .collect();
            (!sanitized.is_empty()).then_some(sanitized)
        });

        let source_locale = self
            .source_locale
            .as_deref()
            .map(str::trim)
            .filter(|locale| !locale.is_empty())
            .map(ToOwned::to_owned);

        IntlLensServerSettings {
            locale_paths,
            source_locale,
        }
    }

    fn server_settings_json(&self) -> serde_json::Value {
        serde_json::to_value(self.normalized_server_settings()).unwrap_or_else(|err| {
            eprintln!("Failed to serialize Intl Lens server settings: {err}");
            json!({})
        })
    }
}

zed::register_extension!(IntlLensExtension);
