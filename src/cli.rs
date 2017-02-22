use std::fs;
use std::env;
use std::process;

use clap::{App, Arg, SubCommand};

use super::Result;
use super::sdk::{Sdk, DumpOptions};
use super::config::Config;


pub fn main() {
    match execute() {
        Ok(()) => {},
        Err(err) => {
            println!("error: {}", err);
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
            SubCommand::with_name("test"))
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
    println!("{:?}", cfg);

    if let Some(matches) = matches.subcommand_matches("convert-sdk") {
        convert_sdk_action(matches.values_of("path").unwrap().collect(),
                           matches.value_of("output-path").unwrap_or("."),
                           matches.is_present("compress"))?;
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
        let mut dst = dst_base.join(&sdk.memdb_filename());
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
