use super::MsgHeader;

#[test]
fn msg_header_test() {
    const VAL: u64 = 3245;
    assert_eq!(
        {
            let header = MsgHeader::new(VAL);
            MsgHeader::len(header.0)
        },
        VAL,
        "MsgHeader incorrectly created or evaluated the length \
        of the value."
    );
}
