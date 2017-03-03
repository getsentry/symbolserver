//! This exposes the command line interface that the binary uses
use std::fs;
use std::io;
use std::env;
use std::process;
use std::sync::Mutex;

use clap::{App, Arg, SubCommand, ArgMatches};
use chrono::UTC;
use log;

use super::{Result, ResultExt, Error};
use super::sdk::{Sdk, DumpOptions};
use super::config::Config;
use super::memdbstash::{MemDbStash, SyncOptions};
use super::api::server::{ApiServer, BindOptions};

struct SimpleLogger<W: ?Sized> {
    f: Mutex<Box<W>>,
}

impl<W: io::Write + Send + ?Sized> log::Log for SimpleLogger<W> {

    fn enabled(&self, metadata: &log::LogMetadata) -> bool {
        metadata.level() <= log::LogLevel::Info
    }

    fn log(&self, record: &log::LogRecord) {
        let mut f = self.f.lock().unwrap();
        if self.enabled(record.metadata()) {
            writeln!(f, "[{}] [{}] {}: {}",
                     UTC::now(),
                     record.level(),
                     record.target().split(':').next().unwrap(),
                     record.args()).ok();
        }
    }
}

fn setup_logging(config: &Config) -> Result<()> {
    let filter = config.get_log_level_filter()?;
    if filter >= log::LogLevel::Debug {
        env::set_var("RUST_BACKTRACE", "1");
    }

    let f : Box<io::Write + Send> = match config.get_log_filename()? {
        Some(path) => Box::new(fs::File::open(path)?),
        None => Box::new(io::stdout()),
    };
    log::set_logger(|max_log_level| {
        max_log_level.set(filter);
        Box::new(SimpleLogger {
            f: Mutex::new(f),
        })
    }).unwrap();
    Ok(())
}

/// Main entry point that starts the CLI
pub fn main() {
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
            if let Some(backtrace) = err.backtrace() {
                debug!("  Traceback: {:?}", backtrace);
            }
            process::exit(1);
        }
    }
}

fn config_from_matches(matches: &ArgMatches) -> Result<Config> {
    let mut cfg = if let Some(config_path) = matches.value_of("config") {
        Config::load_file(config_path)?
    } else {
        Config::load_default()?
    };

    if let Some(value) = matches.value_of("log_level") {
        let filter = value.parse()
            .map_err(|_| Error::from("Invalid value for log level"))?;
        cfg.set_log_level_filter(filter);
    };

    if let Some(value) = matches.value_of("symbol_dir") {
        cfg.set_symbol_dir(value);
    }

    if let Some(value) = matches.value_of("aws_bucket_url") {
        cfg.set_aws_bucket_url(value);
    }

    if let Some(value) = matches.value_of("aws_region") {
        let region = value.parse()
            .map_err(|_| Error::from("Invalid AWS region"))?;
        cfg.set_aws_region(region);
    }

    Ok(cfg)
}

fn execute() -> Result<()> {
    let app = App::new("sentry-symbolserver")
        .about("This tool implements an Apple SDK processor and server.")
        .arg(Arg::with_name("config")
             .long("config")
             .value_name("FILE")
             .help("The path to the config file"))
        .arg(Arg::with_name("log_level")
             .short("l")
             .long("log-level")
             .value_name("LEVEL")
             .help("Overrides the log level from the config"))
        .arg(Arg::with_name("symbol_dir")
             .short("p")
             .long("symbol-dir")
             .value_name("PATH")
             .help("The path to the symbol directory"))
        .arg(Arg::with_name("aws_bucket_url")
             .long("aws-bucket-url")
             .value_name("URL")
             .help("The bucket URL the sync tool should pull from"))
        .arg(Arg::with_name("aws_region")
             .long("aws-region")
             .value_name("REGION")
             .help("Sets the AWS region the bucket is located in"))
        .subcommand(
            SubCommand::with_name("sync")
                .about("Updates symbols from S3"))
        .subcommand(
            SubCommand::with_name("run")
                .about("Runs the symbol server")
                .arg(Arg::with_name("disable_sync")
                     .long("disable-sync")
                     .help("Disables the background synching"))
                .arg(Arg::with_name("bind")
                     .long("bind")
                     .value_name("ADDR")
                     .help("Bind to a specific address (ip:port)"))
                .arg(Arg::with_name("bind_fd")
                     .long("bind-fd")
                     .value_name("FD")
                     .help("Bind to a specific file descriptor")))
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

    let cfg = config_from_matches(&matches)?;
    setup_logging(&cfg)?;

    if let Some(matches) = matches.subcommand_matches("convert-sdk") {
        convert_sdk_action(matches.values_of("path").unwrap().collect(),
                           matches.value_of("output-path").unwrap_or("."),
                           matches.is_present("compress"))?;
    } else if let Some(matches) = matches.subcommand_matches("run") {
        run_action(&cfg, matches)?;
    } else if let Some(_matches) = matches.subcommand_matches("sync") {
        sync_action(&cfg)?;
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

fn sync_action(config: &Config) -> Result<()> {
    let stash = MemDbStash::new(config)?;
    stash.sync(SyncOptions {
        user_facing: true,
        ..Default::default()
    })?;
    Ok(())
}

fn run_action(config: &Config, matches: &ArgMatches) -> Result<()> {
    let api_server = ApiServer::new(config)?;

    if !matches.is_present("disable_sync") {
        api_server.spawn_sync_thread()?;
    }

    api_server.run(if let Some(addr) = matches.value_of("bind") {
        BindOptions::BindToAddr(addr)
    } else if let Some(fd) = matches.value_of("bind_fd") {
        BindOptions::BindToFd(fd.parse().chain_err(|| "invalid value for file descriptor")?)
    } else {
        BindOptions::UseConfig
    })?;

    Ok(())
}
