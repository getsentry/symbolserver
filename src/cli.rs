use std::fs;
use std::env;
use std::process;

use clap::{App, Arg, SubCommand};

use super::Result;
use super::sdk::{Sdk, DumpOptions};


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
        .subcommand(
            SubCommand::with_name("convert-sdk")
                .about("Converts an SDK into a memdb file")
                .arg(Arg::with_name("path")
                     .index(1)
                     .value_name("PATH")
                     .required(true)
                     .multiple(true)
                     .help("Path to the support folder"))
                .arg(Arg::with_name("output-path")
                     .short("o")
                     .long("output")
                     .help("Where the result should be stored")));
    let matches = app.get_matches();

    if let Some(matches) = matches.subcommand_matches("convert-sdk") {
        convert_sdk_action(matches.values_of("path").unwrap().collect(),
                           matches.value_of("output-path").unwrap_or("."))?;
    }

    Ok(())
}

fn convert_sdk_action(paths: Vec<&str>, output_path: &str) -> Result<()> {
    let dst_base = env::current_dir().unwrap().join(output_path);

    for (idx, path) in paths.iter().enumerate() {
        if idx > 0 {
            println!("");
        }
        let sdk = Sdk::new(&path)?;
        let dst = dst_base.join(&sdk.memdb_filename());
        println!("SDK {} ({} {}):", sdk.info().name(),
                 sdk.info().version(), sdk.info().build());
        let f = fs::File::create(dst)?;
        sdk.dump_memdb(f, DumpOptions { ..Default::default() })?;
    }

    Ok(())
}
