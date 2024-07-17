use log::{debug, error, info};
use maxima::core::{launch::{self, LaunchMode}, LockedMaxima};

use crate::GameInfo;

pub async fn start_game_request(maxima_arc: LockedMaxima, game_info: GameInfo) {
    let maxima = maxima_arc.lock().await;
    let logged_in = maxima.auth_storage().lock().await.current().is_some();
    if !logged_in {
        info!("Ignoring request to start game, not logged in.");
        return;
    }

    debug!("got request to start game {:?}", game_info.offer);

    let exe_override = if game_info.settings.exe_override.is_empty() {
        None
    } else {
        Some(game_info.settings.exe_override)
    };

    let arg_list = launch::parse_arguments(&game_info.settings.launch_args);

    info!("args parsed from \"{}\": ", &game_info.settings.launch_args);
    for arg in &arg_list {
        info!("{}", arg);
    }

    drop(maxima);
    let result = launch::start_game(maxima_arc.clone(), LaunchMode::Online(game_info.offer), exe_override, arg_list).await;
    if result.is_err() {
        error!("Failed to start game! Reason: {}", result.err().unwrap());
    }
    

}
