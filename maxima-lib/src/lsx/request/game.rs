const LANGUAGES: &str =
    "ar_SA,de_DE,en_US,es_ES,es_MX,fr_FR,it_IT,ja_JP,ko_KR,pl_PL,pt_BR,ru_RU,zh_CN,zh_TW";
//const LANGUAGES: &str = "de_DE,en_US,es_ES,es_MX,fr_FR,it_IT,ja_JP,pl_PL,pt_BR,ru_RU,zh_TW";
//const LANGUAGES: &str = "en_US,es_ES,fr_FR,pt_BR";

use crate::{
    lsx::{
        connection::ConnectionState,
        request::LSXRequestError,
        types::{
            LSXGameInfoId, LSXGetAllGameInfo, LSXGetAllGameInfoResponse, LSXGetGameInfo,
            LSXGetGameInfoResponse, LSXResponseType,
        },
    },
    make_lsx_handler_response,
};

pub async fn handle_game_info_request(
    _: &mut ConnectionState,
    request: LSXGetGameInfo,
) -> Result<Option<LSXResponseType>, LSXRequestError> {
    let game_info = match request.attr_GameInfoId {
        LSXGameInfoId::FreeTrial => "false".to_string(),
        LSXGameInfoId::Languages => LANGUAGES.to_string(),
        LSXGameInfoId::InstalledLanguage => "en_US".to_string(),
    };

    make_lsx_handler_response!(Response, GetGameInfoResponse, { attr_GameInfo: game_info })
}

// <GetAllGameInfoResponse FullGamePurchased="true" FullGameReleased="true" InstalledVersion="0" MaxGroupSize="16" Languages="ar_SA,de_DE,en_US,es_ES,es_MX,fr_FR,it_IT,ja_JP,ko_KR,pl_PL,pt_BR,ru_RU,zh_CN,zh_TW" Expiration="0000-00-00T00:00:00" UpToDate="true" HasExpiration="false" InstalledLanguage="" EntitlementSource="STEAM" FullGameReleaseDate="2020-10-22T09:00:00" AvailableVersion="1.0.64.43203" DisplayName="Battlefield V Definitive Edition" FreeTrial="false" SystemTime="2023-06-23T04:22:10"/>

pub async fn handle_all_game_info_request(
    state: &mut ConnectionState,
    _request: LSXGetAllGameInfo,
) -> Result<Option<LSXResponseType>, LSXRequestError> {
    let current_offer = {
        let maxima = state.maxima().lock().await;

        maxima
            .playing()
            .as_ref()
            .and_then(|context| context.current_offer())
    };

    let (display_name, available_version, installed_version, full_game_release_date) =
        match current_offer.as_ref() {
            Some(owned_offer) => {
                let display_name = owned_offer.offer().display_name().to_string();
                let available_version = owned_offer
                    .offer()
                    .downloads()
                    .first()
                    .and_then(|download| download.game_version().clone())
                    .unwrap_or_else(|| "0".to_string());
                let installed_version = owned_offer
                    .installed_version()
                    .await
                    .ok()
                    .unwrap_or_else(|| "0".to_string());
                let full_game_release_date = owned_offer
                    .offer()
                    .downloads()
                    .first()
                    .map(|download| download.build_live_date().clone())
                    .unwrap_or_else(|| "0000-00-00T00:00:00".to_string());

                (
                    display_name,
                    available_version,
                    installed_version,
                    full_game_release_date,
                )
            }
            None => (
                "Unknown Game".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0000-00-00T00:00:00".to_string(),
            ),
        };

    make_lsx_handler_response!(Response, GetAllGameInfoResponse, {
        attr_FullGamePurchased: true,
        attr_FullGameReleased: true,
        attr_InstalledVersion: installed_version,
        attr_MaxGroupSize: 16,
        attr_Languages: LANGUAGES.to_string(),
        attr_Expiration: "0000-00-00T00:00:00".to_string(),
        attr_UpToDate: true,
        attr_HasExpiration: false,
        attr_EntitlementSource: "STEAM".to_string(),
        attr_AvailableVersion: available_version,
        attr_DisplayName: display_name,
        attr_FreeTrial: false,
        attr_InstalledLanguage: "en_US".to_string(),
        attr_FullGameReleaseDate: full_game_release_date,
        attr_SystemTime: "2023-06-22T04:00:00".to_string()
    })
}
