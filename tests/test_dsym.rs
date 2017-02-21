extern crate symbolserver;
extern crate uuid;

use symbolserver::dsym::Object;

const DEMO_DYLIB: &'static [u8] = include_bytes!("res/libKoreanConverter.dylib");

#[test]
fn test_basics() {
    let obj = Object::from_slice(DEMO_DYLIB).unwrap();
    let variants = obj.variants();
    assert_eq!(variants.len(), 2);

    let mut vars : Vec<_> = variants.iter().collect();
    vars.sort_by_key(|x| x.arch());
    assert_eq!(vars[0].arch(), "arm64");
    assert_eq!(vars[0].name(), Some("/System/Library/CoreServices/Encodings/libKoreanConverter.dylib"));
    assert_eq!(vars[0].uuid(), Some("fe6d76d4-8c3a-3a9a-9f63-f4a475501f1b".parse().unwrap()));
    assert_eq!(vars[0].vmaddr(), 6804459520);
    assert_eq!(vars[0].vmsize(), 143360);

    assert_eq!(vars[1].arch(), "armv7s");
    assert_eq!(vars[1].name(), Some("/System/Library/CoreServices/Encodings/libKoreanConverter.dylib"));
    assert_eq!(vars[1].uuid(), Some("383fbe5b-e16e-362f-8937-ed303ab58e72".parse().unwrap()));
    assert_eq!(vars[1].vmaddr(), 744677376);
    assert_eq!(vars[1].vmsize(), 135168);
}

#[test]
fn test_symbols() {
    let obj = Object::from_slice(DEMO_DYLIB).unwrap();
    let variants = obj.variants();

    assert_eq!(variants.len(), 2);
    for var in variants {
        let mut symbols = obj.symbols(var.arch()).unwrap();
        let mut count = 0;
        for (addr, sym) in symbols.iter() {
            if sym == "___CFFromMacKoreanLen" {
                if var.arch() == "arm64" {
                    assert_eq!(addr, 6804482832);
                } else {
                    assert_eq!(addr, 744692588);
                }
            }
            count += 1;
        }
        assert_eq!(count, 15);
    }
}
