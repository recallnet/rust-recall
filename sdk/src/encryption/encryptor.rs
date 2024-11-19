use dare::{DAREEncryptor, MAX_PAYLOAD_SIZE};
use futures_core::ready;
use pin_project::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

#[pin_project]
pub struct EncryptReader<R: AsyncRead> {
    #[pin]
    inner: R,
    encryptor: DAREEncryptor,

    buffer: Vec<u8>,       // Buffer for encrypted data
    pos: usize,            // How much encrypted data was consumed
    chunk_buffer: Vec<u8>, // Buffer to accumulate chunk-size reads
    chunk_filled: usize,   // How much of the chunk buffer is filled
}

impl<R: AsyncRead> EncryptReader<R> {
    pub fn new(inner: R, encryptor: DAREEncryptor) -> Self {
        EncryptReader {
            inner,
            encryptor,
            buffer: Vec::new(),
            pos: 0,
            chunk_buffer: vec![0u8; MAX_PAYLOAD_SIZE],
            chunk_filled: 0,
        }
    }
}

impl<R: AsyncRead> AsyncRead for EncryptReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let mut this = self.project();

        // If there's unread data in the buffer, copy it to the ReadBuf
        if *this.pos < this.buffer.len() {
            let available = this.buffer.len() - *this.pos;
            let to_copy = available.min(buf.remaining());

            buf.put_slice(&this.buffer[*this.pos..*this.pos + to_copy]);
            *this.pos += to_copy;
            return Poll::Ready(Ok(()));
        }

        // We need to keep reading until we fill the chunk buffer
        while *this.chunk_filled < MAX_PAYLOAD_SIZE {
            let filled_before = *this.chunk_filled;

            let mut temp_buf = ReadBuf::new(&mut this.chunk_buffer[*this.chunk_filled..]);
            ready!(this.inner.as_mut().poll_read(cx, &mut temp_buf))?;
            *this.chunk_filled += temp_buf.filled().len();

            if *this.chunk_filled == filled_before {
                break;
            }
        }

        if *this.chunk_filled == 0 {
            return Poll::Ready(Ok(()));
        }

        // Encrypt the chunk and store it in the buffer
        // TODO: handle error
        let encrypted_data = this
            .encryptor
            .encrypt(&this.chunk_buffer[..*this.chunk_filled])
            .unwrap();
        this.buffer.extend_from_slice(&encrypted_data);

        // Reset chunk filled for the next read cycle
        *this.chunk_filled = 0;

        // Copy encrypted data to the output buffer
        let available = this.buffer.len() - *this.pos;
        let to_copy = available.min(buf.remaining());

        buf.put_slice(&this.buffer[*this.pos..*this.pos + to_copy]);
        *this.pos += to_copy;

        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use crate::encryption::encryptor::EncryptReader;
    use dare::{CipherSuite, DAREDecryptor, DAREEncryptor};
    use std::io::Cursor;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    pub async fn test_async_encryption() {
        let key = [0u8; 32]; // In practice, use a secure random key

        let plaintext = b"a".repeat(200000).to_vec();

        // Encryption
        let encryptor = DAREEncryptor::new(key, CipherSuite::AES256GCM).expect("should not fail");

        let mut reader = EncryptReader::new(Cursor::new(&plaintext), encryptor);
        let mut encrypted = Vec::new();
        reader.read_to_end(&mut encrypted).await.unwrap();

        // Decryption
        let mut decryptor = DAREDecryptor::new(key);
        let mut decrypted = Cursor::new(Vec::new());
        decryptor
            .decrypt_stream(&mut Cursor::new(&encrypted), &mut decrypted)
            .await
            .unwrap();

        // assertion
        assert_eq!(plaintext, decrypted.into_inner());
    }
}
