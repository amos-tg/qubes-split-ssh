use super::{
    MsgHeader,
    flags::NONE,
};

#[test]
fn msg_header_test() {
    const VAL: u64 = 3245;
    assert_eq!(
        {
            let mut header = MsgHeader::new();
            header.update(VAL, NONE);
            MsgHeader::len(&header)
        },
        VAL,
        "MsgHeader incorrectly created or evaluated the length \
        of the value."
    );
}
