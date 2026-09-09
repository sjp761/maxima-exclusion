use tokio::sync::mpsc::{self, Receiver, Sender};

use anyhow::{Result, bail};
use log::info;
use maxima::core::{
    LockedMaxima, Maxima, MaximaOptionsBuilder,
    auth::{
        TokenResponse, context::AuthContext, login::begin_oauth_login_flow, nucleus_token_exchange,
    },
};

pub struct InteractThreadLoginResponse {
    pub success: bool,
    pub name: String,
}

pub enum MaximaLibRequest {
    LoginRequest,
    GetGamesRequest,
    GetFriendsRequest,
    GetUserAvatarRequest(String, String),
    GetGameImagesRequest(String),
    GetGameDetailsRequest(String),
    StartGameRequest(String, bool),
    ShutdownRequest,
}

pub enum MaximaLibResponse {
    LoginResponse(InteractThreadLoginResponse),
    LoginCacheEmpty,
    GameInfoResponse(),
    FriendInfoResponse(),
    UserAvatarResponse(),
    GameDetailsResponse(),
    GameUIImagesResponse(),
    InteractionThreadDiedResponse,
}

pub struct BridgeThread {
    pub rx: Receiver<MaximaLibResponse>,
    pub tx: Sender<MaximaLibRequest>,
}

impl BridgeThread {
    pub fn new() -> Self {
        let (tx0, rx1) = mpsc::channel(1);
        let (tx1, rx0) = mpsc::channel(1);

        tokio::task::spawn(async move {
            let die_fallback = tx1.clone();

            if let Err(err) = BridgeThread::run(rx1, tx1).await {
                let _ = die_fallback.send(MaximaLibResponse::InteractionThreadDiedResponse);
                panic!("Interact thread failed! {err}");
            }

            info!("Interact thread shut down");
        });

        Self { rx: rx0, tx: tx0 }
    }

    async fn run(
        mut rx1: Receiver<MaximaLibRequest>,
        tx1: Sender<MaximaLibResponse>,
    ) -> Result<()> {
        let maxima_arc: LockedMaxima = Maxima::new_with_options(
            MaximaOptionsBuilder::default()
                .dummy_local_user(false)
                .load_auth_storage(true)
                .build()?,
        )
        .await?;

        {
            let maxima = maxima_arc.lock().await;

            let Ok(()) = maxima.start_lsx(maxima_arc.clone()).await else {
                info!("LSX failed to start!");
                return Ok(());
            };
            info!("LSX started");

            let logged_in = {
                let mut auth_storage = maxima.auth_storage().lock().await;
                auth_storage.logged_in().await?
            };

            if logged_in {
                let user = maxima.local_user().await?;
                let message = MaximaLibResponse::LoginResponse(InteractThreadLoginResponse {
                    success: true,
                    name: user.player().as_ref().unwrap().display_name().to_owned(),
                });
                tx1.send(message).await.ok();
            } else {
                tx1.send(MaximaLibResponse::LoginCacheEmpty).await.ok();
            }
        }

        'main: loop {
            let Ok(request) = rx1.try_recv() else {
                continue;
            };

            let _result: Result<()> = match request {
                MaximaLibRequest::LoginRequest => {
                    let login = async || {
                        let maxima = maxima_arc.lock().await;
                        let mut auth_storage = maxima.auth_storage().lock().await;

                        if !auth_storage.logged_in().await? {
                            let res = login_flow().await?;
                            auth_storage.add_account(&res).await?;
                        }

                        tx1.send(MaximaLibResponse::LoginResponse(
                            InteractThreadLoginResponse {
                                success: true,
                                name: maxima
                                    .local_user()
                                    .await?
                                    .player()
                                    .as_ref()
                                    .unwrap()
                                    .display_name()
                                    .to_owned(),
                            },
                        ))
                        .await
                        .ok();

                        Ok(())
                    };
                    login().await
                }

                MaximaLibRequest::GetGamesRequest => {
                    todo!("GetGamesRequest")
                }
                MaximaLibRequest::GetFriendsRequest => {
                    todo!("GetFriendsRequest")
                }
                MaximaLibRequest::GetGameImagesRequest(_) => {
                    todo!("GetGameImagesRequest")
                }
                MaximaLibRequest::GetUserAvatarRequest(_, _) => {
                    todo!("GetUserAvatarRequest")
                }
                MaximaLibRequest::GetGameDetailsRequest(_) => {
                    todo!("GetGameDetailsRequest")
                }
                MaximaLibRequest::StartGameRequest(_, _) => {
                    todo!("StartGameRequest")
                }

                MaximaLibRequest::ShutdownRequest => break 'main Ok(()),
            };
        }
    }
}

pub async fn login_flow() -> Result<TokenResponse> {
    let mut auth_context = AuthContext::new()?;
    begin_oauth_login_flow(&mut auth_context).await?;

    if auth_context.code().is_none() {
        bail!("Login failed!");
    }

    info!("Received login...");

    let token_res = nucleus_token_exchange(&auth_context).await;
    if token_res.is_err() {
        bail!("Login failed: {}", token_res.err().unwrap());
    }

    Ok(token_res?)
}
