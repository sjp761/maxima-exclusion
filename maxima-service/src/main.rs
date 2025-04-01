#[cfg(windows)]
#[macro_use]
extern crate windows_service;

#[cfg(windows)]
mod service;

#[cfg(windows)]
fn main() -> anyhow::Result<()> {
    service::start_service()?;
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
