use crate::{
    lsx::{
        connection::ConnectionState,
        request::LSXRequestError,
        types::{LSXGetVoipStatus, LSXGetVoipStatusResponse, LSXResponseType},
    },
    make_lsx_handler_response,
};

pub async fn handle_voip_status_request(
    _: &mut ConnectionState,
    _: LSXGetVoipStatus,
) -> Result<Option<LSXResponseType>, LSXRequestError> {
    make_lsx_handler_response!(Response, GetVoipStatusResponse, { attr_Available: false, attr_Active: false })
}
