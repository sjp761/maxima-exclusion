use std::ffi::OsString;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::Duration;

use actix_web::{HttpResponse, Responder, get, post, web};
use log::{error, info};
use maxima::core::background_service::{
    BACKGROUND_SERVICE_PORT, ServiceLibraryInjectionRequest, ServiceTouchupRequest,
};
use maxima::util::dll_injector::{DllInjector, InjectionError};
use maxima::util::native::SafeParent;
use maxima::util::registry::set_up_registry;
use maxima::util::service::SERVICE_NAME;
use structured_logger::json::new_writer;
use tokio::sync::oneshot;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::{
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};

use crate::service::error::ServerError;
use crate::service::hash::get_sha256_hash_of_pid;

pub(crate) mod error;
mod hash;

define_windows_service!(ffi_service_main, service_main);

const START_TIMEOUT: Duration = Duration::from_secs(30);
const STOP_TIMEOUT: Duration = Duration::from_secs(30);
const SERVICE_POLL_INTERVAL: Duration = Duration::from_millis(250);
const ERROR_TIMEOUT: u32 = 1460;
const ERROR_UNEXPECTED_EXIT: u32 = 1;

enum Event {
    Bound,
    Exited(std::io::Result<()>),
}

fn service_main(arguments: Vec<OsString>) {
    if let Err(error) = bootstrap_service(arguments) {
        error!("Service main failed: {error}");
    }
}

fn service_status(
    state: ServiceState,
    controls_accepted: ServiceControlAccept,
    exit_code: ServiceExitCode,
    checkpoint: u32,
    wait_hint: Duration,
) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted,
        exit_code,
        checkpoint,
        wait_hint,
        process_id: None,
    }
}

fn start_pending_status() -> ServiceStatus {
    service_status(
        ServiceState::StartPending,
        ServiceControlAccept::empty(),
        ServiceExitCode::Win32(0),
        1,
        START_TIMEOUT,
    )
}

fn running_status() -> ServiceStatus {
    service_status(
        ServiceState::Running,
        ServiceControlAccept::STOP,
        ServiceExitCode::Win32(0),
        0,
        Duration::default(),
    )
}

fn stop_pending_status() -> ServiceStatus {
    service_status(
        ServiceState::StopPending,
        ServiceControlAccept::empty(),
        ServiceExitCode::Win32(0),
        1,
        STOP_TIMEOUT,
    )
}

fn stopped_status(exit_code: ServiceExitCode) -> ServiceStatus {
    service_status(
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
        exit_code,
        0,
        Duration::default(),
    )
}

fn bootstrap_service(_arguments: Vec<OsString>) -> Result<(), ServerError> {
    let (shutdown_tx, shutdown_rx) = std_mpsc::channel::<()>();
    let (_tx, _rx) = std_mpsc::channel::<Event>();
    let (stop_tx, stop_rx) = oneshot::channel::<()>();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop => {
                let _ = shutdown_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

    status_handle.set_service_status(start_pending_status())?;

    let log_path = Path::new("C:/ProgramData/Maxima/Logs/MaximaBackgroundService.log");
    std::fs::create_dir_all(log_path.safe_parent()?)?;
    let log_file = File::create(log_path)?;

    structured_logger::Builder::new()
        .with_default_writer(new_writer(log_file))
        .init();

    info!("Starting Background Service");

    let _handle = thread::spawn(move || run_(_tx, stop_rx));

    match _rx.recv_timeout(START_TIMEOUT) {
        Ok(Event::Bound) => {
            info!("HTTP server bound to 127.0.0.1:{BACKGROUND_SERVICE_PORT}");
        }

        Ok(Event::Exited(Err(error))) => {
            error!("HTTP server failed before readiness: {error}");

            status_handle.set_service_status(stopped_status(ServiceExitCode::Win32(
                error.raw_os_error().unwrap_or(ERROR_UNEXPECTED_EXIT as i32) as u32,
            )))?;

            let _ = _handle.join();
            return Err(error.into());
        }

        Ok(Event::Exited(Ok(()))) => {
            error!("HTTP server stopped before it became ready");

            status_handle.set_service_status(stopped_status(ServiceExitCode::Win32(
                ERROR_UNEXPECTED_EXIT,
            )))?;

            let _ = _handle.join();
            return Err(ServerError::HttpServerStopped);
        }

        Err(std_mpsc::RecvTimeoutError::Timeout) => {
            error!("Timed out waiting for HTTP server bind");

            let _ = stop_tx.send(());
            let _ = _handle.join();

            status_handle
                .set_service_status(stopped_status(ServiceExitCode::Win32(ERROR_TIMEOUT)))?;

            return Err(ServerError::BindTimeout);
        }

        Err(std_mpsc::RecvTimeoutError::Disconnected) => {
            error!("HTTP server thread ended without reporting readiness");

            let _ = _handle.join();

            status_handle.set_service_status(stopped_status(ServiceExitCode::Win32(
                ERROR_UNEXPECTED_EXIT,
            )))?;

            return Err(ServerError::HttpServerStopped);
        }
    }

    status_handle.set_service_status(running_status())?;

    let (stopped_by_scm, exit_code) = loop {
        match shutdown_rx.recv_timeout(SERVICE_POLL_INTERVAL) {
            Ok(()) => break (true, ServiceExitCode::Win32(0)),

            Err(std_mpsc::RecvTimeoutError::Timeout) => match _rx.try_recv() {
                Ok(Event::Exited(Ok(()))) => {
                    error!("HTTP server stopped unexpectedly");
                    break (false, ServiceExitCode::Win32(ERROR_UNEXPECTED_EXIT));
                }

                Ok(Event::Exited(Err(error))) => {
                    error!("HTTP server failed unexpectedly: {error}");

                    let win32_code =
                        error.raw_os_error().unwrap_or(ERROR_UNEXPECTED_EXIT as i32) as u32;

                    break (false, ServiceExitCode::Win32(win32_code));
                }

                Ok(Event::Bound) => {
                    error!("Received unexpected duplicate HTTP bound event");
                }

                Err(std_mpsc::TryRecvError::Empty) => {}

                Err(std_mpsc::TryRecvError::Disconnected) => {
                    error!("HTTP server event channel disconnected");
                    break (false, ServiceExitCode::Win32(ERROR_UNEXPECTED_EXIT));
                }
            },

            Err(std_mpsc::RecvTimeoutError::Disconnected) => {
                error!("Service control channel disconnected");
                break (false, ServiceExitCode::Win32(ERROR_UNEXPECTED_EXIT));
            }
        }
    };

    if stopped_by_scm {
        info!("Stopping Background Service after SCM stop request");
    } else {
        error!("Stopping Background Service because the HTTP server exited unexpectedly");
    }

    status_handle.set_service_status(stop_pending_status())?;

    let _ = stop_tx.send(());
    let _ = _handle.join();

    status_handle.set_service_status(stopped_status(exit_code))?;

    Ok(())
}

#[get("/set_up_registry")]
async fn req_set_up_registry() -> impl Responder {
    info!("Setting up registry");

    if let Err(error) = set_up_registry() {
        return format!("Error: {error}");
    }

    "Done".to_owned()
}

pub fn inject_dll(pid: u32, dll_path: &str) -> Result<(), InjectionError> {
    DllInjector::new(pid).inject(dll_path)
}

// This is for KYBER. Ideally this would be moved to a separate Kyber service,
// but it isn't a great user experience to have to install two Windows services.
#[post("/inject_library")]
async fn req_inject_library(body: web::Bytes) -> Result<HttpResponse, ServerError> {
    info!("Injecting library");

    let request: ServiceLibraryInjectionRequest = serde_json::from_slice(&body)?;
    let hash = get_sha256_hash_of_pid(request.pid)?;

    if hex::encode(hash) != "7880e40d79e981b064baaf06f10785601222c6e227a656b70112c24b1f82e2ce" {
        return Err(ServerError::InvalidInjectionTarget);
    }

    inject_dll(request.pid, &request.path)?;

    Ok(HttpResponse::Ok().body("Injected"))
}

// Replaces the current touchup techniques using Maxima's service to leverage administrative privileges for file access.
// sheesh + sheesh = omegasheesh
#[post("/touchup")]
async fn req_touchup(body: web::Bytes) -> Result<HttpResponse, ServerError> {
    info!("Running touchup");

    let request: ServiceTouchupRequest = serde_json::from_slice(&body)?;
    let manifest = maxima::core::manifest::load_manifest_from_disk(
        Path::new(&request.output_dir).join(maxima::core::manifest::MANIFEST_RELATIVE_PATH),
    )
    .await?;
    manifest
        .run_touchup(&PathBuf::from(request.output_dir), None)
        .await?;

    Ok(HttpResponse::Ok().body("Done"))
}

fn run_(_tx: std_mpsc::Sender<Event>, stop_rx: oneshot::Receiver<()>) {
    actix_web::rt::System::new().block_on(async move {
        use actix_web::{App, HttpServer};

        let http_server = match HttpServer::new(|| {
            App::new()
                .service(req_set_up_registry)
                .service(req_inject_library)
                .service(req_touchup)
        })
        .bind(("127.0.0.1", BACKGROUND_SERVICE_PORT))
        {
            Ok(server) => server,
            Err(error) => {
                let _ = _tx.send(Event::Exited(Err(error)));
                return;
            }
        };

        let server = http_server.run();
        let handle = server.handle();
        tokio::pin!(server);

        if _tx.send(Event::Bound).is_err() {
            handle.stop(true).await;
            let _ = (&mut server).await;
            return;
        }

        tokio::select! {
            result = &mut server => {
                let _ = _tx.send(Event::Exited(result));
            }

            _ = stop_rx => {
                info!("Stopping HTTP server");
                handle.stop(true).await;

                let result = (&mut server).await;
                let _ = _tx.send(Event::Exited(result));
            }
        }
    });
}

pub fn start_service() -> Result<(), ServerError> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}
