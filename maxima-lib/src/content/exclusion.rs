use crate::util::native::maxima_dir;
use globset::{Glob, GlobSet, GlobSetBuilder};
use log::{error, info, warn};
use std::fs::File;
use std::io::{BufRead, BufReader};

pub fn get_exclusion_list(slug: &str) -> GlobSet {
    let mut builder = GlobSetBuilder::new();

    if let Ok(dir) = maxima_dir()
    // Checks to make sure maxima directory exists
    {
        let filepath = dir.join("exclude").join(&slug); // Path to exclusion file
        info!("Loading exclusion file from {}", filepath.display());

        if let Ok(file) = File::open(&filepath) {
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() {
                let entry = line.trim();
                if !entry.is_empty() && !entry.starts_with('#') {
                    if let Ok(g) = Glob::new(entry) {
                        builder.add(g);
                    } else {
                        warn!("Invalid glob pattern '{}' in {}", entry, filepath.display());
                    }
                }
            }
        } else {
            warn!("Exclusion file not found: {}", filepath.display());
        }
    } else {
        error!("Failed to resolve maxima data directory");
    }

    builder
        .build()
        .unwrap_or_else(|_| GlobSetBuilder::new().build().unwrap()) // Returns an empty GlobSet on failure
}
