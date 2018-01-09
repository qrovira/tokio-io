extern crate tokio_io;
extern crate futures;

use tokio_io::{AsyncRead, AsyncWrite};
use tokio_io::codec::LengthDelimitedCodec;

use futures::{Stream, Sink, Poll};
use futures::Async::*;

use std::io;
use std::collections::VecDeque;

macro_rules! mock {
    ($($x:expr,)*) => {{
        let mut v = VecDeque::new();
        v.extend(vec![$($x),*]);
        Mock { calls: v }
    }};
}


#[test]
fn read_empty_io_yields_nothing() {
    let mock = mock!();
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert_eq!(framed.poll().unwrap(), Ready(None));
}

#[test]
fn read_single_frame_one_packet() {
    let mock = mock! {
        Ok(b"\x00\x00\x00\x09abcdefghi"[..].into()),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert_eq!(framed.poll().unwrap(), Ready(Some(b"abcdefghi"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(None));
}

#[test]
fn read_single_frame_one_packet_little_endian() {
    let mock = mock! {
        Ok(b"\x09\x00\x00\x00abcdefghi"[..].into()),
    };
    let mut codec = LengthDelimitedCodec::new();
    codec.set_length_field_is_big_endian(false);
    let mut framed = mock.framed(codec);

    assert_eq!(framed.poll().unwrap(), Ready(Some(b"abcdefghi"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(None));
}

#[test]
fn read_single_multi_frame_one_packet() {
    let mut data: Vec<u8> = vec![];
    data.extend_from_slice(b"\x00\x00\x00\x09abcdefghi");
    data.extend_from_slice(b"\x00\x00\x00\x03123");
    data.extend_from_slice(b"\x00\x00\x00\x0bhello world");

    let mock = mock! { Ok(data.into()), };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert_eq!(framed.poll().unwrap(), Ready(Some(b"abcdefghi"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(Some(b"123"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(Some(b"hello world"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(None));
}

#[test]
fn read_single_frame_multi_packet() {
    let mock = mock! {
        Ok(b"\x00\x00"[..].into()),
        Ok(b"\x00\x09abc"[..].into()),
        Ok(b"defghi"[..].into()),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert_eq!(framed.poll().unwrap(), Ready(Some(b"abcdefghi"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(None));
}

#[test]
fn read_multi_frame_multi_packet() {
    let mock = mock! {
        Ok(b"\x00\x00"[..].into()),
        Ok(b"\x00\x09abc"[..].into()),
        Ok(b"defghi"[..].into()),
        Ok(b"\x00\x00\x00\x0312"[..].into()),
        Ok(b"3\x00\x00\x00\x0bhello world"[..].into()),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert_eq!(framed.poll().unwrap(), Ready(Some(b"abcdefghi"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(Some(b"123"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(Some(b"hello world"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(None));
}

#[test]
fn read_single_frame_multi_packet_wait() {
    let mock = mock! {
        Ok(b"\x00\x00"[..].into()),
        Err(would_block()),
        Ok(b"\x00\x09abc"[..].into()),
        Err(would_block()),
        Ok(b"defghi"[..].into()),
        Err(would_block()),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert_eq!(framed.poll().unwrap(), NotReady);
    assert_eq!(framed.poll().unwrap(), NotReady);
    assert_eq!(framed.poll().unwrap(), Ready(Some(b"abcdefghi"[..].into())));
    assert_eq!(framed.poll().unwrap(), NotReady);
    assert_eq!(framed.poll().unwrap(), Ready(None));
}

#[test]
fn read_multi_frame_multi_packet_wait() {
    let mock = mock! {
        Ok(b"\x00\x00"[..].into()),
        Err(would_block()),
        Ok(b"\x00\x09abc"[..].into()),
        Err(would_block()),
        Ok(b"defghi"[..].into()),
        Err(would_block()),
        Ok(b"\x00\x00\x00\x0312"[..].into()),
        Err(would_block()),
        Ok(b"3\x00\x00\x00\x0bhello world"[..].into()),
        Err(would_block()),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert_eq!(framed.poll().unwrap(), NotReady);
    assert_eq!(framed.poll().unwrap(), NotReady);
    assert_eq!(framed.poll().unwrap(), Ready(Some(b"abcdefghi"[..].into())));
    assert_eq!(framed.poll().unwrap(), NotReady);
    assert_eq!(framed.poll().unwrap(), NotReady);
    assert_eq!(framed.poll().unwrap(), Ready(Some(b"123"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(Some(b"hello world"[..].into())));
    assert_eq!(framed.poll().unwrap(), NotReady);
    assert_eq!(framed.poll().unwrap(), Ready(None));
}

#[test]
fn read_incomplete_head() {
    let mock = mock! {
        Ok(b"\x00\x00"[..].into()),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert!(framed.poll().is_err());
}

#[test]
fn read_incomplete_head_multi() {
    let mock = mock! {
        Err(would_block()),
        Ok(b"\x00"[..].into()),
        Err(would_block()),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert_eq!(framed.poll().unwrap(), NotReady);
    assert_eq!(framed.poll().unwrap(), NotReady);
    assert!(framed.poll().is_err());
}

#[test]
fn read_incomplete_payload() {
    let mock = mock! {
        Ok(b"\x00\x00\x00\x09ab"[..].into()),
        Err(would_block()),
        Ok(b"cd"[..].into()),
        Err(would_block()),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert_eq!(framed.poll().unwrap(), NotReady);
    assert_eq!(framed.poll().unwrap(), NotReady);
    assert!(framed.poll().is_err());
}

#[test]
fn read_max_frame_len() {
    let mock = mock! {
        Ok(b"\x00\x00\x00\x09abcdefghi"[..].into()),
    };
    let mut codec = LengthDelimitedCodec::new();
    codec.set_max_frame_length(5);
    let mut framed = mock.framed(codec);

    assert_eq!(framed.poll().unwrap_err().kind(), io::ErrorKind::InvalidData);
}

#[test]
fn read_update_max_frame_len_at_rest() {
    let mock = mock! {
        Ok(b"\x00\x00\x00\x09abcdefghi"[..].into()),
        Ok(b"\x00\x00\x00\x09abcdefghi"[..].into()),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert_eq!(framed.poll().unwrap(), Ready(Some(b"abcdefghi"[..].into())));
    framed.codec_mut().set_max_frame_length(5);
    assert_eq!(framed.poll().unwrap_err().kind(), io::ErrorKind::InvalidData);
}

#[test]
fn read_update_max_frame_len_in_flight() {
    let mock = mock! {
        Ok(b"\x00\x00\x00\x09abcd"[..].into()),
        Err(would_block()),
        Ok(b"efghi"[..].into()),
        Ok(b"\x00\x00\x00\x09abcdefghi"[..].into()),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert_eq!(framed.poll().unwrap(), NotReady);
    framed.codec_mut().set_max_frame_length(5);
    assert_eq!(framed.poll().unwrap(), Ready(Some(b"abcdefghi"[..].into())));
    assert_eq!(framed.poll().unwrap_err().kind(), io::ErrorKind::InvalidData);
}

#[test]
fn read_one_byte_length_field() {
    let mock = mock! {
        Ok(b"\x09abcdefghi"[..].into()),
    };
    let mut codec = LengthDelimitedCodec::new();
    codec.set_length_field_len(1);
    let mut framed = mock.framed(codec);

    assert_eq!(framed.poll().unwrap(), Ready(Some(b"abcdefghi"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(None));
}

#[test]
fn read_header_offset() {
    let mock = mock! {
        Ok(b"zzzz\x00\x09abcdefghi"[..].into()),
    };
    let mut codec = LengthDelimitedCodec::new();
    codec.set_length_field_len(2);
    codec.set_length_field_offset(4);
    let mut framed = mock.framed(codec);

    assert_eq!(framed.poll().unwrap(), Ready(Some(b"abcdefghi"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(None));
}

#[test]
fn read_single_multi_frame_one_packet_skip_none_adjusted() {
    let mut data: Vec<u8> = vec![];
    data.extend_from_slice(b"xx\x00\x09abcdefghi");
    data.extend_from_slice(b"yy\x00\x03123");
    data.extend_from_slice(b"zz\x00\x0bhello world");
    let mock = mock! { Ok(data.into()), };
    let mut codec = LengthDelimitedCodec::new();
    codec.set_length_field_len(2);
    codec.set_length_field_offset(2);
    codec.set_num_skip(0);
    codec.set_length_adjustment(4);
    let mut framed = mock.framed(codec);

    assert_eq!(framed.poll().unwrap(), Ready(Some(b"xx\x00\x09abcdefghi"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(Some(b"yy\x00\x03123"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(Some(b"zz\x00\x0bhello world"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(None));
}

#[test]
fn read_single_multi_frame_one_packet_length_includes_head() {
    let mut data: Vec<u8> = vec![];
    data.extend_from_slice(b"\x00\x0babcdefghi");
    data.extend_from_slice(b"\x00\x05123");
    data.extend_from_slice(b"\x00\x0dhello world");
    let mock = mock! { Ok(data.into()), };
    let mut codec = LengthDelimitedCodec::new();
    codec.set_length_field_len(2);
    codec.set_length_adjustment(-2);
    let mut framed = mock.framed(codec);

    assert_eq!(framed.poll().unwrap(), Ready(Some(b"abcdefghi"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(Some(b"123"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(Some(b"hello world"[..].into())));
    assert_eq!(framed.poll().unwrap(), Ready(None));
}

#[test]
fn write_single_frame_length_adjusted() {
    let mock = mock! {
        Ok(b"\x00\x00\x00\x0b"[..].into()),
        Ok(b"abcdefghi"[..].into()),
        Ok(Flush),
    };
    let mut codec = LengthDelimitedCodec::new();
    codec.set_length_adjustment(-2);
    let mut framed = mock.framed(codec);

    assert!(framed.start_send("abcdefghi".into()).unwrap().is_ready());
    assert!(framed.poll_complete().unwrap().is_ready());
    assert!(framed.get_ref().calls.is_empty());
}

#[test]
fn write_nothing_yields_nothing() {
    let mock = mock!();
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert!(framed.poll_complete().unwrap().is_ready());
}

#[test]
fn write_single_frame_one_packet() {
    let mock = mock! {
        Ok(b"\x00\x00\x00\x09"[..].into()),
        Ok(b"abcdefghi"[..].into()),
        Ok(Flush),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert!(framed.start_send("abcdefghi".into()).unwrap().is_ready());
    assert!(framed.poll_complete().unwrap().is_ready());
    assert!(framed.get_ref().calls.is_empty());
}

#[test]
fn write_single_multi_frame_one_packet() {
    let mock = mock! {
        Ok(b"\x00\x00\x00\x09"[..].into()),
        Ok(b"abcdefghi"[..].into()),
        Ok(b"\x00\x00\x00\x03"[..].into()),
        Ok(b"123"[..].into()),
        Ok(b"\x00\x00\x00\x0b"[..].into()),
        Ok(b"hello world"[..].into()),
        Ok(Flush),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert!(framed.start_send("abcdefghi".into()).unwrap().is_ready());
    assert!(framed.start_send("123".into()).unwrap().is_ready());
    assert!(framed.start_send("hello world".into()).unwrap().is_ready());
    assert!(framed.poll_complete().unwrap().is_ready());
    assert!(framed.get_ref().calls.is_empty());
}

#[test]
fn write_single_multi_frame_multi_packet() {
    let mock = mock! {
        Ok(b"\x00\x00\x00\x09"[..].into()),
        Ok(b"abcdefghi"[..].into()),
        Ok(Flush),
        Ok(b"\x00\x00\x00\x03"[..].into()),
        Ok(b"123"[..].into()),
        Ok(Flush),
        Ok(b"\x00\x00\x00\x0b"[..].into()),
        Ok(b"hello world"[..].into()),
        Ok(Flush),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert!(framed.start_send("abcdefghi".into()).unwrap().is_ready());
    assert!(framed.poll_complete().unwrap().is_ready());
    assert!(framed.start_send("123".into()).unwrap().is_ready());
    assert!(framed.poll_complete().unwrap().is_ready());
    assert!(framed.start_send("hello world".into()).unwrap().is_ready());
    assert!(framed.poll_complete().unwrap().is_ready());
    assert!(framed.get_ref().calls.is_empty());
}

#[test]
fn write_single_frame_would_block() {
    let mock = mock! {
        Err(would_block()),
        Ok(b"\x00\x00"[..].into()),
        Err(would_block()),
        Ok(b"\x00\x09"[..].into()),
        Ok(b"abcdefghi"[..].into()),
        Ok(Flush),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert!(framed.start_send("abcdefghi".into()).unwrap().is_ready());
    assert!(!framed.poll_complete().unwrap().is_ready());
    assert!(!framed.poll_complete().unwrap().is_ready());
    assert!(framed.poll_complete().unwrap().is_ready());

    assert!(framed.get_ref().calls.is_empty());
}

#[test]
fn write_single_frame_little_endian() {
    let mock = mock! {
        Ok(b"\x09\x00\x00\x00"[..].into()),
        Ok(b"abcdefghi"[..].into()),
        Ok(Flush),
    };
    let mut codec = LengthDelimitedCodec::new();
    codec.set_length_field_is_big_endian(false);
    let mut framed = mock.framed(codec);

    assert!(framed.start_send("abcdefghi".into()).unwrap().is_ready());
    assert!(framed.poll_complete().unwrap().is_ready());
    assert!(framed.get_ref().calls.is_empty());
}


#[test]
fn write_single_frame_with_short_length_field() {
    let mock = mock! {
        Ok(b"\x09"[..].into()),
        Ok(b"abcdefghi"[..].into()),
        Ok(Flush),
    };
    let mut codec = LengthDelimitedCodec::new();
    codec.set_length_field_len(1);
    let mut framed = mock.framed(codec);

    assert!(framed.start_send("abcdefghi".into()).unwrap().is_ready());
    assert!(framed.poll_complete().unwrap().is_ready());
    assert!(framed.get_ref().calls.is_empty());
}

#[test]
fn write_max_frame_len() {
    let mock = mock! {};
    let mut codec = LengthDelimitedCodec::new();
    codec.set_max_frame_length(5);
    let mut framed = mock.framed(codec);

    assert_eq!(framed.start_send("abcdef".into()).unwrap_err().kind(), io::ErrorKind::InvalidInput);
    assert!(framed.get_ref().calls.is_empty());
}

#[test]
fn write_update_max_frame_len_at_rest() {
    let mock = mock! {
        Ok(b"\x00\x00\x00\x06"[..].into()),
        Ok(b"abcdef"[..].into()),
        Ok(Flush),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert!(framed.start_send("abcdef".into()).unwrap().is_ready());
    assert!(framed.poll_complete().unwrap().is_ready());
    framed.codec_mut().set_max_frame_length(5);
    assert_eq!(framed.start_send("abcdef".into()).unwrap_err().kind(), io::ErrorKind::InvalidInput);
    assert!(framed.get_ref().calls.is_empty());
}

#[test]
fn write_update_max_frame_len_in_flight() {
    let mock = mock! {
        Ok(b"\x00\x00\x00\x06"[..].into()),
        Ok(b"ab"[..].into()),
        Err(would_block()),
        Ok(b"cdef"[..].into()),
        Ok(Flush),
    };
    let mut framed = mock.framed(LengthDelimitedCodec::new());

    assert!(framed.start_send("abcdef".into()).unwrap().is_ready());
    assert!(!framed.poll_complete().unwrap().is_ready());
    framed.codec_mut().set_max_frame_length(5);
    assert!(framed.poll_complete().unwrap().is_ready());
    assert_eq!(framed.start_send("abcdef".into()).unwrap_err().kind(), io::ErrorKind::InvalidInput);
    assert!(framed.get_ref().calls.is_empty());
}

// ===== Test utils =====

fn would_block() -> io::Error {
    io::Error::new(io::ErrorKind::WouldBlock, "would block")
}

struct Mock {
    calls: VecDeque<io::Result<Op>>,
}

enum Op {
    Data(Vec<u8>),
    Flush,
}

use self::Op::*;

impl io::Read for Mock {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        match self.calls.pop_front() {
            Some(Ok(Op::Data(data))) => {
                debug_assert!(dst.len() >= data.len());
                dst[..data.len()].copy_from_slice(&data[..]);
                Ok(data.len())
            }
            Some(Ok(_)) => panic!(),
            Some(Err(e)) => Err(e),
            None => Ok(0),
        }
    }
}

impl AsyncRead for Mock {
}

impl io::Write for Mock {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        match self.calls.pop_front() {
            Some(Ok(Op::Data(data))) => {
                let len = data.len();
                assert!(src.len() >= len, "expect={:?}; actual={:?}", data, src);
                assert_eq!(&data[..], &src[..len]);
                Ok(len)
            }
            Some(Ok(_)) => panic!(),
            Some(Err(e)) => Err(e),
            None => Ok(0),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.calls.pop_front() {
            Some(Ok(Op::Flush)) => {
                Ok(())
            }
            Some(Ok(_)) => panic!(),
            Some(Err(e)) => Err(e),
            None => Ok(()),
        }
    }
}

impl AsyncWrite for Mock {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        Ok(Ready(()))
    }
}

impl<'a> From<&'a [u8]> for Op {
    fn from(src: &'a [u8]) -> Op {
        Op::Data(src.into())
    }
}

impl From<Vec<u8>> for Op {
    fn from(src: Vec<u8>) -> Op {
        Op::Data(src)
    }
}
