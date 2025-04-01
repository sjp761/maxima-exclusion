use std::fs::File;

use actix_web::{get, post, web, HttpResponse, Responder};
use anyhow::Result;
use dll_syringe::process::OwnedProcess;
use dll_syringe::Syringe;
use log::{info, warn};
use maxima::util::registry::set_up_registry;
use maxima::util::service::SERVICE_NAME;
use std::ffi::OsString;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;
use structured_logger::json::new_writer;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::{
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};

use maxima::core::background_service::{ServiceLibraryInjectionRequest, BACKGROUND_SERVICE_PORT};

use crate::service::error::ServiceError;
use crate::service::hash::get_sha256_hash_of_pid;

mod error;
mod hash;

define_windows_service!(ffi_service_main, service_main);

fn service_main(arguments: Vec<OsString>) {
    if let Err(_e) = bootstrap_service(arguments) {
        // Handle error in some way.
    }
}

fn bootstrap_service(_arguments: Vec<OsString>) -> windows_service::Result<()> {
    let (shutdown_tx, shutdown_rx) = mpsc::channel();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop | ServiceControl::Interrogate => {
                shutdown_tx.send(()).unwrap();
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

    let next_status = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };

    status_handle.set_service_status(next_status)?;
    run_service(shutdown_rx).expect("Failed to run service");

    info!("Shutting down...");

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    Ok(())
}

#[get("/set_up_registry")]
async fn req_set_up_registry() -> impl Responder {
    info!("Setting up registry");
    let result = set_up_registry();
    if result.is_err() {
        return format!("Error: {}", result.err().unwrap());
    }

    format!("Done")
}

// This is for KYBER. Ideally this would be moved to a separate Kyber service,
// but it isn't a great user experience to have to install two windows services.
// We'll eventually find a better workaround and move this somewhere else.
#[post("/inject_library")]
async fn req_inject_library(body: web::Bytes) -> Result<HttpResponse, ServiceError> {
    info!("Injecting...");

    let req: ServiceLibraryInjectionRequest = serde_json::from_slice(&body)?;
    let process = OwnedProcess::from_pid(req.pid)?;

    let hash_result = get_sha256_hash_of_pid(req.pid);
    if hash_result.is_err() {
        return Err(ServiceError::InternalError);
    }

    // Ensure we're only injecting into STAR WARS Battlefront 2. Ideally we would check
    // the hash of the dll as well, but there isn't a great way to do that since there
    // are multiple release channels and dev builds.
    if hex::encode(hash_result.unwrap())
        != "7880e40d79e981b064baaf06f10785601222c6e227a656b70112c24b1f82e2ce"
    {
        warn!("Attempted to inject into invalid process!");
        return Err(ServiceError::InternalError);
    }

    let syringe = Syringe::for_process(process);
    syringe.inject(req.path).unwrap();

    Ok(HttpResponse::Ok().body("Injected"))
}

fn run_service(shutdown_rx: Receiver<()>) -> Result<()> {
    let log_path = Path::new("C:/ProgramData/Maxima/Logs/MaximaBackgroundService.log");
    std::fs::create_dir_all(log_path.parent().unwrap())?;
    let log_file = File::create(log_path)?;

    structured_logger::Builder::new()
        .with_default_writer(new_writer(log_file))
        .init();

    info!("Started Background Service");

    thread::spawn(|| {
        actix_web::rt::System::new().block_on(async {
            use actix_web::{App, HttpServer};

            let _ = HttpServer::new(|| {
                App::new()
                    .service(req_set_up_registry)
                    .service(req_inject_library)
            })
            .bind(("127.0.0.1", BACKGROUND_SERVICE_PORT))
            .unwrap()
            .run()
            .await;
        });
    });

    let _ = shutdown_rx.recv();

    Ok(())
}

pub fn start_service() -> Result<()> {
    service_dispatcher::start("MaximaBackgroundService", ffi_service_main)?;
    Ok(())
}
