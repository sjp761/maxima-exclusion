use anyhow::{Result, bail};
use lazy_static::lazy_static;
use regex::Regex;
use reqwest::StatusCode;
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use ureq::OrAnyStatus;

use crate::core::auth::execute_auth_exchange;
use crate::core::endpoints::API_PROXY_NOVAFUSION_LICENSES;

lazy_static! {
    static ref EMAIL_PATTERN: Regex = Regex::new(r"^([a-z0-9_+]([a-z0-9_+.]*[a-z0-9_+])?)@([a-z0-9]+([\-\.]{1}[a-z0-9]+)*\.[a-z]{2,6})").unwrap();
}

pub async fn begin_oauth_login_flow() -> Result<Option<String>> {
    // Hardcoded for now, need to figure out where pc_sign comes from. All I know for now is it identifies a device for 2fa.
    open::that("https://accounts.ea.com/connect/auth?response_type=token&client_id=JUNO_PC_CLIENT&pc_sign=eyJhdiI6InYxIiwiYnNuIjoiRGVmYXVsdCBzdHJpbmciLCJnaWQiOjc5NDQsImhzbiI6IkFBMDAwMDAwMDAwMDAwMDAxMjc3IiwibWFjIjoiJGI0MmU5OTRjNTBhZiIsIm1pZCI6IjUyODUwNDMyMDkxOTEyODgwNDMiLCJtc24iOiJEZWZhdWx0IHN0cmluZyIsInN2IjoidjIiLCJ0cyI6IjIwMjMtMi0xMiAxMzo0NTozNjo5MzcifQ.c__XyfI01HjScx1yJ4JpZWklwMO9qn4iC9OQ5oJFE3A")?;
    let listener = TcpListener::bind("127.0.0.1:31033").await?;

    loop {
        let (mut socket, _) = listener.accept().await?;

        let (read, _) = socket.split();
        let mut reader = BufReader::new(read);

        let mut line = String::new();
        reader.read_line(&mut line).await?;

        if line.starts_with("GET /auth") {
            let query_string = line
                .split_once("?")
                .map(|(_, qs)| qs.trim())
                .map(querystring::querify)
                .unwrap();

            for query in query_string {
                if query.0 == "access_token" {
                    return Ok(Some(query.1.to_string()));
                }
            }

            return Ok(None);
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct NovaLoginValue {
    #[serde(rename = "@value")]
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NovaLoginErrorCode {
    InvalidPassword,
    ValidationFailed
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NovaLoginError {
    #[serde(rename = "@code")]
    pub code: NovaLoginErrorCode,
    pub auth_token: Option<NovaLoginValue>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct NovaLoginResponse {
    pub error: NovaLoginError,
}

// Use the OOA API to retrieve an access token without a captcha
pub async fn manual_login(persona: &str, password: &str) -> Result<String> {
    let mut query = Vec::new();
    query.push(("contentId", "1"));

    if EMAIL_PATTERN.is_match(persona) {
        query.push(("ea_email", persona));
    } else {
        query.push(("ea_persona", persona));
    }

    query.push(("ea_password", password));

    let res = ureq::get(API_PROXY_NOVAFUSION_LICENSES)
        .query_pairs(query)
        .call()
        .or_any_status()?;
    if res.status() != StatusCode::CONFLICT {
        bail!("License API did not acknowledge login request properly");
    }

    let error: NovaLoginError = quick_xml::de::from_str(&res.into_string()?).unwrap();
    if error.code != NovaLoginErrorCode::ValidationFailed {
        bail!("{:?}", error.code);
    }

    let token = error.auth_token.unwrap().value;
    execute_auth_exchange(&token, "JUNO_PC_CLIENT", "token").await
}
