extern crate symbolserver;

#[test]
fn test_stuff() {
    use symbolserver::dsym::test;
    test();
    panic!("stuff");
}
