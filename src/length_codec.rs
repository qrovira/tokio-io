use codec::{Encoder,Decoder};

use bytes::{Buf, BufMut, BytesMut, BigEndian, LittleEndian, IntoBuf};

use std::io::{self, Cursor};

use std::cmp;

/// Some codec
#[derive(Debug)]
pub struct LengthDelimitedCodec {
    // Maximum frame length
    max_frame_len: usize,

    // Number of bytes representing the field length
    length_field_len: usize,

    // Number of bytes in the header before the length field
    length_field_offset: usize,

    // Adjust the length specified in the header field by this amount
    length_adjustment: isize,

    // Total number of bytes to skip before reading the payload, if not set,
    // `length_field_len + length_field_offset`
    num_skip: Option<usize>,

    // Length field byte order (little or big endian)
    length_field_is_big_endian: bool,

    // Read state
    state: DecodeState,
}

#[derive(Debug)]
enum DecodeState {
    Head,
    Data(usize),
}

impl LengthDelimitedCodec {
    /// New
    pub fn new() -> Self {
        LengthDelimitedCodec {
            // Default max frame length of 8MB
            max_frame_len: 8 * 1_024 * 1_024,

            // Default byte length of 4
            length_field_len: 4,

            // Default to the header field being at the start of the header.
            length_field_offset: 0,

            length_adjustment: 0,

            // Total number of bytes to skip before reading the payload, if not set,
            // `length_field_len + length_field_offset`
            num_skip: None,

            // Default to reading the length field in network (big) endian.
            length_field_is_big_endian: true,

            // Initial decoder state
            state: DecodeState::Head,
        }
    }
    /// stub
    pub fn max_frame_length(&self) -> usize {
        self.max_frame_len
    }

    /// stub
    pub fn set_max_frame_length(&mut self, len: usize) {
        self.max_frame_len = len;
    }

    /// stub
    pub fn length_field_len(&self) -> usize {
        self.length_field_len
    }

    /// stub
    pub fn set_length_field_len(&mut self, len: usize) {
        self.length_field_len = len;
    }

    /// stub
    pub fn length_field_offset(&self) -> usize {
        self.length_field_offset
    }

    /// stub
    pub fn set_length_field_offset(&mut self, len: usize) {
        self.length_field_offset = len;
    }

    /// stub
    pub fn length_adjustment(&self) -> isize {
        self.length_adjustment
    }

    /// stub
    pub fn set_length_adjustment(&mut self, len: isize) {
        self.length_adjustment = len;
    }

    /// stub
    pub fn num_skip(&self) -> usize {
        self.num_skip.unwrap_or(self.length_field_offset + self.length_field_len)
    }

    /// stub
    pub fn set_num_skip(&mut self, len: usize) {
        self.num_skip = Some(len);
    }

    /// stub
    pub fn length_field_is_big_endian(&self) -> bool {
        self.length_field_is_big_endian
    }

    /// stub
    pub fn set_length_field_is_big_endian(&mut self, is_big_endian: bool) {
        self.length_field_is_big_endian = is_big_endian;
    }

    fn decode_head(&mut self, src: &mut BytesMut) -> io::Result<Option<usize>> {
        let head_len = self.num_head_bytes();
        let field_len = self.length_field_len;

        if src.len() < head_len {
            // Not enough data
            return Ok(None);
        }

        let n = {
            let mut src = Cursor::new(&mut *src);

            // Skip the required bytes
            src.advance(self.length_field_offset);

            // match endianess
            let n = if self.length_field_is_big_endian {
                src.get_uint::<BigEndian>(field_len)
            } else {
                src.get_uint::<LittleEndian>(field_len)
            };

            if n > self.max_frame_len as u64 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Frame too big"));
            }

            // The check above ensures there is no overflow
            let n = n as usize;

            // Adjust `n` with bounds checking
            let n = if self.length_adjustment < 0 {
                n.checked_sub(-self.length_adjustment as usize)
            } else {
                n.checked_add(self.length_adjustment as usize)
            };

            // Error handling
            match n {
                Some(n) => n,
                None => return Err(io::Error::new(io::ErrorKind::InvalidInput, "provided length would overflow after adjustment")),
            }
        };

        let num_skip = self.num_skip();

        if num_skip > 0 {
            let _ = src.split_to(num_skip);
        }

        // Ensure that the buffer has enough space to read the incoming
        // payload
        src.reserve(n);

        return Ok(Some(n));
    }

    fn decode_data(&self, n: usize, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
        // At this point, the buffer has already had the required capacity
        // reserved. All there is to do is read.
        if src.len() < n {
            return Ok(None);
        }

        Ok(Some(src.split_to(n)))
    }

    fn num_head_bytes(&self) -> usize {
        let num = self.length_field_offset + self.length_field_len;
        cmp::max(num, self.num_skip.unwrap_or(0))
    }
}

impl Decoder for LengthDelimitedCodec {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
        let n = match self.state {
            DecodeState::Head => {
                match try!(self.decode_head(src)) {
                    Some(n) => {
                        self.state = DecodeState::Data(n);
                        n
                    }
                    None => return Ok(None),
                }
            }
            DecodeState::Data(n) => n,
        };

        match try!(self.decode_data(n, src)) {
            Some(data) => {
                // Update the decode state
                self.state = DecodeState::Head;

                // Make sure the buffer has enough space to read the next head
                src.reserve(self.num_head_bytes());

                Ok(Some(data))
            }
            None => Ok(None),
        }
    }
}

impl Encoder for LengthDelimitedCodec {
    type Item = BytesMut;
    type Error = io::Error;

    fn encode(&mut self, item: BytesMut, dst: &mut BytesMut) -> Result<(), io::Error> {
        let buf = item.into_buf();
        let n = buf.remaining();

        if n > self.max_frame_len {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Frame too big"));
        }

        // Adjust `n` with bounds checking
        let n = if self.length_adjustment < 0 {
            n.checked_add(-self.length_adjustment as usize)
        } else {
            n.checked_sub(self.length_adjustment as usize)
        };

        // Error handling
        let n = match n {
            Some(n) => n,
            None => return Err(io::Error::new(io::ErrorKind::InvalidInput, "provided length would overflow after adjustment")),
        };

        if self.length_field_is_big_endian {
            dst.put_uint::<BigEndian>(n as u64, self.length_field_len);
        } else {
            dst.put_uint::<LittleEndian>(n as u64, self.length_field_len);
        }

        dst.extend_from_slice(buf.bytes());

        Ok(())
    }
}

