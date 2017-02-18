extern crate symbolserver;

use std::env;
use std::path::Path;

use symbolserver::sdk::SdkProcessor;
use symbolserver::Result;

fn do_main() -> Result<()> {
    let args : Vec<_> = env::args().collect();
    let p = Path::new(&args[1]);
    let sdk_proc = SdkProcessor::new(p)?;
    println!("{:?}", sdk_proc.info());
    for obj_res in sdk_proc.objects()? {
        let (filename, obj) = obj_res?;
        for var in obj.variants() {
            println!("  {} {:?} [{}]", filename, var.name(), var.arch());
            let mut syms = obj.symbols(var.arch())?;
            for (addr, sym) in syms.iter() {
                println!("    {:016x}  {}", addr, sym);
            }
        }
    }
    Ok(())
}

fn main() {
    do_main().unwrap();
}
