// Handles different languages for the UI

use log::error;
use serde::Deserialize;
use serde_json;
use std::fs;
use sys_locale;

use crate::FrontendLanguage;

#[derive(Deserialize)]
pub struct LocalizedStrings {
    pub errors: LocalizedGenericErrors,
    pub startup_flow: LocalizedStartupFlow,
    pub menubar: LocalizedMenubar,
    pub profile_menu: LocalizedProfileMenu,
    pub games_view: LocalizedGamesView,
    pub friends_view: LocalizedFriendsView,
    pub settings_view: LocalizedSettingsView,
    pub locale: LocalizedLocaleInfo,
    pub modals: LocalizedModals,
}

#[derive(Deserialize)]
pub struct LocalizedModals {
    pub close: String,
    pub game_install: LocalizedGameInstallModal,
    pub game_settings: LocalizedGameSettingsModal,
}

#[derive(Deserialize)]
pub struct LocalizedGameInstallModal {
    pub header: String,
    pub locate_installed: String,
    pub locate_action: String,
    pub locate_in_progress: String,
    pub locate_failed: String,
    pub fresh_download: String,
    pub fresh_path_confirmation: String,
    pub fresh_path_invalid: String,
    pub fresh_action: String,
    
}

#[derive(Deserialize)]
pub struct LocalizedGameSettingsModal {
    pub header: String,
    pub not_installed: String,
    pub cloud_saves: String,
    pub launch_arguments: String,
    pub executable_override: String,
    pub uninstall: String,
}

#[derive(Deserialize)]
pub struct LocalizedLocaleInfo {
    pub default: String,
    pub en_us: String,
}

#[derive(Deserialize)]
pub struct LocalizedStartupFlow {
    pub starting: String,
    pub logging_in: String,
    pub login_header: String,
    pub login_button: String,
    pub service_installer_header: String,
    pub service_installer_description: String,
    pub service_installer_button: String
}

#[derive(Deserialize)]
pub struct LocalizedGenericErrors {
    pub view_not_impl: String,
    pub view_coming_soon: String,
    pub critical_thread_crashed: String,
}

#[derive(Deserialize)]
pub struct LocalizedMenubar {
    pub games: String,
    pub store: String,
    pub friends: String,
    pub settings: String,
    pub downloads: String,
}

#[derive(Deserialize)]
pub struct LocalizedProfileMenu {
    pub view_profile: String,
    pub view_wishlist: String,
}

#[derive(Deserialize)]
pub struct LocalizedGamesView {
    pub toolbar: LocalizedGamesViewToolbar,
    pub main: LocalizedGamesViewMain,
    pub details: LocalizedGamesViewDetails,
}

#[derive(Deserialize)]
pub struct LocalizedSettingsView {
    pub interface: LocalizedInterfaceSettings,
    pub game_installation: LocalizedGameInstallationSettings,
}

#[derive(Deserialize)]
pub struct LocalizedInterfaceSettings {
    pub header: String,
    pub language: String,
}

#[derive(Deserialize)]
pub struct LocalizedGameInstallationSettings {
    pub header: String,
    pub default_folder: String,
}

#[derive(Deserialize)]
pub struct LocalizedGamesViewToolbar {
    pub genre_filter: String,
    pub genre_options: LocalizedGamesViewToolbarGenreOptions,
    pub platform_filter: String,
    pub platform_options: LocalizedGamesViewToolbarPlatformOptions,
    pub search_bar_hint: String,
    pub running_suffix: String,
}

#[derive(Deserialize)]
pub struct LocalizedGamesViewToolbarGenreOptions {
    pub all: String,
    pub shooter: String,
    pub simulation: String,
}

#[derive(Deserialize)]
pub struct LocalizedGamesViewToolbarPlatformOptions {
    pub all: String,
    pub windows: String,
    pub mac: String,
}

#[derive(Deserialize)]
pub struct LocalizedGamesViewMain {
    pub play: String,
    pub stop: String,
    pub install: String,
    pub uninstall: String,
    pub pause: String,
    pub resume: String,
    pub settings: String,
    pub playtime: String,
    pub achievements: String,
    pub no_loaded_games: String,
}

#[derive(Deserialize)]
pub struct LocalizedGamesViewDetails {
    pub min_system_req: String,
    pub rec_system_req: String,
}

#[derive(Deserialize)]
pub struct LocalizedFriendsView {
    pub toolbar: LocalizedFriendsViewToolbar,
    pub friend_actions: LocalizedFriendsViewFriendActions,
    pub status: LocalizedFriendsViewStatus,
}

#[derive(Deserialize)]
pub struct LocalizedFriendsViewToolbar {
    pub online: String,
    pub all: String,
    pub pending: String,
    pub blocked: String,
    pub add_friend: String,
    pub filter_options: LocalizedFriendsViewToolbarSearchFilterOptions,
    pub search_hint: String,
}

#[derive(Deserialize)]
pub struct LocalizedFriendsViewFriendActions {
    pub profile: String,
    pub chat: String,
    pub unfriend: String,
}

#[derive(Deserialize)]
pub struct LocalizedFriendsViewStatus {
    pub unknown: String,
    pub do_not_disturb: String,
    pub away: String,
    pub online: String,
    pub offline: String,
    pub prepend: bool,
    pub playing: String,
}

#[derive(Deserialize)]
pub struct LocalizedFriendsViewToolbarSearchFilterOptions {
    pub name: String,
    pub game: String,
}

pub struct TranslationManager {
    pub localization: LocalizedStrings,
}

macro_rules! language_include_matcher {
    (
        $match_var:expr, $fallback_var:expr;
        $($name:expr => $file:expr),* $(,)?
    ) => {
        match $match_var {
            $(
                $name => include_str!(concat!("../res/locale/", $file, ".json")),
            )*
            _ => $fallback_var,
        }
    };
}

/// shorthand for .replace(), sean thinks it's cleaner and won't shut up about it
macro_rules! positional_replace {
    ($key:expr, $($name:expr, $value:expr),*) => {
        {
            let mut temp_string = $key.to_string();
            $(
                temp_string = temp_string.replace(concat!("{", $name, "}"), &$value.to_string());
            )*
            temp_string
        }
    };
}

pub(crate) use positional_replace;

impl TranslationManager {
    /// Gets an instance of LocalizedStrings for the specified locale code
    pub fn get_for_locale(code: &str) -> LocalizedStrings {
        let english = include_str!("../res/locale/en_us.json").to_owned();
        let locale_json: &str = language_include_matcher!(code, &english;
            "en-US" => "en_us",
            //"de-bug" => "de_bug"
        );

        serde_json::from_str(locale_json).unwrap()
    }
    
    pub fn new(lang: &FrontendLanguage) -> Self {
        let locale = match lang {
            FrontendLanguage::SystemDefault => {
                let locale: Option<String> = sys_locale::get_locale();
                if let Some(code) = locale { code } else { "en-US".to_owned() }
            },
            FrontendLanguage::EnUS => "en-US".to_owned(),
        };
        
        let s = Self::get_for_locale(locale.as_str());
        Self { localization: s }
    }

    pub fn new_with(code: &str) -> Self {
        Self { localization: Self::get_for_locale(code) }
    }
}