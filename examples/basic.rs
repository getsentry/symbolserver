extern crate symbolserver;

use std::env;
use std::fs;
use std::path::Path;

use symbolserver::sdk::SdkProcessor;
use symbolserver::memdb::MemDbBuilder;
use symbolserver::Result;

fn do_main() -> Result<()> {
    let args : Vec<_> = env::args().collect();
    let p = Path::new(&args[1]);
    let out = fs::File::create("/tmp/test.memdb")?;
    let sdk_proc = SdkProcessor::new(p)?;
    let mut builder = MemDbBuilder::new(out, sdk_proc.info())?;

    for obj_res in sdk_proc.objects()? {
        let (filename, obj) = obj_res?;
        builder.write_object(&obj, Some(&filename))?;
    }
    builder.flush()?;

    Ok(())
}

fn main() {
    do_main().unwrap();
}
