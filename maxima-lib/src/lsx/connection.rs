use derive_getters::Getters;
use log::{debug, error, warn};
use quick_xml::DeError;
use rand::rand_core::Rng;
use regex::Regex;
use std::{io::ErrorKind, path::PathBuf, sync::LazyLock};
use sysinfo::{Pid, System};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;
use tokio::{io::AsyncWriteExt, sync::MutexGuard};

use super::{
    request::{
        account::handle_query_entitlements_request,
        auth::handle_auth_code_request,
        challenge::handle_challenge_response,
        config::handle_config_request,
        core::{
            handle_connectivity_request, handle_set_downloader_util_request,
            handle_settings_request,
        },
        game::{handle_all_game_info_request, handle_game_info_request},
        igo::handle_show_igo_window_request,
        license::handle_license_request,
        offer::handle_query_offers_request,
        profile::{
            handle_get_block_list_request, handle_presence_request, handle_profile_request,
            handle_query_friends_request, handle_query_image_request,
            handle_query_presence_request, handle_set_presence_request,
        },
        progressive_install::{handle_pi_availability_request, handle_pi_installed_chunks_request},
        voip::handle_voip_status_request,
    },
    types::{
        LSX, LSXChallenge, LSXEvent, LSXEventType, LSXMessageType, LSXRequest, LSXResponse,
        create_lsx_message,
    },
};
use crate::{
    core::{LockedMaxima, Maxima, MaximaEvent, launch::ActiveGameContext},
    lsx::{request::LSXRequestError, types::LSXRequestType},
    util::{
        native::NativeError,
        simple_crypto::{simple_decrypt, simple_encrypt},
    },
};

#[derive(Error, Debug)]
pub enum LSXConnectionError {
    #[error(transparent)]
    XmlDeserialize(#[from] DeError),
    #[error(transparent)]
    XmlSerialize(#[from] quick_xml::se::SeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Request(#[from] LSXRequestError),
    #[error(transparent)]
    Native(#[from] NativeError),

    #[error("LSX connection closed")]
    Closed,
    #[error("there is no active game context, LSX connection cannot be established")]
    GameContext,
    #[error("internal error in LSX connection: {0}")]
    Internal(ErrorKind),
}

const CORE_SENDER: &str = "EALS";

const CHALLENGE_BUILD: &str = "release";
//const CHALLENGE_KEY: &str = "cacf897a20b6d612ad0c05e011df52bb";
const CHALLENGE_VERSION: &str = "10,5,64,37936"; // is it Origin-related (this is the last version) or EA Desktop (would be way above, needs checkings)
static LSX_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<LSX>.*?</LSX>").expect("LSX pattern regex should be valid"));

macro_rules! lsx_message_matcher {
    (
        $connection_var:expr, $message_var:expr, $message_type:ty;
        $($name:ident $handler:ident),* $(,)?
    ) => {
        pastey::paste! {
            match $message_var {
                $(
                    $message_type::$name(msg) => $handler($connection_var, msg).await,
                )*
            }?
        }
    };
}

pub enum EncryptionState {
    Disabled,
    Ready([u8; 16]),
    Enabled([u8; 16]),
}

#[derive(Getters)]
pub struct ConnectionState {
    maxima: LockedMaxima,
    access_token: String,
    challenge: String,
    encryption: EncryptionState,
    pid: u32,
}

impl ConnectionState {
    /// Enable encryption on the packet after next
    pub fn enable_encryption(&mut self, encryption_key: [u8; 16]) {
        self.encryption = EncryptionState::Ready(encryption_key);
    }
}

pub fn get_os_pid(context: &ActiveGameContext) -> Result<u32, NativeError> {
    let mut pid = None;

    let sys = System::new_all();
    for (p_pid, process) in sys.processes() {
        if process.cmd().is_empty() {
            continue;
        }

        let mut cmd = process.cmd()[0].to_string_lossy().into_owned();

        // Wine path handling
        if cfg!(unix) && cmd.starts_with("Z:") {
            cmd = cmd.replace("Z:", "").replace('\\', "/");
        }

        if !cmd.starts_with(context.game_path()) {
            continue;
        }

        for ele in process.environ() {
            let env = ele.to_string_lossy();
            let (key, value) = env.split_once('=').unwrap_or((&env, ""));
            if key != "MXLaunchId" || value != context.launch_id() {
                continue;
            }

            pid = Some(p_pid.as_u32());
            break;
        }
    }

    Ok(pid.unwrap_or(0))
}

#[cfg(target_os = "windows")]
pub async fn get_wine_pid(
    _launch_id: &str,
    _name: &str,
    _slug: Option<&str>,
) -> Result<u32, NativeError> {
    Ok(0)
}

#[cfg(target_os = "linux")]
pub async fn get_wine_pid(
    launch_id: &str,
    name: &str,
    slug: Option<&str>,
) -> Result<u32, NativeError> {
    use crate::core::background_service::wine_get_pid;
    wine_get_pid(launch_id, name, slug).await
}

pub struct Connection {
    stream: TcpStream,
    state: ConnectionState,
    tx: Sender<String>,
}

impl Connection {
    pub async fn new(
        maxima_arc: LockedMaxima,
        mut stream: TcpStream,
        tx: Sender<String>,
    ) -> Result<Self, LSXConnectionError> {
        stream.set_nodelay(true)?;

        let maxima: MutexGuard<'_, Maxima> = maxima_arc.lock().await;
        let context: &ActiveGameContext = match maxima.playing() {
            Some(ctx) => ctx,
            None => {
                stream.shutdown().await?;
                return Err(LSXConnectionError::GameContext);
            }
        };

        // The PID system is mainly for Kyber injection
        let mut pid = get_os_pid(context);
        if cfg!(unix)
            && let Ok(os_pid) = pid
        {
            let sys = System::new_all();
            if let Some(process) = sys.process(Pid::from_u32(os_pid)) {
                let filename = PathBuf::from(
                    process.cmd()[0]
                        .to_string_lossy()
                        .replace("Z:", "")
                        .replace('\\', "/"),
                )
                .file_name()
                .ok_or(NativeError::FileName)?
                .to_str()
                .ok_or(NativeError::Stringify)?
                .to_owned();

                pid = get_wine_pid(context.launch_id(), &filename, context.slug().as_deref()).await;
            } else {
                warn!(
                    "Failed to find game process while looking for PID {}",
                    os_pid
                );
            }
        }

        if let Err(ref err) = pid {
            warn!("Error while finding game PID: {}", err);
        } else if pid.as_ref().unwrap() == &0 {
            warn!("Failed to find PID through launch ID, things may not work!");
        }

        // Generate fresh 16-byte challenge per connection
        let challenge: String = {
            let mut bytes = [0u8; 16];
            rand::rng().fill_bytes(&mut bytes);
            hex::encode(bytes)
        };

        drop(maxima);

        let state = ConnectionState {
            maxima: maxima_arc.clone(),
            access_token: maxima_arc.lock().await.access_token().await.unwrap(),
            challenge,
            encryption: EncryptionState::Disabled,
            pid: pid.unwrap_or(0),
        };

        Ok(Self { stream, state, tx })
    }

    // Initialization

    pub async fn queue_challenge(&mut self) -> Result<(), LSXConnectionError> {
        let challenge = create_lsx_message(LSXMessageType::Event(LSXEvent {
            sender: CORE_SENDER.to_string(),
            value: LSXEventType::Challenge(LSXChallenge {
                attr_build: CHALLENGE_BUILD.to_string(),
                attr_key: self.state.challenge.to_owned(),
                attr_version: CHALLENGE_VERSION.to_string(),
            }),
        }));

        self.queue_message(challenge).await?;
        Ok(())
    }

    pub async fn write_message(&mut self, message: String) -> Result<(), LSXConnectionError> {
        self.stream.write_all(message.as_bytes()).await?;
        let _ = self.stream.flush().await;
        Ok(())
    }

    pub async fn queue_message(&mut self, message: LSX) -> Result<(), LSXConnectionError> {
        let mut str = quick_xml::se::to_string(&message)?;
        debug!("Queuing LSX Message: {}", str);

        if let EncryptionState::Enabled(key) = self.state.encryption {
            str = simple_encrypt(str.as_bytes(), &key)
        };

        str += "\0"; // Sent strings need to be null terminated
        let _ = self.tx.send(str).await;
        Ok(())
    }

    pub async fn read_incoming_messages(&mut self) -> Result<(), LSXConnectionError> {
        self.stream.readable().await?;
        let mut buffer = [0; 1024 * 8];
        let n = match self.stream.try_read(&mut buffer) {
            Ok(0) => return Err(LSXConnectionError::Closed),
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(()),
            Err(e) => return Err(e.into()),
        };

        let trimmed_buffer = &buffer[..(n - 1)];
        let message = if let EncryptionState::Enabled(key) = self.state.encryption {
            simple_decrypt(trimmed_buffer, &key)
        } else {
            String::from_utf8_lossy(trimmed_buffer).trim().to_owned()
        };

        for mat in LSX_PATTERN.find_iter(message.as_str()) {
            if let Err(err) = self.process_incoming_message(mat.as_str()).await {
                error!("Failed to process message: {}", err);
            }
        }

        Ok(())
    }

    // Message Processing

    async fn process_incoming_message(&mut self, message: &str) -> Result<(), LSXConnectionError> {
        debug!("Received LSX Message: {}", message);

        let mut message = message.to_string();
        // replace unstable remove_matches (when most of the devs seem to think that it's stable since 2024 ??) w/ regex
        message = message.replace(r#"version="" "#, "");
        let lsx_message: LSX = quick_xml::de::from_str(message.as_str())?;

        let reply = match lsx_message.value {
            LSXMessageType::Request(msg) => self.process_request_message(msg).await?,
            LSXMessageType::Event(_) => {
                None // Blank for now
            }
            LSXMessageType::Response(_) => {
                warn!(
                    "Unexpected LSX Response message received, ignoring (server message type sent to server)"
                );
                None
            }
        };

        if let Some(reply) = reply {
            self.queue_message(LSX { value: reply }).await?;
        }

        if let EncryptionState::Ready(key) = &self.state.encryption {
            self.state.encryption = EncryptionState::Enabled(*key);
        }

        Ok(())
    }

    async fn process_request_message(
        &mut self,
        message: LSXRequest,
    ) -> Result<Option<LSXMessageType>, LSXConnectionError> {
        {
            let (maxima_arc, pid) = { (self.state.maxima.clone(), *self.state.pid()) };

            maxima_arc
                .lock()
                .await
                .call_event(MaximaEvent::ReceivedLSXRequest(pid, message.value.clone()));
        }

        let result = lsx_message_matcher!(

            &mut self.state, message.value, LSXRequestType;

            ChallengeResponse handle_challenge_response,
            GetBlockList handle_get_block_list_request,
            GetConfig handle_config_request,
            GetProfile handle_profile_request,
            GetSetting handle_settings_request,
            RequestLicense handle_license_request,
            GetGameInfo handle_game_info_request,
            GetAllGameInfo handle_all_game_info_request,
            GetInternetConnectedState handle_connectivity_request,
            IsProgressiveInstallationAvailable handle_pi_availability_request,
            AreChunksInstalled handle_pi_installed_chunks_request,
            GetAuthCode handle_auth_code_request,
            GetPresence handle_presence_request,
            SetPresence handle_set_presence_request,
            QueryOffers handle_query_offers_request,
            QueryPresence handle_query_presence_request,
            QueryFriends handle_query_friends_request,
            QueryEntitlements handle_query_entitlements_request,
            QueryImage handle_query_image_request,
            GetVoipStatus handle_voip_status_request,
            ShowIGOWindow handle_show_igo_window_request,
            SetDownloaderUtilization handle_set_downloader_util_request,
        );

        Ok(match result {
            Some(result) => Some(LSXMessageType::Response(LSXResponse {
                sender: message.recipient,
                id: message.id,
                value: result,
            })),
            None => None,
        })
    }
}
