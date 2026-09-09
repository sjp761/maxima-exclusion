#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//extern crate windows_service;

use std::env::current_exe;
use std::error::Error;
use std::string::FromUtf8Error;
use thiserror::Error;
use tokio::process::Command;

use base64::{Engine, engine::general_purpose};
use maxima::core::launch::BootstrapLaunchArgs;
use maxima::util::BackgroundServiceControlError;
use maxima::util::native::NativeError;
#[cfg(windows)]
use maxima::util::service::{is_service_valid, register_service};
use url::{Url, form_urlencoded};

#[cfg(target_os = "macos")]
mod macos;

#[derive(Error, Debug)]
pub(crate) enum RunError {
    #[error(transparent)]
    BackgroundService(#[from] BackgroundServiceControlError),
    #[error(transparent)]
    Base64(#[from] base64::DecodeError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Native(#[from] NativeError),
    #[error(transparent)]
    ParseUrl(#[from] url::ParseError),
    #[error(transparent)]
    ParseUtf8(#[from] FromUtf8Error),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
}

#[cfg(not(target_os = "macos"))]
#[tokio::main]
async fn main() -> Result<(), RunError> {
    let _ = handle_launch_args().await?;

    Ok(())
}

#[cfg(target_os = "macos")]
#[tokio::main]
async fn main() -> Result<()> {
    use cacao::appkit::App;

    use crate::macos::MaximaBootstrapApp;

    let handle = tokio::runtime::Handle::current();
    App::new(
        "dev.armchairdevelopers.MaximaBootstrap",
        MaximaBootstrapApp::new(handle),
    )
    .run();

    Ok(())
}

async fn handle_launch_args() -> Result<bool, RunError> {
    let mut args: Vec<String> = std::env::args().collect();
    args.remove(0);

    let result = run(&args).await;
    if cfg!(debug_assertions) || std::env::var("MAXIMA_DEBUG").is_ok() {
        println!("Args: {:?}", args);

        let str_result = result
            .as_ref()
            .map_err(|e| {
                let source = e.source();

                if source.is_some() {
                    source.unwrap().to_string()
                } else {
                    e.to_string()
                }
            })
            .err()
            .unwrap_or("Success".to_string());
        println!("Result: {}", str_result);

        // Pause terminal
        //std::io::Read::read(&mut std::io::stdin(), &mut [0]).unwrap();
    }

    result
}

#[cfg(windows)]
fn service_setup() -> Result<(), BackgroundServiceControlError> {
    if is_service_valid()? {
        return Ok(());
    }

    register_service()?;

    Ok(())
}

#[cfg(not(windows))]
fn service_setup() -> Result<(), BackgroundServiceControlError> {
    Ok(())
}

#[cfg(windows)]
async fn platform_launch(args: BootstrapLaunchArgs) -> Result<(), NativeError> {
    let mut binding = Command::new(args.path);
    let child = binding.args(args.args);

    let _status = child.spawn()?.wait().await?;
    // bail!("{}", status.code().unwrap());
    Ok(())
}

#[cfg(unix)]
async fn platform_launch(args: BootstrapLaunchArgs) -> Result<(), NativeError> {
    use maxima::unix::wine::CommandType;
    use maxima::unix::wine::run_wine_command;
    use maxima::unix::wine::wine_prefix_dir;

    run_wine_command(
        args.path.into(),
        Some(args.args),
        None,
        false,
        CommandType::WaitForExitAndRun,
        &wine_prefix_dir(&args.slug).unwrap(),
    )
    .await?;

    Ok(())
}

async fn run(args: &[String]) -> Result<bool, RunError> {
    let len = args.len();
    if len == 1 {
        let arg = &args[0];

        if arg == "--noop" {
            return Ok(true);
        }

        if arg.starts_with("link2ea") {
            todo!();
        }

        if arg.starts_with("origin2") {
            let url = Url::parse(arg)?;
            let mut offer_id = None;
            let mut cmd_params = None;

            for (key, value) in form_urlencoded::parse(url.query().unwrap_or_default().as_bytes()) {
                match key.as_ref() {
                    "offerIds" => offer_id = Some(value),
                    "cmdParams" => cmd_params = Some(value),
                    _ => {}
                }
            }

            let _offer_id = offer_id.unwrap();
            let cmd_params = cmd_params.unwrap();

            let mut child = Command::new(current_exe()?.with_file_name("maxima-cli.exe"));
            child.env(
                "MAXIMA_LAUNCH_ARGS",
                urlencoding::decode(cmd_params.as_ref())?
                    .into_owned()
                    .replace("\\\"", "\""),
            );
            println!(
                "{}",
                urlencoding::decode(cmd_params.as_ref())?
                    .into_owned()
                    .replace("\\\"", "\"")
            );
            child.env("KYBER_INTERFACE_PORT", "3005");
            child.args(["--mode", "launch", "--offer-id", "Origin.OFR.50.0002148"]);
            child.spawn()?.wait().await?;

            return Ok(true);
        }

        if arg.starts_with("qrc") {
            let raw_query = arg
                .split("login_successful.html?")
                .nth(1)
                .unwrap_or_default();
            let mut serializer = form_urlencoded::Serializer::new(String::new());
            for (key, value) in form_urlencoded::parse(raw_query.as_bytes()) {
                serializer.append_pair(key.as_ref(), value.as_ref());
            }
            let query = serializer.finish();
            reqwest::get(format!("http://127.0.0.1:31033/auth?{}", query)).await?;

            return Ok(true);
        }

        return Ok(false);
    }

    if len > 1 {
        let command = &args[0];
        let handled = match command.as_str() {
            "launch" => {
                let decoded = general_purpose::STANDARD.decode(&args[1])?;
                let launch_args: BootstrapLaunchArgs = serde_json::from_slice(&decoded)?;
                platform_launch(launch_args).await?;

                true
            }
            _ => false,
        };
        return Ok(handled);
    }

    service_setup()?;

    Ok(false)
}
