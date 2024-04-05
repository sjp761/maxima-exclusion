use anyhow::{Ok, Result};
use egui::Context;
use std::sync::mpsc::{Receiver, Sender};

use log::info;
use maxima::{
    core::{
        service_layer::{
            ServiceFriends, ServiceGetMyFriendsRequestBuilder, SERVICE_REQUEST_GETMYFRIENDS,
        },
        LockedMaxima, Maxima, MaximaOptionsBuilder,
    },
    rtm::client::BasicPresence,
};

pub struct EventThreadFriendStatusResponse {
    pub id: String,
    pub presence: maxima::rtm::client::RichPresence,
}

pub enum MaximaEventResponse {
    FriendStatusResponse(EventThreadFriendStatusResponse),
}

pub enum MaximaEventRequest {
    SubscribeToFriendPresence,
    ShutdownRequest,
}

pub struct EventThread {
    pub rx: Receiver<MaximaEventResponse>,
    pub tx: Sender<MaximaEventRequest>,
}

impl EventThread {
    pub fn new(ctx: &Context) -> Self {
        let (tx0, rx1) = std::sync::mpsc::channel();
        let (tx1, rx0) = std::sync::mpsc::channel();
        let context = ctx.clone();

        tokio::task::spawn(async move {
            let result = EventThread::run(rx1, tx1, &context).await;
            if result.is_err() {
                panic!("Event thread failed! {}", result.err().unwrap());
            } else {
                info!("Event thread shut down")
            }
        });

        Self { rx: rx0, tx: tx0 }
    }

    async fn run(
        rx: Receiver<MaximaEventRequest>,
        tx: Sender<MaximaEventResponse>,
        ctx: &Context,
    ) -> Result<()> {
        let maxima_arc: LockedMaxima = Maxima::new_with_options(
            MaximaOptionsBuilder::default()
                .dummy_local_user(false)
                .load_auth_storage(true)
                .build()?,
        )?;

        let mut maxima = maxima_arc.lock().await;

        let friends: ServiceFriends = maxima
            .service_layer()
            .request(
                SERVICE_REQUEST_GETMYFRIENDS,
                ServiceGetMyFriendsRequestBuilder::default()
                    .offset(0)
                    .limit(100)
                    .is_mutual_friends_enabled(false)
                    .build()?,
            )
            .await?;

        let rtm = maxima.rtm();
        rtm.login().await?;
        rtm.set_presence(BasicPresence::Away, "Test", "Origin.OFR.50.0002148")
            .await?;

        let players: Vec<String> = friends
            .friends()
            .items()
            .iter()
            .map(|f| f.id().to_owned())
            .collect();
        info!("Subscribed to {} players", players.len());

        rtm.subscribe(&players).await?;
        drop(maxima);

        'outer: loop {
            let mut maxima = maxima_arc.lock().await;
            maxima.rtm().heartbeat().await?;

            {
                let store = maxima.rtm().presence_store().lock().await;
                for entry in store.iter() {
                    tx.send(MaximaEventResponse::FriendStatusResponse(
                        EventThreadFriendStatusResponse {
                            id: entry.0.to_string(),
                            presence: entry.1,
                        },
                    ))?;
                    // This can cause excessive repainting if it keeps updating friends we know about
                    egui::Context::request_repaint(&ctx);
                }
            }

            drop(maxima);

            let request = rx.try_recv();
            if request.is_err() {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                continue;
            }

            match request? {
                MaximaEventRequest::SubscribeToFriendPresence => {}
                MaximaEventRequest::ShutdownRequest => break 'outer Ok(()),
            }

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
}
