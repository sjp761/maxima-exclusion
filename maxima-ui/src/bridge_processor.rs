use log::{debug, error, info, warn};

use crate::{
    bridge_thread, views::{downloads_view::QueuedDownload, friends_view::UIFriendImageWrapper}, GameDetails, GameDetailsWrapper, GameUIImages, GameUIImagesWrapper, MaximaEguiApp
};

pub fn frontend_processor(app: &mut MaximaEguiApp, ctx: &egui::Context) {
    puffin::profile_function!();

    while let Ok(result) = app.backend.backend_listener.try_recv() {
        match result {
            bridge_thread::MaximaLibResponse::LoginResponse(res) => {
                info!("Got something");
                if !res.success {
                    warn!("Login failed.");
                    app.in_progress_credential_status = res.description;
                    continue;
                }

                app.logged_in = true;
                info!("Logged in as {}!", &res.description);
                app.user_name = res.description.clone();
                app.login_cache_waiting = false;
                app.backend
                    .backend_commander
                    .send(bridge_thread::MaximaLibRequest::GetGamesRequest)
                    .unwrap();
                app.backend
                    .backend_commander
                    .send(bridge_thread::MaximaLibRequest::GetFriendsRequest)
                    .unwrap();
            }
            bridge_thread::MaximaLibResponse::LoginCacheEmpty => {
                app.login_cache_waiting = false;
            }
            bridge_thread::MaximaLibResponse::GameInfoResponse(res) => {
                app.games.insert(res.game.slug.clone(), res.game);
                ctx.request_repaint(); // Run this loop once more, just to see if any games got lost
            }
            bridge_thread::MaximaLibResponse::GameDetailsResponse(res) => {
                if res.response.is_err() {
                    continue;
                }

                let response = res.response.unwrap();

                for (slug, game) in &mut app.games {
                    if !slug.eq(&res.slug) {
                        continue;
                    }

                    game.details = GameDetailsWrapper::Available(GameDetails {
                        time: response.time,
                        achievements_unlocked: response.achievements_unlocked,
                        achievements_total: response.achievements_total,
                        path: response.path.clone(),
                        system_requirements_min: response.system_requirements_min.clone(),
                        system_requirements_rec: response.system_requirements_rec.clone(),
                    });
                }

                ctx.request_repaint();
            }
            bridge_thread::MaximaLibResponse::FriendInfoResponse(res) => {
                app.friends.push(res.friend);
                ctx.request_repaint();
            }
            bridge_thread::MaximaLibResponse::GameUIImagesResponse(res) => {
                debug!("Got UIImages back from the interact thread");
                if res.response.is_err() {
                    continue;
                }

                let response = res.response.unwrap();

                for (slug, game) in &mut app.games {
                    if !slug.eq(&res.slug) {
                        continue;
                    }

                    debug!("setting images for {:?}", game.slug);
                    game.images = GameUIImagesWrapper::Available(GameUIImages {
                        hero: response.hero.to_owned(),
                        logo: response.logo.to_owned(),
                    });
                }
                ctx.request_repaint(); // Run this loop once more, just to see if any games got lost
            }
            bridge_thread::MaximaLibResponse::UserAvatarResponse(res) => {
                debug!("Got Avatar back from the interact thread");
                if res.response.is_err() {
                    error!("{}", res.response.err().expect("").to_string());
                    continue;
                }

                let response = res.response.unwrap();

                for user in &mut app.friends {
                    if !user.id.eq(&res.id) {
                        continue;
                    }

                    user.avatar = UIFriendImageWrapper::Available(response.clone());
                }
                ctx.request_repaint(); // Run this loop once more, just to see if any games got lost
            }
            bridge_thread::MaximaLibResponse::InteractionThreadDiedResponse => {
                error!("interact thread died");
                app.critical_bg_thread_crashed = true;
            }
            bridge_thread::MaximaLibResponse::ActiveGameChanged(slug) => {
                app.playing_game = slug;
            },
            bridge_thread::MaximaLibResponse::LocateGameResponse(res) => {
                app.installer_state.locate_response = Some(res);
                app.installer_state.locating = false;
            },
            bridge_thread::MaximaLibResponse::DownloadProgressChanged(offer_id, progress) => {
                if let Some(dl_ing) = app.installing_now.as_mut() {
                    if dl_ing.offer == offer_id {
                        dl_ing.downloaded_bytes = progress.bytes;
                        dl_ing.total_bytes = progress.bytes_total;
                    }
                }
            },
            bridge_thread::MaximaLibResponse::DownloadFinished(_) => {
                // idk
            },
            bridge_thread::MaximaLibResponse::DownloadQueueUpdate(current, queue) => {
                if let Some(current) = current {
                    if !app.installing_now.as_ref().is_some_and(|n| n.offer == current) {
                        app.installing_now = Some(QueuedDownload {
                            slug: {
                                // This sucks!
                                let mut rtn: String = String::new();
                                for (slug, game) in &app.games {
                                    if game.offer.eq(&current) {
                                        // "but it's less code in the nest"
                                        // "WHO CARES"
                                        // (it was the same amount overall)
                                        rtn = slug.to_string();
                                        break;
                                    }
                                }
                                rtn
                            },
                            offer: current,
                            downloaded_bytes: 0,
                            total_bytes: 0
                        })
                    }
                } else {
                    app.installing_now = None;
                }

                app.install_queue.clear();
                for offer in queue {
                    let i_fucking_hate_this = QueuedDownload {
                        slug: {
                            let mut rtn: String = String::new();
                            for (slug, game) in &app.games {
                                if game.offer.eq(&offer) {
                                    rtn = slug.to_string();
                                    break;
                                }
                            }
                            rtn
                        },
                        offer: offer.clone(),
                        downloaded_bytes: 0,
                        total_bytes: 0
                    };
                    app.install_queue.insert(offer, i_fucking_hate_this);
                }
                
            },
        }
    }
}
