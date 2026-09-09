use log::{debug, info};
use std::env;

use crate::{
    core::{auth::hardware::HardwareInfo, launch::LaunchMode},
    lsx::{
        connection::ConnectionState,
        request::LSXRequestError,
        types::{LSXRequestLicense, LSXRequestLicenseResponse, LSXResponseType},
    },
    make_lsx_handler_response,
    ooa::{LicenseAuth, request_license},
};

pub async fn handle_license_request(
    state: &mut ConnectionState,
    request: LSXRequestLicense,
) -> Result<Option<LSXResponseType>, LSXRequestError> {
    info!("Requesting OOA License and Denuvo Token");

    if let Ok(token) = env::var("MAXIMA_DENUVO_TOKEN") {
        return make_lsx_handler_response!(Response, RequestLicenseResponse, { attr_License: token.to_owned() });
    }

    let mut maxima = state.maxima().lock().await;

    let playing = maxima.playing().as_ref().unwrap();
    let content_id = playing.content_id().to_owned();
    let mode = playing.mode();
    let slug = playing.slug().clone();

    let auth = match mode {
        LaunchMode::Offline(_) => {
            return make_lsx_handler_response!(Response, RequestLicenseResponse, { attr_License: String::new() });
        }
        LaunchMode::Online(_) => LicenseAuth::AccessToken(maxima.access_token().await?),
        LaunchMode::OnlineOffline(_, persona, password) => {
            LicenseAuth::Direct(persona.to_owned(), password.to_owned())
        }
    };

    // TODO: how to get version
    let hw_info = HardwareInfo::new(2, slug.as_deref());
    let license = request_license(
        &content_id,
        &hw_info.generate_hardware_hash(),
        &auth,
        Some(request.attr_RequestTicket.as_str()),
        Some(request.attr_TicketEngine.as_str()),
    )
    .await?;

    if license.game_token.is_none() {
        return Err(LSXRequestError::Denuvo);
    }

    info!("Successfully retrieved license tokens");

    let token = license.game_token.as_ref().unwrap();

    debug!("Got Denuvo Token: {}", token);

    make_lsx_handler_response!(Response, RequestLicenseResponse, { attr_License: token.to_owned() })
}
