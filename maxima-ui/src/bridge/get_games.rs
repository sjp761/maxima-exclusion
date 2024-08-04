use anyhow::{Ok, Result, bail};
use egui::Context;
use log::{debug, info};
use maxima::{core::{service_layer::{ServiceGame, ServiceGameImagesRequestBuilder, ServiceLayerClient, SERVICE_REQUEST_GAMEIMAGES}, LockedMaxima}, util::native::maxima_dir};
use std::{fs, sync::mpsc::Sender};

use crate::{
    bridge_thread::{InteractThreadGameListResponse, MaximaLibResponse}, ui_image::UIImageCacheLoaderCommand, GameDetailsWrapper, GameInfo
};

async fn get_preferred_hero_image(images: &Option<ServiceGame>) -> Option<String> {
    if images.is_none() {
        return None;
    }

    let key_art = images.as_ref().unwrap().key_art();
    if key_art.is_none() {
        return None;
    }

    let key_art = key_art.as_ref().unwrap();

    debug!("{:?}", key_art);
    if let Some(img) = key_art.aspect_10x3_image() {
        return Some(img.path().clone());
    }

    if let Some(img) = key_art.aspect_2x1_image() {
        return Some(img.path().clone());
    }

    if let Some(img) = key_art.aspect_16x9_image() {
        return Some(img.path().clone());
    }

    None
}

fn get_logo_image(images: &Option<ServiceGame>) -> Option<String> {
    if images.is_none() {
        return None;
    }

    let logo_set = images.as_ref().unwrap().primary_logo();
    if logo_set.is_none() {
        return None;
    }

    let largest_logo = logo_set.as_ref().unwrap().largest_image();
    if largest_logo.is_none() {
        return None;
    }

    Some(largest_logo.as_ref().unwrap().path().to_string())
}

async fn handle_images(slug: String, locale: String, has_hero: bool, has_logo: bool, channel: Sender<UIImageCacheLoaderCommand>, service_layer: ServiceLayerClient) -> Result<()> {
    let images: Option<ServiceGame> = // TODO: make it a result
            if !has_hero || !has_logo { //game hasn't been cached yet
                service_layer
                .request(SERVICE_REQUEST_GAMEIMAGES, ServiceGameImagesRequestBuilder::default()
                .should_fetch_context_image(!has_logo)
                .should_fetch_backdrop_images(!has_hero)
                .game_slug(slug.clone())
                .locale(locale.clone())
                .build()?).await?
            } else { None };

        if !has_hero {
            if let Some(hero) = get_preferred_hero_image(&images).await {
                channel.send(UIImageCacheLoaderCommand::ProvideRemote(crate::ui_image::UIImageType::Hero(slug.clone()), hero)).unwrap()
            }
        }

        if !has_logo {
            if let Some(logo) = get_logo_image(&images) {
                channel.send(UIImageCacheLoaderCommand::ProvideRemote(crate::ui_image::UIImageType::Logo(slug), logo))?
            } else {
                channel.send(UIImageCacheLoaderCommand::Stub(crate::ui_image::UIImageType::Logo(slug)))?
            }
        }
        Ok(())
}


pub async fn get_games_request(
    maxima_arc: LockedMaxima,
    channel: Sender<MaximaLibResponse>,
    channel1: Sender<UIImageCacheLoaderCommand>,
    ctx: &Context,
) -> Result<()> {
    debug!("recieved request to load games");
    let mut maxima = maxima_arc.lock().await;
    let service_layer = maxima.service_layer().clone();
    let locale = maxima.locale().short_str().to_owned();
    let logged_in = maxima.auth_storage().lock().await.current().is_some();
    if !logged_in {
        bail!("Ignoring request to load games, not logged in.");
    }


    let owned_games = maxima.mut_library().games().await;
    
    
    if owned_games.len() <= 0 {
        return Ok(());
    }

    for game in owned_games {
        info!("processing {}", &game.base_offer().slug());
        let game_info = GameInfo {
            slug: game.base_offer().slug().to_string(),
            offer: game.base_offer().offer().offer_id().to_string(),
            name: game.name(),
            details: GameDetailsWrapper::Unloaded,
            dlc: game.extra_offers().clone(),
            installed: game.base_offer().installed().await,
            has_cloud_saves: game.base_offer().offer().has_cloud_save(),
        };
        let slug = game_info.slug.clone();
        let settings = crate::GameSettings {
            //TODO: eventually support cloud saves, the option is here for that but for now, keep it disabled in ui!
            cloud_saves: true,
            launch_args: String::new(),
            exe_override: String::new(),
        };
        let res = MaximaLibResponse::GameInfoResponse(InteractThreadGameListResponse {
            game: game_info,
            settings
        });
        channel.send(res)?;
        
        let game_hero = maxima_dir()
            .unwrap()
            .join("cache/ui/images/")
            .join(&slug)
            .join("hero.jpg");
        let game_logo = maxima_dir()
            .unwrap()
            .join("cache/ui/images/")
            .join(&slug)
            .join("logo.png");
        let has_hero = fs::metadata(&game_hero).is_ok();
        let has_logo = fs::metadata(&game_logo).is_ok();

        if !has_hero || !has_logo {
            //we're like 20 tasks deep i swear but this shit's gonna be real fast, trust
            let slug_send = slug.clone();
            let locale_send = locale.clone();
            let channel_send = channel1.clone();
            let service_layer_send = service_layer.clone();
            tokio::task::spawn(async move {
                handle_images(slug_send, locale_send, has_hero, has_logo, channel_send, service_layer_send)
            });
        }

        egui::Context::request_repaint(&ctx);
    }
    Ok(())
}
