use std::process;

use clap::{App, Arg, SubCommand};

use super::Result;
use super::sdk::Sdk;


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
                     .help("Path to the support folder"))
                .arg(Arg::with_name("output-path")
                     .short("o")
                     .long("output")
                     .help("Where the result should be stored")));
    let matches = app.get_matches();

    if let Some(matches) = matches.subcommand_matches("convert-sdk") {
        convert_sdk_action(matches.value_of("path").unwrap(),
                           matches.value_of("output-path").unwrap_or("."))?;
    }

    Ok(())
}

fn convert_sdk_action(path: &str, output_path: &str) -> Result<()> {
    let sdk = Sdk::new(&path)?;
    println!("Processing SDK");
    println!("  Name:       {}", sdk.info().name());
    println!("  Version:    {} [{}]", sdk.info().version(), sdk.info().build());
    println!("  MemDB File: {}", sdk.memdb_filename());
    sdk.dump_memdb_to_folder(output_path)?;
    println!("Exported!");
    Ok(())
}
