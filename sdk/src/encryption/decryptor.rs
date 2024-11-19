use bytes::{Buf, BufMut, BytesMut};
use dare::{DAREDecryptor, DAREHeader, HEADER_SIZE, TAG_SIZE};
use futures_core::ready;
use pin_project::pin_project;
use std::io::{Error, ErrorKind};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncWrite;

pub struct Filter {
    pub offset: u64,
    pub length: u64,
    pub consumed: u64,
}

#[pin_project(project = DecryptWriterStateProj)]
enum DecryptWriterState {
    ReadingHeader,
    Decrypting(DAREHeader),
    Writing,
}

#[pin_project]
pub struct DecryptWriter<W: AsyncWrite + Unpin> {
    #[pin]
    inner: W,
    decryptor: DAREDecryptor,
    #[pin]
    state: DecryptWriterState,
    buffer: BytesMut,    // Internal buffer for incoming data
    decrypted: BytesMut, // Buffer for decrypted data
    is_writing: bool,    // Indicates if a write to the inner write is happening
    should_filter: bool, // Indicates if the decrypted content should be filtered

    // proprieties used in case of filtering
    offset: u64,    // bytes less than offset should be ignored
    consumed: u64,  // how many bytes we have consumed from original content
    remaining: u64, // how many bytes left to return
}

impl<W: AsyncWrite + Unpin> DecryptWriter<W> {
    pub fn new(inner: W, decryptor: DAREDecryptor) -> Self {
        Self {
            inner,
            decryptor,
            state: DecryptWriterState::ReadingHeader,
            buffer: BytesMut::new(),
            decrypted: BytesMut::new(),
            is_writing: false,
            should_filter: false,
            offset: 0,
            consumed: 0,
            remaining: 0,
        }
    }

    pub fn with_filter(inner: W, decryptor: DAREDecryptor, filter: Filter) -> Self {
        Self {
            inner,
            decryptor,
            state: DecryptWriterState::ReadingHeader,
            buffer: BytesMut::new(),
            decrypted: BytesMut::new(),
            is_writing: false,
            should_filter: true,
            offset: filter.offset,
            consumed: filter.consumed,
            remaining: filter.length,
        }
    }

    fn filter_bytes<'a>(&mut self, plaintext: &'a [u8]) -> &'a [u8] {
        if !self.should_filter {
            return plaintext;
        }

        let plaintext_size = plaintext.len() as u64;

        // We haven't reached offset yet, we must ignore the decrypted content
        if self.consumed + plaintext_size <= self.offset {
            self.consumed += plaintext_size;
            return &plaintext[plaintext.len()..];
        }

        // We reached offset, so we take the bytes from offset up to the end of package
        //
        // +---------------------------------+
        // |  DISCARD  |      GRAB THIS      |
        // +---------------------------------+
        // |           |
        // consumed    offset
        //
        let plaintext_within_range = &plaintext[(self.offset - self.consumed) as usize..];
        let plaintext_within_range_size = plaintext_within_range.len() as u64;

        // if grabbed fewer bytes than the remaining bytes to take, we return it all
        //
        // +---------------------------------+-----------------------
        // |  DISCARD  |      GRAB THIS      |       NEXT PACKAGE
        // +---------------------------------+-----------------------
        // |           |                                   |
        // consumed    offset                              remaining
        //
        if plaintext_within_range_size <= self.remaining {
            self.consumed += plaintext_size;
            self.offset = self.consumed;
            self.remaining -= plaintext_within_range_size;
            return plaintext_within_range;
        }

        // if not, we must take up to the remaining
        //
        // +---------------------------------+
        // |  DISCARD  | GRAB THIS | DISCARD |
        // +---------------------------------+
        // |           |           |
        // consumed    offset      remaining
        //
        let plaintext_within_range = &plaintext_within_range[..self.remaining as usize];
        let plaintext_within_range_size = plaintext_within_range.len() as u64;
        self.consumed += plaintext_size;
        self.offset = self.consumed;
        self.remaining -= plaintext_within_range_size;

        plaintext_within_range
    }
}

impl<R: AsyncWrite + Unpin> AsyncWrite for DecryptWriter<R> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        let this = self.as_mut().project();

        // Fill the internal buffer only if we are not in the middle of writing
        if !*this.is_writing {
            this.buffer.put_slice(buf);
        }

        loop {
            let mut this = self.as_mut().project();

            if this.buffer.is_empty() && this.decrypted.is_empty() {
                break;
            }

            match this.state.as_mut().project() {
                DecryptWriterStateProj::ReadingHeader => {
                    // if our internal buffer is not big enough to read the header of a package,
                    // we request more data
                    if this.buffer.len() < HEADER_SIZE {
                        return Poll::Ready(Ok(buf.len()));
                    }

                    let header = this.buffer.split_to(HEADER_SIZE);

                    let dare_header = DAREHeader::from_bytes(header.as_ref())
                        .map_err(|err| Error::new(ErrorKind::InvalidData, err.to_string()))?;
                    this.state.set(DecryptWriterState::Decrypting(dare_header));
                }
                DecryptWriterStateProj::Decrypting(dare_header) => {
                    // if our internal buffer is not big enough to read the rest of the package,
                    // we request more data
                    if this.buffer.len() < dare_header.payload_size() as usize + TAG_SIZE {
                        return Poll::Ready(Ok(buf.len()));
                    }

                    let message = this
                        .buffer
                        .split_to(dare_header.payload_size() as usize + TAG_SIZE);

                    let decrypted = this
                        .decryptor
                        .decrypt(&dare_header.to_bytes()[..], message.as_ref())
                        .map_err(|err| Error::new(ErrorKind::InvalidData, err.to_string()))?;

                    #[allow(clippy::drop_non_drop)]
                    drop(this);
                    let decrypted = self.filter_bytes(&decrypted);
                    let mut this = self.as_mut().project();

                    this.decrypted.put_slice(decrypted);
                    this.state.set(DecryptWriterState::Writing);
                    *this.is_writing = true;
                }
                DecryptWriterStateProj::Writing => {
                    match ready!(this.inner.poll_write(cx, this.decrypted)) {
                        Ok(n) => {
                            this.decrypted.advance(n);
                            if this.decrypted.is_empty() {
                                this.state.set(DecryptWriterState::ReadingHeader);
                                *this.is_writing = false;
                            }
                        }
                        Err(err) => return Poll::Ready(Err(err)),
                    };
                }
            }
        }

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        let this = self.as_mut().project();
        if this.buffer.is_empty() {
            return Poll::Ready(Ok(()));
        }

        let header = this.buffer.split_to(HEADER_SIZE);

        let dare_header = DAREHeader::from_bytes(header.as_ref())
            .map_err(|err| Error::new(ErrorKind::InvalidData, err.to_string()))?;
        let message = this
            .buffer
            .split_to(dare_header.payload_size() as usize + TAG_SIZE);

        let decrypted = this
            .decryptor
            .decrypt(&dare_header.to_bytes()[..], message.as_ref())
            .map_err(|err| Error::new(ErrorKind::InvalidData, err.to_string()))?;

        match ready!(this.inner.poll_write(cx, &decrypted)) {
            Ok(_) => Poll::Ready(Ok(())),
            Err(err) => Poll::Ready(Err(err)),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use crate::encryption::decryptor::{DecryptWriter, Filter};
    use dare::{CipherSuite, DAREDecryptor, DAREEncryptor};
    use std::io::Cursor;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    pub async fn test_async_decryption() {
        let key = [0u8; 32]; // In practice, use a secure random key

        let plaintext = b"a".repeat(200000).to_vec();

        // Encryption
        let mut encryptor =
            DAREEncryptor::new(key, CipherSuite::AES256GCM).expect("should not fail");
        let mut encrypted = Cursor::new(Vec::new());
        encryptor
            .encrypt_stream(&mut Cursor::new(&plaintext), &mut encrypted)
            .await
            .unwrap();

        // Decryption
        let decryptor = DAREDecryptor::new(key);
        let mut destination = Cursor::new(Vec::new());
        let mut writer = DecryptWriter::new(&mut destination, decryptor);
        writer.write_all(&encrypted.into_inner()).await.unwrap();

        // assertion
        let bytes = destination.into_inner();
        assert_eq!(plaintext.len(), bytes.len());
        assert_eq!(plaintext, bytes);
    }

    #[tokio::test]
    async fn test_async_decryption_with_filter() {
        let key = [0u8; 32]; // In practice, use a secure random key

        // Let's generate a content of 200,000 bytes, that repeats itself every 5 bytes.
        // This content requires 4 packages (20,000 / 65536 ) for encryption.
        //
        // 0               65535              131071              196607    199999
        // +-------------------+---------------------------------------+---------+
        // |       64KiB       |       64KiB       |       64KiB       | 3392 B  |
        // +-------------------+---------------------------------------+---------+
        // abcde.............deabcdea............eabcdea.............abcdea......e
        let plaintext = b"abcde".repeat(40000).to_vec();

        // Encryption
        let mut encryptor =
            DAREEncryptor::new(key, CipherSuite::AES256GCM).expect("should not fail");
        let mut encrypted = Vec::new();
        let mut plaintext_cursor = Cursor::new(&plaintext);
        encryptor
            .encrypt_stream(&mut plaintext_cursor, &mut encrypted)
            .await
            .unwrap();

        struct TestSpec {
            offset: u64,
            length: u64,
            expected: &'static str,
        }

        let tests = vec![
            TestSpec {
                offset: 0,
                length: 5,
                expected: "abcde",
            },
            TestSpec {
                offset: 0,
                length: 6,
                expected: "abcdea",
            },
            TestSpec {
                offset: 65533,
                length: 3,
                expected: "dea",
            },
            TestSpec {
                offset: 65533,
                length: 8,
                expected: "deabcdea",
            },
            TestSpec {
                offset: 69999,
                length: 1,
                expected: "e",
            },
            TestSpec {
                offset: 131069,
                length: 7,
                expected: "eabcdea",
            },
            TestSpec {
                offset: 196605,
                length: 6,
                expected: "abcdea",
            },
            TestSpec {
                offset: 199999,
                length: 1,
                expected: "e",
            },
        ];

        for test in tests {
            let decryptor = DAREDecryptor::new(key);
            let mut decrypted = Vec::new();
            let decrypted_cursor = Cursor::new(&mut decrypted);
            let mut writer = DecryptWriter::with_filter(
                decrypted_cursor,
                decryptor,
                Filter {
                    offset: test.offset,
                    length: test.length,
                    consumed: 0,
                },
            );
            tokio::io::copy(&mut Cursor::new(&mut encrypted), &mut writer)
                .await
                .unwrap();

            assert_eq!(
                test.expected.len(),
                decrypted.len(),
                "test case (len, offset) = ({}, {})",
                test.length,
                test.offset
            );
            assert_eq!(test.expected.as_bytes(), decrypted);
        }
    }
}
