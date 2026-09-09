use crate::{
    BackendStallState, GameDetails, GameDetailsWrapper, MaximaEguiApp,
    bridge_thread::{self, BackendError},
    views::downloads_view::QueuedDownload,
};
use log::{info, warn};
use tokio::sync::mpsc::error::TryRecvError;

pub fn frontend_processor(app: &mut MaximaEguiApp, ctx: &egui::Context) {
    puffin::profile_function!();

    if app.critical_error.is_some() {
        return;
    }

    'outer: loop {
        match app.backend.backend_listener.try_recv() {
            Ok(result) => {
                use bridge_thread::MaximaLibResponse::*;
                match result {
                    LoginResponse(res) => {
                        if let Err(error) = &res {
                            warn!("Login failed. {}", error);
                            continue;
                        }
                        let res = res.unwrap();

                        info!("Logged in as {}!", &res.you.display_name());
                        app.user_name = res.you.display_name().clone();
                        app.user_id = res.you.id().clone();
                        app.backend_state = BackendStallState::BingChilling;
                        let _ = app
                            .backend
                            .backend_commander
                            .send(bridge_thread::MaximaLibRequest::GetGamesRequest);
                        let _ = app
                            .backend
                            .backend_commander
                            .send(bridge_thread::MaximaLibRequest::GetFriendsRequest);
                    }
                    LoginCacheEmpty => app.backend_state = BackendStallState::UserNeedsToLogIn,
                    ServiceNeedsStarting => {
                        app.backend_state = BackendStallState::UserNeedsToInstallService
                    }
                    ServiceStarted => app.backend_state = BackendStallState::Starting,
                    GameInfoResponse(res) => {
                        app.games.insert(res.game.slug.clone(), res.game);
                    }
                    GameDetailsResponse(res) => {
                        let response = res.response;
                        if let Some(game) = app.games.get_mut(&res.slug) {
                            game.details = GameDetailsWrapper::Available(GameDetails {
                                time: response.time,
                                achievements_unlocked: response.achievements_unlocked,
                                achievements_total: response.achievements_total,
                                path: response.path.clone(),
                                system_requirements_min: response.system_requirements_min.clone(),
                                system_requirements_rec: response.system_requirements_rec.clone(),
                            });
                        }
                    }
                    FriendInfoResponse(res) => app.friends.push(res.friend),
                    CriticalError(err) => app.critical_error = Some(*err),
                    NonFatalError(err) => app.nonfatal_errors.push(*err),
                    ActiveGameChanged(slug) => app.playing_game = slug,
                    LocateGameResponse(res) => {
                        if matches!(
                            res,
                            bridge_thread::InteractThreadLocateGameResponse::Success
                        ) {
                            let _ = app
                                .backend
                                .backend_commander
                                .send(bridge_thread::MaximaLibRequest::GetGamesRequest);
                        }
                        app.installer_state.locate_response = Some(res);
                        app.installer_state.locating = false;
                        app.backend
                            .backend_commander
                            .send(bridge_thread::MaximaLibRequest::GetGamesRequest)
                            .unwrap();
                    }
                    DownloadProgressChanged(offer_id, progress) => {
                        if let Some(dl_ing) = app.installing_now.as_mut() {
                            if dl_ing.offer == offer_id {
                                dl_ing.downloaded_bytes = progress.bytes;
                                dl_ing.total_bytes = progress.bytes_total;
                            }
                        }
                    }
                    DownloadFinished(_) => {
                        app.backend
                            .backend_commander
                            .send(bridge_thread::MaximaLibRequest::GetGamesRequest)
                            .unwrap();
                    }
                    DownloadQueueUpdate(current, queue) => {
                        if let Some(current) = current {
                            if !app.installing_now.as_ref().is_some_and(|n| n.offer == current) {
                                let slug = find_slug_for_offer(&app.games, &current);
                                if !slug.is_empty() {
                                    app.installing_now = Some(QueuedDownload {
                                        slug,
                                        offer: current,
                                        downloaded_bytes: 0,
                                        total_bytes: 0,
                                    });
                                }
                            }

                            app.install_queue.clear();
                            for offer in queue {
                                let slug = find_slug_for_offer(&app.games, &offer);
                                if slug.is_empty() {
                                    continue;
                                }
                                app.install_queue.insert(
                                    offer.clone(),
                                    QueuedDownload {
                                        slug,
                                        offer,
                                        downloaded_bytes: 0,
                                        total_bytes: 0,
                                    },
                                );
                            }
                        } else {
                            app.installing_now = None;
                        }
                    }
                    DownloadFailed(offer, reason) => {
                        app.installing_now = None;
                        app.nonfatal_errors.push(BackendError::DownloadFailed(offer, reason));
                    }
                }
                ctx.request_repaint();
            }
            Err(TryRecvError::Empty) => break 'outer,
            Err(TryRecvError::Disconnected) => {
                app.critical_error = Some(BackendError::ChannelDisconnected);
                break 'outer;
            }
        }
    }
}

fn find_slug_for_offer(
    games: &std::collections::HashMap<String, crate::GameInfo>,
    offer: &str,
) -> String {
    games
        .iter()
        .find(|(_, g)| g.offer == offer)
        .map(|(slug, _)| slug.clone())
        .unwrap_or_default()
}
