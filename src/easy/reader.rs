use crate::FileKey;
use cbc::cipher::BlockEncryptMut;
use cbc::cipher::KeyIvInit;
use cbc::cipher::StreamCipher;
use pin_project_lite::pin_project;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::task::ready;
use tokio::io::AsyncRead;
use tokio::io::ReadBuf;

type Aes128Ctr128BE = ctr::Ctr128BE<aes::Aes128>;
type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;

pin_project! {
    /// A reader for a file that does not validate its contents.
    pub struct DownloadNoValidateReader<R> {
        #[pin]
        reader: R,
        cipher: Aes128Ctr128BE,
    }
}

impl<R> DownloadNoValidateReader<R> {
    /// Make a new reader.
    pub(crate) fn new(reader: R, file_key: &FileKey) -> Self {
        let cipher = Aes128Ctr128BE::new(
            &file_key.key.to_be_bytes().into(),
            &file_key.iv.to_be_bytes().into(),
        );

        Self { reader, cipher }
    }
}

impl<R> AsyncRead for DownloadNoValidateReader<R>
where
    R: AsyncRead,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // See: https://users.rust-lang.org/t/blocking-permit/36865/5
        const MAX_LEN: usize = 64 * 1024;

        let this = self.as_mut().project();

        let mut unfilled_buf = buf.take(MAX_LEN);

        let result = ready!(this.reader.poll_read(cx, &mut unfilled_buf));
        result?;

        let new_bytes = unfilled_buf.filled_mut();
        this.cipher.apply_keystream(new_bytes);
        let new_bytes_len = new_bytes.len();
        buf.advance(new_bytes_len);

        Poll::Ready(Ok(()))
    }
}

pin_project! {
     /// A reader for a file that validates its contents.
    pub struct DownloadValidateReader<R> {
        #[pin]
        reader: R,
        cipher: Aes128Ctr128BE,

        file_key: FileKey,
        chunk_iter: ChunkIter,
        left_in_chunk: usize,
        file_mac: u128,
        chunk_mac: u128,
        buffer: Vec<u8>,
    }
}

impl<R> DownloadValidateReader<R> {
    /// Create a new reader.
    pub(crate) fn new(reader: R, file_key: &FileKey) -> Self {
        const MAX_CHUNK_SIZE: usize = 128 * 8 * 1024;

        let cipher = Aes128Ctr128BE::new(
            &file_key.key.to_be_bytes().into(),
            &file_key.iv.to_be_bytes().into(),
        );
        let mut chunk_iter = ChunkIter::new();
        // ChunkIter is infinite.
        let (_, left_in_chunk) = chunk_iter.next().unwrap();
        // This can only fail when a usize is a u16.
        let left_in_chunk = usize::try_from(left_in_chunk).unwrap();
        let chunk_mac = create_chunk_mac(file_key);
        let buffer = Vec::with_capacity(MAX_CHUNK_SIZE);

        Self {
            reader,
            cipher,

            file_key: file_key.clone(),
            chunk_iter,
            left_in_chunk,
            file_mac: 0,
            chunk_mac,
            buffer,
        }
    }
}

impl<R> AsyncRead for DownloadValidateReader<R>
where
    R: AsyncRead,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // See: https://users.rust-lang.org/t/blocking-permit/36865/5
        const MAX_LEN: usize = 64 * 1024;

        let this = self.as_mut().project();

        // Limit max chunk processed at a time to avoid blocking.
        let mut unfilled_buf = buf.take(MAX_LEN);

        let result = ready!(this.reader.poll_read(cx, &mut unfilled_buf));
        result?;

        let new_bytes = unfilled_buf.filled_mut();
        this.cipher.apply_keystream(new_bytes);
        let new_bytes_len = new_bytes.len();

        if new_bytes_len == 0 {
            *this.file_mac ^= *this.chunk_mac;
            let mut file_mac_bytes = this.file_mac.to_be_bytes();
            aes_cbc_encrypt_u128(this.file_key.key, &mut file_mac_bytes);
            *this.file_mac = u128::from_be_bytes(file_mac_bytes);

            let file_mac_bytes = this.file_mac.to_be_bytes();
            let file_mac_u32_0 = u32::from_be_bytes(file_mac_bytes[..4].try_into().unwrap());
            let file_mac_u32_1 = u32::from_be_bytes(file_mac_bytes[4..8].try_into().unwrap());
            let file_mac_u32_2 = u32::from_be_bytes(file_mac_bytes[8..12].try_into().unwrap());
            let file_mac_u32_3 = u32::from_be_bytes(file_mac_bytes[12..].try_into().unwrap());

            let final_file_mac_u32_0 = file_mac_u32_0 ^ file_mac_u32_1;
            let final_file_mac_u32_1 = file_mac_u32_2 ^ file_mac_u32_3;

            let mut final_file_mac_bytes = [0; 8];
            final_file_mac_bytes[..4].copy_from_slice(&final_file_mac_u32_0.to_be_bytes());
            final_file_mac_bytes[4..].copy_from_slice(&final_file_mac_u32_1.to_be_bytes());
            let final_file_mac = u64::from_be_bytes(final_file_mac_bytes);

            if final_file_mac != this.file_key.meta_mac {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "mac mismatch",
                )));
            }
        } else {
            this.buffer.extend(&*new_bytes);

            let mut buffer_start = 0;
            while this.buffer[buffer_start..].len() >= 16 {
                let mut len = std::cmp::min(*this.left_in_chunk, this.buffer[buffer_start..].len());
                len -= len % 16;

                let mut chunk_iter = this.buffer[buffer_start..buffer_start + len].chunks_exact(16);
                for chunk in &mut chunk_iter {
                    let block: [u8; 16] = chunk
                        .try_into()
                        .expect("chunk should always be a multiple of 16");
                    *this.chunk_mac ^= u128::from_be_bytes(block);
                    let mut chunk_mac_bytes = this.chunk_mac.to_be_bytes();
                    aes_cbc_encrypt_u128(this.file_key.key, &mut chunk_mac_bytes);
                    *this.chunk_mac = u128::from_be_bytes(chunk_mac_bytes);
                }
                buffer_start += len;

                *this.left_in_chunk -= len;
                if *this.left_in_chunk == 0 {
                    *this.file_mac ^= *this.chunk_mac;
                    let mut file_mac_bytes = this.file_mac.to_be_bytes();
                    aes_cbc_encrypt_u128(this.file_key.key, &mut file_mac_bytes);
                    *this.file_mac = u128::from_be_bytes(file_mac_bytes);

                    *this.chunk_mac = create_chunk_mac(this.file_key);

                    // ChunkIter is infinite.
                    let (_, left_in_chunk) = this.chunk_iter.next().unwrap();
                    // This can only fail when a usize is a u16.
                    *this.left_in_chunk = usize::try_from(left_in_chunk).unwrap();
                }
            }
            let mut remainder_copy = [0; 16];
            let remainder_len = this.buffer[buffer_start..].len();
            remainder_copy[..remainder_len].copy_from_slice(&this.buffer[buffer_start..]);
            this.buffer.clear();
            if remainder_len != 0 {
                this.buffer.extend(&remainder_copy[..remainder_len]);
            }

            buf.advance(new_bytes_len);
        }

        Poll::Ready(Ok(()))
    }
}

/// An iterator over chunks
struct ChunkIter {
    /// The offset into the file
    offset: u64,
    delta: u64,
}

impl ChunkIter {
    fn new() -> Self {
        Self {
            delta: 0,
            offset: 0,
        }
    }
}

impl Iterator for ChunkIter {
    type Item = (u64, u64);

    fn next(&mut self) -> Option<Self::Item> {
        self.delta += 128 * 1024;
        self.delta = std::cmp::min(self.delta, 1024 * 1024);

        let old_offset = self.offset;
        self.offset += self.delta;

        Some((old_offset, self.delta))
    }
}

fn create_chunk_mac(file_key: &FileKey) -> u128 {
    let mut chunk_mac_bytes = [0; 16];
    let iv_bytes = file_key.iv.to_be_bytes();
    chunk_mac_bytes[..8].copy_from_slice(&iv_bytes[..8]);
    chunk_mac_bytes[8..].copy_from_slice(&iv_bytes[..8]);
    u128::from_be_bytes(chunk_mac_bytes)
}

fn aes_cbc_encrypt_u128(key: u128, data: &mut [u8; 16]) {
    let mut cipher = Aes128CbcEnc::new(&key.to_be_bytes().into(), &[0; 16].into());
    cipher.encrypt_block_mut((data).into());
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[expect(clippy::erasing_op, clippy::identity_op)]
    fn chunk_iter() {
        let mut iter = ChunkIter::new();
        assert!(iter.next() == Some((128 * 0 * 2014, 128 * 1 * 1024)));
        assert!(iter.next() == Some((128 * 1 * 1024, 128 * 2 * 1024)));
        assert!(iter.next() == Some((128 * 3 * 1024, 128 * 3 * 1024)));
        assert!(iter.next() == Some((128 * 6 * 1024, 128 * 4 * 1024)));
        assert!(iter.next() == Some((128 * 10 * 1024, 128 * 5 * 1024)));
        assert!(iter.next() == Some((128 * 15 * 1024, 128 * 6 * 1024)));
        assert!(iter.next() == Some((128 * 21 * 1024, 128 * 7 * 1024)));
        assert!(iter.next() == Some((128 * 28 * 1024, 128 * 8 * 1024)));
        assert!(iter.next() == Some((128 * 36 * 1024, 128 * 8 * 1024)));
        assert!(iter.next() == Some((128 * 44 * 1024, 128 * 8 * 1024)));
    }
}
