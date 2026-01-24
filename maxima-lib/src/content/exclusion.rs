use std::fs::File;
use std::io::{BufRead, BufReader};
use crate::util::native::maxima_dir;
use globset::{GlobSet, GlobSetBuilder, Glob};
use log::{info, warn, error};

pub fn get_exclusion_list(offer_id: String) -> GlobSet
{
    let mut builder = GlobSetBuilder::new();

    if let Ok(dir) = maxima_dir() // Checks to make sure maxima directory exists
    {
        let filepath = dir.join("exclude").join(&offer_id); // Path to exclusion file
        info!("Loading exclusion file from {}", filepath.display());

        if let Ok(file) = File::open(&filepath) // Opens the exclusion file, fails if not found
        { 
            let reader = BufReader::new(file);
            for line in reader.lines().flatten() 
            {
                let entry = line.trim();
                if !entry.is_empty() && !entry.starts_with('#') 
                {
                    match Glob::new(entry)  // Create a glob from the entry, checks if valid pattern, if not logs a warning, 
                    {
                        Ok(glob) => 
                        {
                            builder.add(glob);
                        }
                        Err(_) => warn!("Invalid glob pattern '{}' in {}", entry, filepath.display()),
                    }
                }
            }
        } 
        else 
        {
            warn!("Exclusion file not found: {}", filepath.display());
        }
    } 
    else 
    {
        error!("Failed to resolve maxima data directory");
    }

    builder.build().unwrap_or_else(|_| GlobSetBuilder::new().build().unwrap()) // Returns an empty GlobSet on failure
}