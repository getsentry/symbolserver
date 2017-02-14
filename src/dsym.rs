use std::io::prelude::*;
use std::process::{Command, Stdio};
use std::io::BufReader;

pub fn call_llvm_nm() {
    let output = Command::new("nm")
                         .arg("-numeric-sort")
                         .arg("-arch")
                         .arg("x86_64")
                         .arg("/Users/haza/Library/Developer/Xcode/DerivedData/SwiftExample-grojhfhmkdyokpfbpjwzjavxvgtz/Build/Products/Release-iphonesimulator/SwiftExample.app.dSYM/Contents/Resources/DWARF/SwiftExample")
                         .stdout(Stdio::piped())
                         .spawn()
                         .expect("failed to execute process");

    let reader = BufReader::new(output.stdout.unwrap());
    for line in reader.lines() {
        let uline = line.unwrap();
        let v: Vec<&str> = uline.trim().split(' ').collect();
        if v.len() != 3 {
            panic!("wrong symbol count");
        }
        let symbol_type = v[1];
        if symbol_type == "t" || symbol_type == "T" {
            let symbol = v[2];
            let address = i64::from_str_radix(v[0], 16).unwrap();
            println!("{}", address);
            println!("{}", symbol);
        }
    }
}

// pub fn call_llvm_nm() -> Child {
//     return Command::new("nm")
//             .arg("-numeric-sort")
//             .arg("-arch")
//             .arg("x86_64")
//             .arg("/Users/haza/Library/Developer/Xcode/DerivedData/SwiftExample-grojhfhmkdyokpfbpjwzjavxvgtz/Build/Products/Release-iphonesimulator/SwiftExample.app.dSYM/Contents/Resources/DWARF/SwiftExample")
//             .stdout(Stdio::piped())
//             .spawn()
//             .expect("failed to execute process");
// }

// /// Represents a reference to a dsym
// #[derive(PartialEq, Debug)]
// pub struct DsymRef {
//     /// Symbolname
//     pub symbol: String,
//     /// Address of the symbol
//     pub address: u64,
// }

// impl DsymRef {

// }

// pub fn call_llvm(process: &mut Child) -> Result<DsymRef> {
//     if let Ok(mut stdout) = process.wait_with_output() {
//         for line in BufReader::new(stdout).lines() {
//             let line = line?;
//             let v: Vec<&str> = line.trim().split(' ').collect();
//             if v.len() != 3 {
//                 panic!("wrong symbol count");
//             }
//             let symbol_type = v[1];
//             if symbol_type == "t" || symbol_type == "T" {
//                 let symbol = v[2];
//                 println!("{}", symbol);

//                 return Ok(DsymRef {
//                     symbol: symbol,
//                     address: u64::from_str_radix(v[0], 16)?
//                 });
//             }
//         }
//     }
// }
