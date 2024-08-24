pub mod dip;
pub mod pre_dip;

use std::path::PathBuf;
use anyhow::{bail, Result};
use dip::DiPManifest;
use pre_dip::PreDiPManifest;

pub const MANIFEST_RELATIVE_PATH: &str = "__Installer/installerdata.xml";

#[async_trait::async_trait]
pub trait GameManifest: Send + std::fmt::Debug{
    async fn run_touchup(&self, install_path: &PathBuf) -> Result<()>;
    fn execute_path(&self, trial: bool) -> Option<String>;
}
#[async_trait::async_trait]
impl GameManifest for DiPManifest {
    async fn run_touchup(&self, install_path: &PathBuf) -> Result<()> {
        self.run_touchup(install_path).await
    }

    fn execute_path(&self, trial: bool) -> Option<String> {
        self.execute_path(trial)
    }
}

#[async_trait::async_trait]
impl GameManifest for PreDiPManifest {
    async fn run_touchup(&self, install_path: &PathBuf) -> Result<()> {
        self.run_touchup(install_path).await
    }

    fn execute_path(&self, _: bool) -> Option<String> {
        None // pre-dip games don't have an exe field, most if not all just use info in the offer
    }
}

pub async fn read(path: PathBuf) -> Result<Box<dyn GameManifest>> {
    let dip_attempt = DiPManifest::read(&path).await;
    if let Ok(manifest) = dip_attempt {
        return Ok(Box::new(manifest));
    }
    let predip_attempt = PreDiPManifest::read(&path).await; 
    if let Ok(manifest) = predip_attempt {
        return Ok(Box::new(manifest));
    }
    bail!(format!("Unsupported Manifest.\nDiP Attempt: {}\nPreDiP Attempt: {}", dip_attempt.unwrap_err(), predip_attempt.unwrap_err()));
}