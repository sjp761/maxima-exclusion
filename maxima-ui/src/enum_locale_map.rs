use crate::{translation_manager::{LocalizedLocaleInfo, TranslationManager}, views::{friends_view::{FriendsViewBarPage, FriendsViewBarStatusFilter}, game_view::{GameViewBarGenre, GameViewBarPlatform}}, FrontendLanguage};

pub trait EnumToString<T> {
    fn get_string(&self, variant: &mut T) -> &str;
    fn get_string_nonmut(&self, variant: &T) -> &str;
}

impl EnumToString<FriendsViewBarPage> for TranslationManager {
    fn get_string_nonmut(&self, variant: &FriendsViewBarPage) -> &str {
        match variant {
            FriendsViewBarPage::Online => &self.localization.friends_view.toolbar.online,
            FriendsViewBarPage::All => &self.localization.friends_view.toolbar.all,
            FriendsViewBarPage::Pending => &self.localization.friends_view.toolbar.pending,
            FriendsViewBarPage::Blocked => &self.localization.friends_view.toolbar.blocked,
        }
    }
    fn get_string(&self, variant: &mut FriendsViewBarPage) -> &str {
        self.get_string_nonmut(variant)
    }
}

impl EnumToString<FriendsViewBarStatusFilter> for TranslationManager {
    fn get_string_nonmut(&self, variant: &FriendsViewBarStatusFilter) -> &str {
        match variant {
            FriendsViewBarStatusFilter::Name => &self.localization.friends_view.toolbar.filter_options.name,
            FriendsViewBarStatusFilter::Game => &self.localization.friends_view.toolbar.filter_options.game,
        }
    }
    fn get_string(&self, variant: &mut FriendsViewBarStatusFilter) -> &str {
        self.get_string_nonmut(variant)
    }
}

impl EnumToString<GameViewBarGenre> for TranslationManager {
    fn get_string_nonmut(&self, variant: &GameViewBarGenre) -> &str {
        match variant {
            GameViewBarGenre::AllGames => &self.localization.games_view.toolbar.genre_options.all,
            GameViewBarGenre::Shooters => &self.localization.games_view.toolbar.genre_options.shooter,
            GameViewBarGenre::Simulation => &self.localization.games_view.toolbar.genre_options.simulation,
        }
    }
    fn get_string(&self, variant: &mut GameViewBarGenre) -> &str {
        self.get_string_nonmut(variant)
    }
}

impl EnumToString<GameViewBarPlatform> for TranslationManager {
    fn get_string_nonmut(&self, variant: &GameViewBarPlatform) -> &str {
        match variant {
            GameViewBarPlatform::AllPlatforms => &self.localization.games_view.toolbar.platform_options.all,
            GameViewBarPlatform::Windows => &self.localization.games_view.toolbar.platform_options.windows,
            GameViewBarPlatform::Mac => &self.localization.games_view.toolbar.platform_options.mac,
        }
    }
    fn get_string(&self, variant: &mut GameViewBarPlatform) -> &str {
        self.get_string_nonmut(variant)
    }
}

impl EnumToString<FrontendLanguage> for TranslationManager {
    fn get_string_nonmut(&self, variant: &FrontendLanguage) -> &str {
        match variant {
            FrontendLanguage::SystemDefault => &self.localization.locale.default,
            FrontendLanguage::EnUS => &self.localization.locale.en_us,
        }
    }
    fn get_string(&self, variant: &mut FrontendLanguage) -> &str {
        self.get_string_nonmut(variant)
    }
}
