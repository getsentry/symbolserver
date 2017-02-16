extern crate symbolserver;

use symbolserver::dsym::SymbolIterator;

#[test]
fn test_sym_iter() {
    let mut iter = SymbolIterator::new("arm64", "/Users/mitsuhiko/Library/Developer/Xcode/iOS DeviceSupport/10.2 (14C92)/Symbols/System/Library/CoreServices/Encodings/libKoreanConverter.dylib").unwrap();
    for item in iter {
        println!("{:?}", item);
    }
}
