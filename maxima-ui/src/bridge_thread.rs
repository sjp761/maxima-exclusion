use egui::Context;
use log::{error, info, warn};
use maxima::core::manifest::handle_touchup_request;
use thiserror::Error;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::mpsc::unbounded_channel;

use crate::{
    GameDetails, GameInfo, GameSettings,
    bridge::{
        game_details::game_details_request, get_friends::get_friends_request,
        get_games::get_games_request, login_oauth::login_oauth, start_game::start_game_request,
    },
    event_thread::{EventThread, MaximaEventRequest, MaximaEventResponse},
    ui_image::UIImageCacheLoaderCommand,
    views::friends_view::UIFriend,
};
use maxima::{
    content::manager::{
        ContentManager, ContentManagerError, QueuedGameBuilder, QueuedGameBuilderError,
    },
    core::{
        LockedMaxima, Maxima, MaximaCreationError, MaximaEvent, MaximaOptionsBuilder,
        MaximaOptionsBuilderError,
        auth::storage::{AuthError, TokenError},
        launch::LaunchError,
        library::LibraryError,
        manifest::{self, MANIFEST_RELATIVE_PATH, ManifestError},
        service_layer::{
            ServiceGameImagesRequestBuilderError, ServiceHeroBackgroundImageRequestBuilderError,
            ServiceLayerError, ServicePlayer,
        },
    },
    gameinfo::GameInstallInfo,
    lsx::service::LSXServerError,
    rtm::RtmError,
    util::{
        native::NativeError,
        registry::{RegistryError, check_registry_validity, set_up_registry},
    },
};
use std::{path::PathBuf, time::Duration};

// TODO(headassbtw): integrate these all into the enums
pub struct InteractThreadLoginResponse {
    pub you: ServicePlayer,
}

pub struct InteractThreadGameListResponse {
    pub game: GameInfo,
    pub settings: GameSettings,
}

pub struct InteractThreadFriendListResponse {
    pub friend: UIFriend,
}

pub struct InteractThreadGameDetailsResponse {
    pub slug: String,
    pub response: GameDetails,
}

pub struct InteractThreadLocateGameFailure {
    pub reason: ManifestError,
    pub xml_path: String,
}

pub enum InteractThreadLocateGameResponse {
    Success,
    Error(InteractThreadLocateGameFailure),
}

pub struct InteractThreadDownloadProgressResponse {
    pub bytes: usize,
    pub bytes_total: usize,
}

pub enum MaximaLibRequest {
    StartService,
    LoginRequestOauth,
    GetGamesRequest,
    GetFriendsRequest,
    GetGameDetailsRequest(String),
    StartGameRequest(GameInfo, Option<GameSettings>),
    InstallGameRequest(String, String, PathBuf, Option<PathBuf>), // offer, slug, path, wine prefix (unix only)
    PauseInstallRequest(String),
    MoveInstallToTopRequest(String),
    LocateGameRequest(String, String, Option<PathBuf>), // slug, path, wine prefix (unix only)
    CancelInstallRequest(String),
    ShutdownRequest,
}

pub enum MaximaLibResponse {
    LoginResponse(Result<InteractThreadLoginResponse, anyhow::Error>),
    LoginCacheEmpty,
    ServiceNeedsStarting,
    ServiceStarted,
    GameInfoResponse(InteractThreadGameListResponse),
    FriendInfoResponse(InteractThreadFriendListResponse),
    GameDetailsResponse(InteractThreadGameDetailsResponse),
    LocateGameResponse(InteractThreadLocateGameResponse),
    // Alerts, rather than responses:
    CriticalError(Box<BackendError>),
    NonFatalError(Box<BackendError>),
    ActiveGameChanged(Option<String>),
    DownloadProgressChanged(String, InteractThreadDownloadProgressResponse),
    DownloadFinished(String),
    DownloadFailed(String, String),
    DownloadQueueUpdate(Option<String>, Vec<String>),
}
pub struct BridgeThread {
    pub backend_listener: UnboundedReceiver<MaximaLibResponse>,
    pub backend_commander: UnboundedSender<MaximaLibRequest>,
    pub rtm_listener: UnboundedReceiver<MaximaEventResponse>,
    pub rtm_commander: UnboundedSender<MaximaEventRequest>,
}

#[derive(Error, Debug)]
pub enum BackendError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    BackgroundServiceControl(#[from] maxima::util::BackgroundServiceControlError),
    #[error(transparent)]
    BackgroundServiceClient(#[from] maxima::core::error::BackgroundServiceClientError),
    #[error(transparent)]
    ContentManager(#[from] ContentManagerError),
    #[error(transparent)]
    Launch(#[from] LaunchError),
    #[error(transparent)]
    Library(#[from] LibraryError),
    #[error(transparent)]
    LSXServer(#[from] LSXServerError),
    #[error(transparent)]
    MaximaCreation(#[from] MaximaCreationError),
    #[error(transparent)]
    MaximaOptionsBuilder(#[from] MaximaOptionsBuilderError),
    #[error(transparent)]
    Native(#[from] NativeError),
    #[error(transparent)]
    QueuedGameBuilder(#[from] QueuedGameBuilderError),
    #[error(transparent)]
    RegistryError(#[from] RegistryError),
    #[error(transparent)]
    Rtm(#[from] RtmError),
    #[error(transparent)]
    ServiceGameImagesRequestBuilder(#[from] ServiceGameImagesRequestBuilderError),
    #[error(transparent)]
    ServiceHeroBackgroundImageRequestBuilder(#[from] ServiceHeroBackgroundImageRequestBuilderError),
    #[error(transparent)]
    ServiceLayer(#[from] ServiceLayerError),
    #[error(transparent)]
    Token(#[from] TokenError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),

    #[error("backend-frontend communication channel disconnected")]
    ChannelDisconnected,
    #[error("no live build available for the requested offer")]
    NoLiveBuildAvailable,
    #[error("tried to perform an action that requires being logged in, but was logged out")]
    LoggedOut,
    #[error("download failed for `{0}`: {1}")]
    DownloadFailed(String, String),
}

impl BridgeThread {
    fn update_queue(
        content_manager: &ContentManager,
        backend_responder: UnboundedSender<MaximaLibResponse>,
    ) {
        let current = if let Some(now) = content_manager.queue().current() {
            Some(now.offer_id().to_owned())
        } else {
            None
        };

        let mut queue: Vec<String> = Vec::new();

        for game in content_manager.queue().queued() {
            queue.push(game.offer_id().to_owned());
        }

        backend_responder.send(MaximaLibResponse::DownloadQueueUpdate(current, queue)).ok();
    }

    pub fn new(
        ctx: &Context,
        remote_provider_channel: UnboundedSender<UIImageCacheLoaderCommand>,
    ) -> Self {
        puffin::profile_function!();
        let (backend_commander, backend_cmd_listener) = unbounded_channel();
        let (backend_responder, backend_listener) = unbounded_channel();
        let (rtm_commander, rtm_cmd_listener) = unbounded_channel();
        let (rtm_responder, rtm_listener) = unbounded_channel();
        let context = ctx.clone();

        let backend_responder_for_task = backend_responder.clone();
        let remote_provider_for_task = remote_provider_channel.clone();

        // you dare spawn a thread without catching my sneaky sneaky panics mister Potter ??
        tokio::task::spawn(async move {
            let die_fallback = backend_responder_for_task.clone();
            match BridgeThread::run(
                backend_cmd_listener,
                backend_responder_for_task,
                rtm_cmd_listener,
                rtm_responder,
                remote_provider_for_task,
                &context,
            )
            .await
            {
                Ok(()) => (),
                Err(err) => {
                    error!("BridgeThread task: run() returned Err: {}", err);
                    let _ = die_fallback.send(MaximaLibResponse::NonFatalError(Box::from(err)));
                }
            }

            info!("Interact thread shut down");
        });

        Self {
            backend_listener,
            backend_commander,
            rtm_listener,
            rtm_commander,
        }
    }

    async fn run(
        mut backend_cmd_listener: UnboundedReceiver<MaximaLibRequest>,
        backend_responder: UnboundedSender<MaximaLibResponse>,
        rtm_cmd_listener: UnboundedReceiver<MaximaEventRequest>,
        rtm_responder: UnboundedSender<MaximaEventResponse>,
        remote_provider_channel: UnboundedSender<UIImageCacheLoaderCommand>,
        ctx: &Context,
    ) -> Result<(), BackendError> {
        // first things first check registry
        // the flow is different for windows/linux but windows needs an extra user prompt,
        // so we're doing both here, instead of selectively cfg'd functions!
        #[cfg(not(windows))]
        if let Err(err) = check_registry_validity() {
            warn!("{}, fixing...", err);
            set_up_registry()?;
        }

        #[cfg(windows)]
        {
            use maxima::{
                core::background_service::request_registry_setup,
                util::{
                    registry::check_registry_validity,
                    service::{
                        is_service_running, is_service_valid, register_service_user, start_service,
                    },
                },
            };

            if !is_elevated::is_elevated() {
                if !is_service_valid()? {
                    info!("Installing service...");
                    backend_responder.send(MaximaLibResponse::ServiceNeedsStarting).ok();

                    'wait_for_auth: loop {
                        let Some(request) = backend_cmd_listener.recv().await else {
                            info!("Backend command channel closed, shutting down");
                            return Ok(());
                        };

                        match request {
                            MaximaLibRequest::StartService => {
                                register_service_user()?;
                                tokio::time::sleep(Duration::from_millis(200)).await;
                                break 'wait_for_auth;
                            }
                            MaximaLibRequest::ShutdownRequest => return Ok(()),
                            _ => {}
                        }
                    }
                }

                if !is_service_running()? {
                    info!("Starting service...");
                    start_service().await?;

                    let service_addr = "127.0.0.1:13021";
                    let mut ready = false;
                    for attempt in 0..50 {
                        match tokio::net::TcpStream::connect(service_addr).await {
                            Ok(_) => {
                                ready = true;
                                info!("Service HTTP endpoint ready after {} attempts", attempt + 1);
                                break;
                            }
                            Err(e) => {
                                if attempt % 5 == 0 {
                                    warn!(
                                        "Service not ready (attempt {}/50): {} — kind: {:?}",
                                        attempt + 1,
                                        e,
                                        e.kind()
                                    );
                                }
                                tokio::time::sleep(Duration::from_millis(300)).await;
                            }
                        }
                    }
                    if !ready {
                        return Err(BackendError::BackgroundServiceClient(
                            maxima::core::error::BackgroundServiceClientError::Request(
                                "Service did not become ready after 50 attempts".to_string(),
                            ),
                        ));
                    }
                }
            }

            if let Err(err) = check_registry_validity() {
                warn!("{}, fixing...", err);
                if let Err(e) = request_registry_setup().await {
                    let source_chain: Vec<String> =
                        std::iter::successors(Some(&e as &dyn std::error::Error), |e| e.source())
                            .map(|e| e.to_string())
                            .collect();
                    error!("Registry setup failed: {} (source: {:?})", e, source_chain);
                    return Err(e.into());
                }
            }
        }
        let maxima_arc: LockedMaxima = Maxima::new_with_options(
            MaximaOptionsBuilder::default()
                .dummy_local_user(false)
                .load_auth_storage(true)
                .build()?,
        )
        .await?;

        let logged_in = {
            let maxima = maxima_arc.lock().await;
            maxima.start_lsx(maxima_arc.clone()).await?;
            info!("LSX started");

            let mut auth_storage = maxima.auth_storage().lock().await;
            auth_storage.logged_in().await?
        };

        if !logged_in {
            backend_responder.send(MaximaLibResponse::LoginCacheEmpty).ok();

            'login: loop {
                let Some(request) = backend_cmd_listener.recv().await else {
                    return Ok(());
                };
                match request {
                    MaximaLibRequest::LoginRequestOauth => {
                        login_oauth(maxima_arc.clone(), backend_responder.clone(), ctx)
                            .await
                            .expect("// TODO(headassbtw): panic message");
                        break 'login;
                    }
                    MaximaLibRequest::ShutdownRequest => return Ok(()),
                    _ => {}
                }
            }
        }

        {
            let maxima = maxima_arc.lock().await;
            let user = maxima.local_user().await?;

            if logged_in {
                backend_responder
                    .send(MaximaLibResponse::LoginResponse(Ok(
                        InteractThreadLoginResponse {
                            you: user.player().as_ref().unwrap().to_owned(),
                        },
                    )))
                    .ok();
            }

            let avatar_result: Result<(), BackendError> = (|| async {
                let avatar = user
                    .player()
                    .as_ref()
                    .ok_or(ServiceLayerError::MissingField)?
                    .avatar()
                    .as_ref()
                    .ok_or(ServiceLayerError::MissingField)?
                    .medium()
                    .path()
                    .to_string();

                remote_provider_channel
                    .send(UIImageCacheLoaderCommand::ProvideRemote(
                        crate::ui_image::UIImageType::Avatar(user.id().to_string()),
                        avatar,
                    ))
                    .ok();
                Ok(())
            })()
            .await;

            if let Err(e) = avatar_result {
                warn!("Failed to load user avatar: {}", e);
                // continue anyway
            }

            ctx.request_repaint();
        }

        let _ = EventThread::new(
            &ctx.clone(),
            maxima_arc.clone(),
            rtm_cmd_listener,
            rtm_responder,
        );

        let mut next_tick = tokio::time::Instant::now() + Duration::from_millis(50);
        let mut playing_cache: Option<String> = None;

        'main: loop {
            tokio::select! {
            _ = tokio::time::sleep_until(next_tick) => {
                next_tick = tokio::time::Instant::now() + Duration::from_millis(50);
                let mut maxima = maxima_arc.lock().await;
                maxima.update().await;

                // progress reporting
                match maxima.playing() {
                    Some(ctx) if playing_cache.is_none() => {
                        if let Some(offer) = ctx.offer() {
                            playing_cache = Some(offer.slug().clone());
                            backend_responder
                                .send(MaximaLibResponse::ActiveGameChanged(Some(
                                    offer.slug().clone(),
                                )))
                                .ok();
                        }
                    }
                    None if playing_cache.is_some() => {
                        playing_cache = None;
                        backend_responder.send(MaximaLibResponse::ActiveGameChanged(None)).ok();
                    }
                    _ => {}
                }

                 if let Some(dl) = maxima.content_manager().current() {
                    backend_responder.send(MaximaLibResponse::DownloadProgressChanged(
                        dl.offer_id().to_string(),
                        InteractThreadDownloadProgressResponse {
                            bytes: dl.bytes_downloaded(),
                            bytes_total: dl.bytes_total(),
                        },
                    )).ok();
                }

                for ev in maxima.consume_pending_events() {
                    match ev {
                        MaximaEvent::InstallFinished(offer_id) => {
                            backend_responder.send(MaximaLibResponse::DownloadFinished(offer_id)).ok();
                            Self::update_queue(maxima.content_manager(), backend_responder.clone());
                        }
                        MaximaEvent::InstallFailed(offer_id, reason) => {
                            backend_responder.send(MaximaLibResponse::DownloadFailed(offer_id, reason)).ok();
                            Self::update_queue(maxima.content_manager(), backend_responder.clone());
                        }
                        _ => {}
                    }
                }
            }

            request = backend_cmd_listener.recv() => {
                let Some(request) = request else {
                    continue 'main;
                };
                let result = match request {
                    MaximaLibRequest::LoginRequestOauth | MaximaLibRequest::StartService => {
                        error!("bro tried to log in twice");
                        Ok(())
                    }

                    MaximaLibRequest::GetGamesRequest => {
                        let maxima = maxima_arc.clone();
                        let responder = backend_responder.clone();
                        let remote = remote_provider_channel.clone();
                        let context = ctx.clone();
                        tokio::task::spawn(async move {
                            if let Err(e) =
                                get_games_request(maxima, responder.clone(), remote, &context).await
                            {
                                let _ =
                                    responder.send(MaximaLibResponse::NonFatalError(Box::from(e))).ok();
                            }
                        });
                        Ok(())
                    }
                    MaximaLibRequest::GetFriendsRequest => {
                        let f = async || {
                            get_friends_request(
                                maxima_arc.clone(),
                                backend_responder.clone(),
                                remote_provider_channel.clone(),
                                ctx,
                            )
                            .await
                        };
                        f().await
                    }
                    MaximaLibRequest::GetGameDetailsRequest(slug) => {
                        let f = async || {
                            game_details_request(
                                maxima_arc.clone(),
                                slug,
                                backend_responder.clone(),
                                ctx,
                            )
                            .await
                        };
                        f().await
                    }
                    MaximaLibRequest::LocateGameRequest(slug, mut path, wine_prefix) => {
                        if path.ends_with('/') || path.ends_with('\\') {
                            path.pop();
                        }

                        let path = PathBuf::from(path);
                        let manifest_path = path.join(MANIFEST_RELATIVE_PATH);
                        let response = match manifest::load_manifest_from_disk(manifest_path.clone()).await {
                            Ok(manifest) => {
                                let should_force_touchup = manifest.needs_touchup_on_locate();
                                if should_force_touchup {
                                    match handle_touchup_request(path, wine_prefix, &slug).await {
                                        Ok(()) => InteractThreadLocateGameResponse::Success,
                                        Err(err) => {
                                            warn!("Touchup failed during locate, treating as success: {}", err);
                                            InteractThreadLocateGameResponse::Success
                                        }
                                    }
                                } else {
                                     #[cfg(windows)]
                                    let game_install_info = GameInstallInfo::new(path, None);

                                    #[cfg(unix)]
                                    let game_install_info = GameInstallInfo::new(path, wine_prefix);

                                    game_install_info.save_to_json(&slug);
                                    InteractThreadLocateGameResponse::Success
                                }
                            }
                            Err(err) => InteractThreadLocateGameResponse::Error(
                                InteractThreadLocateGameFailure {
                                    reason: err,
                                    xml_path: manifest_path.to_string_lossy().into_owned(),
                                },
                            ),
                        };

                        let _ = backend_responder
                            .send(MaximaLibResponse::LocateGameResponse(response))
                            .ok();
                        info!("finished locating");
                        ctx.request_repaint();
                        Ok(())
                    }

                    MaximaLibRequest::InstallGameRequest(offer, slug, path, wine_prefix) => {
                        let mut maxima = maxima_arc.lock().await;
                        let builds = maxima.content_manager().service().available_builds(&offer).await?;
                        let build = builds.live_build().ok_or(BackendError::NoLiveBuildAvailable)?;
                        let game = QueuedGameBuilder::default()
                            .offer_id(offer)
                            .build_id(build.build_id().to_owned())
                            .path(path)
                            .slug(slug)
                            .wine_prefix(wine_prefix)
                            .build()?;
                        maxima.content_manager().add_install(game).await?;
                        Self::update_queue(maxima.content_manager(), backend_responder.clone());
                        Ok(())
                    }
                    MaximaLibRequest::CancelInstallRequest(offer) => {
                        let mut maxima = maxima_arc.lock().await;
                        maxima.content_manager().cancel_install(&offer).await?;
                        Self::update_queue(maxima.content_manager(), backend_responder.clone());
                        Ok(())
                    }
                    MaximaLibRequest::PauseInstallRequest(offer) => {
                        let mut maxima = maxima_arc.lock().await;
                        maxima.content_manager().pause_install(&offer).await?;
                        Self::update_queue(maxima.content_manager(), backend_responder.clone());
                        Ok(())
                    }
                    MaximaLibRequest::MoveInstallToTopRequest(offer) => {
                        let mut maxima = maxima_arc.lock().await;
                        maxima.content_manager().move_install_to_top(&offer).await?;
                        Self::update_queue(maxima.content_manager(), backend_responder.clone());
                        Ok(())
                    }
                    MaximaLibRequest::StartGameRequest(info, settings) => {
                        Ok(start_game_request(maxima_arc.clone(), info, settings).await?)
                    }
                    MaximaLibRequest::ShutdownRequest => {
                        info!("BridgeThread: received shutdown request");
                        break 'main Ok(());
                    }
                };

                if let Err(err) = result {
                            backend_responder.send(MaximaLibResponse::NonFatalError(Box::from(err))).ok();
                        }
                    }
                }
            puffin::GlobalProfiler::lock().new_frame();
        }
    }
}
