#![allow(non_snake_case)]

use std::path::{Path, PathBuf};

use crate::{
    core::manifest::{ManifestError, bytes_to_string},
    util::native::platform_path,
};
use derive_getters::Getters;
use serde::Deserialize;

#[derive(Default, Debug, Clone, Deserialize, PartialEq)]
pub struct DiPContentIDs {
    #[serde(rename = "contentID", default)]
    pub items: Vec<String>,
}

#[derive(Default, Debug, Clone, Deserialize, PartialEq)]
pub struct DiPUninstall {
    pub path: String,
}

#[derive(Default, Debug, Clone, Deserialize, PartialEq)]
pub struct DiPInstallManifest {
    #[serde(rename = "filePath")]
    pub file_path: String,
}

/// <gameTitle locale="en_US">SimCity 3000 Unlimited</gameTitle>
#[derive(Default, Debug, Clone, Deserialize, PartialEq)]
pub struct DiPGameTitle {
    #[serde(rename = "@locale")]
    pub locale: String,
    #[serde(rename = "$text")]
    pub title: String,
}

#[derive(Default, Debug, Clone, Deserialize, PartialEq)]
pub struct DiPGameTitles {
    #[serde(rename = "gameTitle", default)]
    pub items: Vec<DiPGameTitle>,
}

/// <name locale="en_US">SimCity 3000 Unlimited Launcher</name>
#[derive(Default, Debug, Clone, Deserialize, PartialEq)]
pub struct DiPName {
    #[serde(rename = "@locale")]
    pub locale: String,
    #[serde(rename = "$text")]
    pub value: String,
}

macro_rules! dip_type {
    (
        $(#[$message_attr:meta])*
        $message_name:ident;
        attr {
            $(
                $(#[$attr_field_attr:meta])*
                $attr_field:ident: $attr_field_type:ty
            ),* $(,)?
        },
        data {
            $(
                $(#[$field_attr:meta])*
                $field:ident: $field_type:ty
            ),* $(,)?
        }
    ) => {
        pastey::paste! {
            $(#[$message_attr])*
            #[derive(Default, Debug, Clone, Deserialize, PartialEq, Getters)]
            pub struct [<DiP $message_name>] {
                $(
                    $(#[$attr_field_attr])*
                    #[serde(rename = "@" $attr_field)]
                    pub [<attr_ $attr_field>]: $attr_field_type,
                )*
                $(
                    $(#[$field_attr])*
                    pub $field: $field_type,
                )*
            }
        }
    }
}

dip_type!(
    FeatureFlags;
    attr {
        allowMultipleInstances: String,
        autoUpdateEnabled: String,
        dynamicContentSupportEnabled: String,
        enableDifferentialUpdate: String,
        enableOriginInGameAPI: String,
        #[serde(default)]
        enableVersionUpdate: String,
        forceTouchupInstallerAfterUpdate: String,
        languageChangeSupportEnabled: String,
        treatUpdatesAsMandatory: String,
        useGameVersionFromManifest: String,
    },
    data {}
);

dip_type!(
    GameVersion;
    attr {
        version: String,
    },
    data {}
);

dip_type!(
    Requirements;
    attr {
        #[serde(default)]
        clientVersion: String,
        osMinVersion: String,
        osReqs64Bit: String,
    },
    data {}
);

dip_type!(
    BuildMetaData;
    attr {},
    data {
        featureFlags: DiPFeatureFlags,
        gameVersion: DiPGameVersion,
        requirements: DiPRequirements,
    }
);

dip_type!(
    Launcher;
    attr {
        uid: String,
    },
    data {
        #[serde(default)]
        name: Vec<DiPName>,
        filePath: String,
        #[serde(default)]
        parameters: Option<String>,
        #[serde(default)]
        requires64BitOS: String,
        #[serde(default)]
        trial: String,
        #[serde(default)]
        requiresMetal: String,
    }
);

dip_type!(
    Runtime;
    attr {},
    data {
        #[serde(default)]
        launcher: Vec<DiPLauncher>,
    }
);

dip_type!(
    Touchup;
    attr {},
    data {
        filePath: String,
        parameters: String,
        #[serde(default)]
        updateParameters: String,
        #[serde(default)]
        repairParameters: String,
    }
);

dip_type!(
    Manifest;
    attr {
        version: String,
    },
    data {
        buildMetaData: DiPBuildMetaData,
        #[serde(rename = "contentIDs", default)]
        contentIds: DiPContentIDs,
        #[serde(default)]
        gameTitles: DiPGameTitles,
        #[serde(default)]
        uninstall: DiPUninstall,
        runtime: DiPRuntime,
        touchup: DiPTouchup,
        #[serde(default)]
        installManifest: DiPInstallManifest,
    }
);

dip_type!(
    LegacyManifest;
    attr {},
    data {
        executable: DiPTouchup,
    }
);

fn remove_leading_slash(path: &str) -> &str {
    path.strip_prefix('/').unwrap_or(path)
}

fn remove_trailing_slash(path: &str) -> &str {
    path.strip_suffix('/').unwrap_or(path)
}

fn remove_trailing_backslash(path: &str) -> &str {
    path.strip_suffix('\\').unwrap_or(path)
}

fn split_args(s: &str) -> Result<Vec<String>, ManifestError> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in s.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ' ' if !in_quotes => {
                if !current.is_empty() {
                    args.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if in_quotes {
        return Err(ManifestError::Decode);
    }

    if !current.is_empty() {
        args.push(current);
    }

    Ok(args)
}

impl DiPTouchup {
    pub fn path(&self) -> &str {
        remove_leading_slash(&self.filePath)
    }
}

impl DiPLauncher {
    pub fn is_trial(&self) -> bool {
        self.trial == "1" || self.trial.eq_ignore_ascii_case("true")
    }

    pub fn display_name(&self, locale: &str) -> Option<&str> {
        self.name
            .iter()
            .find(|n| n.locale == locale)
            .or_else(|| self.name.iter().find(|n| n.locale == "en_US"))
            .map(|n| n.value.as_str())
    }
}

impl DiPManifest {
    pub async fn read(path: &Path) -> Result<Self, ManifestError> {
        let bytes = tokio::fs::read(path).await?;
        let string = bytes_to_string(bytes).ok_or(ManifestError::Decode)?;

        Ok(quick_xml::de::from_str(&string)?)
    }

    pub fn version(&self) -> &str {
        &self.buildMetaData.gameVersion.attr_version
    }

    pub fn locale(&self) -> &str {
        self.gameTitles
            .items
            .iter()
            .find(|t| t.locale == "en_US")
            .map(|t| t.locale.as_str())
            .or_else(|| self.gameTitles.items.first().map(|t| t.locale.as_str()))
            .unwrap_or("en_US")
    }

    pub fn execute_path(&self, trial: bool) -> Option<&str> {
        self.runtime
            .launcher
            .iter()
            .find(|l| l.is_trial() == trial)
            .map(|l| l.filePath.as_str())
    }

    fn collect_touchup_args(
        &self,
        install_path: &Path,
        locale: &str,
    ) -> Result<Vec<String>, ManifestError> {
        let install_str = platform_path(
            remove_trailing_backslash(install_path.to_str().ok_or(ManifestError::Decode)?)
                .replace('/', "\\"),
        )
        .to_str()
        .ok_or(ManifestError::Decode)?
        .to_string();

        let expanded = self
            .touchup
            .parameters
            .replace("{locale}", locale)
            .replace("{installLocation}", &install_str);

        split_args(&expanded)
    }

    #[cfg(unix)]
    pub async fn run_touchup(
        &self,
        install_path: &PathBuf,
        wine_prefix_path: Option<PathBuf>,
    ) -> Result<(), ManifestError> {
        use crate::unix::{
            fs::case_insensitive_path,
            wine::{CommandType, invalidate_mx_wine_registry, run_wine_command},
        };

        let install_path = PathBuf::from(remove_trailing_slash(
            install_path.to_str().ok_or(ManifestError::Decode)?,
        ));
        let args = self.collect_touchup_args(&install_path, self.locale())?;
        let path = install_path.join(&self.touchup.path());
        let path = case_insensitive_path(path).to_string_lossy().to_string();
        run_wine_command(
            path.into(),
            Some(args),
            None,
            true,
            CommandType::Run,
            &wine_prefix_path.unwrap(),
        )
        .await?;
        invalidate_mx_wine_registry().await;
        Ok(())
    }

    #[cfg(windows)]
    pub async fn run_touchup(
        &self,
        install_path: &PathBuf,
        _wine_prefix_path: Option<PathBuf>,
    ) -> Result<(), ManifestError> {
        use crate::util::native::NativeError;
        use tokio::process::Command;

        let args = self.collect_touchup_args(install_path, self.locale())?;
        let path = install_path.join(self.touchup.path());

        let status = Command::new(&path).args(&args).spawn()?.wait().await?;

        if !status.success() {
            return Err(ManifestError::Native(NativeError::Command(
                status.code().unwrap_or(0),
            )));
        }

        Ok(())
    }
}
