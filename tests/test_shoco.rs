extern crate symbolserver;

use symbolserver::shoco::{compress, decompress};


#[test]
fn test_basics() {
    let input_str = "___102-[ICCloudContext fetchDatabaseChangesOperation:finishedWithServerChangeToken:error:completionHandler:]";
    let input_bytes = input_str.as_bytes();
    let compressed = compress(input_bytes);
    assert!(compressed.len() < input_bytes.len());
    let decompressed = decompress(compressed.as_slice());
    assert_eq!(input_bytes, decompressed.as_slice());
}
