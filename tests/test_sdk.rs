extern crate libsymbolserver;

use std::path::Path;

use libsymbolserver::sdk::SdkInfo;

#[test]
fn test_sdk_info_parse_ios() {
    let info = SdkInfo::from_path(Path::new("/Users/mitsuhiko/Library/Developer/Xcode/iOS DeviceSupport/10.2 (14C92)")).unwrap();
    assert_eq!(info.name(), "iOS");
    assert_eq!(info.version_major(), 10);
    assert_eq!(info.version_minor(), 2);
    assert_eq!(info.version_patchlevel(), 0);
    assert_eq!(info.build(), Some("14C92"));
}

#[test]
fn test_sdk_info_parse_ios_patchlevel() {
    let info = SdkInfo::from_path(Path::new("/Users/mitsuhiko/Library/Developer/Xcode/iOS DeviceSupport/10.2.3 (14C93)")).unwrap();
    assert_eq!(info.name(), "iOS");
    assert_eq!(info.version_major(), 10);
    assert_eq!(info.version_minor(), 2);
    assert_eq!(info.version_patchlevel(), 3);
    assert_eq!(info.build(), Some("14C93"));
}

#[test]
fn test_sdk_info_parse_ios_patchlevel_ext() {
    let info = SdkInfo::from_path(Path::new("/Users/mitsuhiko/Library/Developer/Xcode/iOS DeviceSupport/10.2.3 (14C93).zip")).unwrap();
    assert_eq!(info.name(), "iOS");
    assert_eq!(info.version_major(), 10);
    assert_eq!(info.version_minor(), 2);
    assert_eq!(info.version_patchlevel(), 3);
    assert_eq!(info.build(), Some("14C93"));
}

#[test]
fn test_sdk_info_parse_ios_patchlevel_ext_memdb() {
    let info = SdkInfo::from_path(Path::new("iOS_10.2.3_14C93.memdb")).unwrap();
    assert_eq!(info.name(), "iOS");
    assert_eq!(info.version_major(), 10);
    assert_eq!(info.version_minor(), 2);
    assert_eq!(info.version_patchlevel(), 3);
    assert_eq!(info.build(), Some("14C93"));
}

#[test]
fn test_sdk_info_parse_tvos_patchlevel_ext() {
    let info = SdkInfo::from_path(Path::new("/Users/mitsuhiko/Library/Developer/Xcode/tvOS DeviceSupport/2.2.3 (14C93).zip")).unwrap();
    assert_eq!(info.name(), "tvOS");
    assert_eq!(info.version_major(), 2);
    assert_eq!(info.version_minor(), 2);
    assert_eq!(info.version_patchlevel(), 3);
    assert_eq!(info.build(), Some("14C93"));
}

#[test]
fn test_sdk_info_parse_filename() {
    let info = SdkInfo::from_filename("iOS_10.2.3_14C93.memdb").unwrap();
    assert_eq!(info.name(), "iOS");
    assert_eq!(info.version_major(), 10);
    assert_eq!(info.version_minor(), 2);
    assert_eq!(info.version_patchlevel(), 3);
    assert_eq!(info.build(), Some("14C93"));
}

#[test]
fn test_sdk_info_parse_filename_no_build() {
    let info = SdkInfo::from_filename("iOS_10.2.3.memdb").unwrap();
    assert_eq!(info.name(), "iOS");
    assert_eq!(info.version_major(), 10);
    assert_eq!(info.version_minor(), 2);
    assert_eq!(info.version_patchlevel(), 3);
    assert_eq!(info.build(), None);
}
