use std::env;

use log::{Level, LevelFilter, Metadata, Record};

pub struct SimpleLogger;
pub static LOGGER: SimpleLogger = SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        let enable_debug = if let Ok(x) = env::var("MAXIMA_LOG_LEVEL") {
            x == "debug"
        } else {
            false
        };

        let log_level = if enable_debug {
            Level::Debug
        } else {
            Level::Info
        };

        metadata.level() <= log_level
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let level = record.level();
            let color: &str = match level {
                Level::Error => "31", // red
                Level::Warn => "33",  // yellow
                Level::Info => "32",  // green
                Level::Debug => "36", // cyan
                Level::Trace => "33", // yellow
            };

            if level == Level::Error {
                println!(
                    "\u{001b}[{}m{}\u{001b}[37m - [{}:{}] - {}",
                    color,
                    level,
                    record.file_static().unwrap(),
                    record.line().unwrap(),
                    record.args()
                );
            } else {
                println!(
                    "\u{001b}[{}m{}\u{001b}[37m - [{}] - {}",
                    color,
                    level,
                    record.module_path().unwrap(),
                    record.args()
                );
            }
        }
    }

    fn flush(&self) {}
}

pub fn init_logger() {
    if enable_ansi_support::enable_ansi_support().is_err() {
        println!("ANSI Colors are unsupported in your terminal, things might look a bit off!");
    }

    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(LevelFilter::Trace))
        .ok();
}
