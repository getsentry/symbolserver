//! This exposes the command line interface that the binary uses
use std::fs;
use std::env;
use std::process;

use clap::{App, Arg, SubCommand};
use log;

use super::Result;
use super::sdk::{Sdk, SdkInfo, DumpOptions};
use super::config::Config;
use super::memdbstash::MemDbStash;
use super::api::server::ApiServer;

struct SimpleLogger;

impl log::Log for SimpleLogger {

    fn enabled(&self, metadata: &log::LogMetadata) -> bool {
        metadata.level() <= log::LogLevel::Info
    }

    fn log(&self, record: &log::LogRecord) {
        if self.enabled(record.metadata()) {
            println!("[{}] {}: {}", record.level(),
                record.target(), record.args());
        }
    }
}

fn setup_logging() {
    log::set_logger(|max_log_level| {
        max_log_level.set(log::LogLevelFilter::Warn);
        Box::new(SimpleLogger)
    }).unwrap();
}

/// Main entry point that starts the CLI
pub fn main() {
    setup_logging();
    match execute() {
        Ok(()) => {},
        Err(err) => {
            use std::error::Error;
            println!("error: {}", err);
            let mut cause = err.cause();
            while let Some(the_cause) = cause {
                println!("  caused by: {}", the_cause);
                cause = the_cause.cause();
            }
            process::exit(1);
        }
    }
}

fn execute() -> Result<()> {
    let app = App::new("SymbolServer")
        .author("Sentry")
        .about("Serves up apple system symbols")
        .arg(Arg::with_name("config")
             .long("config")
             .value_name("FILE")
             .help("The path to the config file"))
        .subcommand(
            SubCommand::with_name("sync-symbols"))
        .subcommand(
            SubCommand::with_name("run"))
        .subcommand(
            SubCommand::with_name("convert-sdk")
                .about("Converts an SDK into a memdb file")
                .arg(Arg::with_name("path")
                     .index(1)
                     .value_name("PATH")
                     .required(true)
                     .multiple(true)
                     .help("Path to the support folder"))
                .arg(Arg::with_name("compress")
                     .short("c")
                     .long("compress")
                     .help("Write compressed files instead."))
                .arg(Arg::with_name("output-path")
                     .short("o")
                     .long("output")
                     .help("Where the result should be stored")));
    let matches = app.get_matches();

    let cfg = if let Some(config_path) = matches.value_of("config") {
        Config::load_file(config_path)?
    } else {
        Config::load_default()?
    };

    if let Some(matches) = matches.subcommand_matches("convert-sdk") {
        convert_sdk_action(matches.values_of("path").unwrap().collect(),
                           matches.value_of("output-path").unwrap_or("."),
                           matches.is_present("compress"))?;
    } else if let Some(_matches) = matches.subcommand_matches("run") {
        run_action(&cfg)?;
    } else if let Some(_matches) = matches.subcommand_matches("sync-symbols") {
        sync_symbols_action(&cfg)?;
    }

    Ok(())
}

fn convert_sdk_action(paths: Vec<&str>, output_path: &str, compress: bool)
    -> Result<()>
{
    let dst_base = env::current_dir().unwrap().join(output_path);

    for (idx, path) in paths.iter().enumerate() {
        if idx > 0 {
            println!("");
        }
        let sdk = Sdk::new(&path)?;
        let mut dst = dst_base.join(&sdk.info().memdb_filename());
        if compress {
            dst.set_extension("memdbz");
        }

        println!("SDK {} ({} {}):", sdk.info().name(),
                 sdk.info().version(), sdk.info().build());

        // make sure we close the file at the end, in case we want to
        // re-open it for compressing.
        let f = fs::File::create(&dst)?;
        let options = DumpOptions {
            show_progress_bar: true,
            compress: compress,
            ..Default::default()
        };
        sdk.dump_memdb(f, options)?;
    }

    Ok(())
}

fn sync_symbols_action(config: &Config) -> Result<()> {
    let stash = MemDbStash::new(config)?;
    let status = stash.get_sync_status()?;
    println!("Sync healthy: {:?} (lag: {})", status.is_healthy(), status.lag());

    stash.sync()?;

    let info = SdkInfo::new("iOS", 10, 2, 1, "14D27");
    let memdb = stash.get_memdb(&info)?;
    println!("MemDB: {:?}", memdb.info());

    Ok(())
}

fn run_action(config: &Config) -> Result<()> {
    let api_server = ApiServer::new(config)?;
    api_server.run()?;
    Ok(())
}
