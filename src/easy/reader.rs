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
        validator: FileValidator,
    }
}

impl<R> DownloadValidateReader<R> {
    /// Create a new reader.
    pub(crate) fn new(reader: R, file_key: &FileKey) -> Self {
        let cipher = Aes128Ctr128BE::new(
            &file_key.key.to_be_bytes().into(),
            &file_key.iv.to_be_bytes().into(),
        );
        let validator = FileValidator::new(file_key.clone());

        Self {
            reader,
            cipher,

            validator,
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
            this.validator.finish().map_err(std::io::Error::other)?;
        } else {
            this.validator.feed(new_bytes);
            buf.advance(new_bytes_len);
        }

        Poll::Ready(Ok(()))
    }
}

/// An iterator over chunks
#[derive(Debug)]
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

/// An error that occurs when a file fails validation.
#[derive(Debug)]
struct FileValidationError {
    /// The actual created mac
    pub actual_mac: [u8; 8],

    /// The expected mac
    pub expected_mac: [u8; 8],
}

impl std::fmt::Display for FileValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "file mac mismatch, expected {} but got {}",
            HexSlice(&self.expected_mac),
            HexSlice(&self.actual_mac)
        )
    }
}

impl std::error::Error for FileValidationError {}

/// A helper to format byte slices as hex
struct HexSlice<'a>(&'a [u8]);

impl std::fmt::Display for HexSlice<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "0x")?;
        for byte in self.0.iter() {
            write!(f, "{:X}", byte)?;
        }
        Ok(())
    }
}

/// A helper to validate files.
struct FileValidator {
    file_key: FileKey,
    chunk_iter: ChunkIter,
    left_in_chunk: usize,
    file_mac: u128,
    chunk_mac: u128,
    buffer: [u8; 16],
    buffer_end: usize,
}

impl FileValidator {
    /// Make a new file validator.
    pub fn new(file_key: FileKey) -> Self {
        let mut chunk_iter = ChunkIter::new();
        // ChunkIter is infinite.
        let (_, left_in_chunk) = chunk_iter.next().unwrap();
        // This can only fail when a usize is a u16.
        let left_in_chunk = usize::try_from(left_in_chunk).unwrap();
        let chunk_mac = create_chunk_mac(&file_key);

        Self {
            file_key,
            chunk_iter,
            left_in_chunk,
            file_mac: 0,
            chunk_mac,
            buffer: [0; 16],
            buffer_end: 0,
        }
    }

    /// Process a block
    fn process_block(&mut self, block: [u8; 16]) {
        self.chunk_mac ^= u128::from_be_bytes(block);
        let mut chunk_mac_bytes = self.chunk_mac.to_be_bytes();
        aes_cbc_encrypt_u128(self.file_key.key, &mut chunk_mac_bytes);
        self.chunk_mac = u128::from_be_bytes(chunk_mac_bytes);

        self.left_in_chunk -= 16;
        if self.left_in_chunk == 0 {
            self.begin_new_chunk();
        }
    }

    fn begin_new_chunk(&mut self) {
        self.file_mac ^= self.chunk_mac;
        let mut file_mac_bytes = self.file_mac.to_be_bytes();
        aes_cbc_encrypt_u128(self.file_key.key, &mut file_mac_bytes);
        self.file_mac = u128::from_be_bytes(file_mac_bytes);

        // Reset chunk state.
        self.chunk_mac = create_chunk_mac(&self.file_key);
        // ChunkIter is infinite.
        let (_, left_in_chunk) = self.chunk_iter.next().unwrap();
        // This can only fail when a usize is a u16.
        self.left_in_chunk = usize::try_from(left_in_chunk).unwrap();
    }

    /// Feed this data.
    ///
    /// This should be fed decrypted bytes.
    pub fn feed(&mut self, mut input: &[u8]) {
        if self.buffer_end != 0 {
            // Try to complete the buffer and process the block.
            let need_to_consume = self.buffer.len() - self.buffer_end;
            let consume_input_len = std::cmp::min(need_to_consume, input.len());

            self.buffer[self.buffer_end..(self.buffer_end + consume_input_len)]
                .copy_from_slice(&input[..consume_input_len]);
            if consume_input_len < need_to_consume {
                // The input was too small to create even 1 block.
                return;
            }

            self.process_block(self.buffer);
            input = &input[need_to_consume..];
            self.buffer_end = 0;
        }

        // Split the input into blocks and process up until the last whole block.
        let mut block_iter = input.chunks_exact(16);
        for block in block_iter.by_ref() {
            // The iter will always produce blocks of the right size.
            let block = block.try_into().unwrap();
            self.process_block(block);
        }

        // Save remaining input into the buffer.
        let remainder = block_iter.remainder();
        let remainder_len = remainder.len();
        if remainder_len != 0 {
            self.buffer[..remainder_len].copy_from_slice(remainder);
            self.buffer_end = remainder_len;
        }
    }

    /// Finish feeding this data and validate the file.
    pub fn finish(&mut self) -> Result<(), FileValidationError> {
        // Ignoring the buffer contents is not a bug.
        // The last few bytes of a file are not validated.
        self.file_mac ^= self.chunk_mac;
        let mut file_mac_bytes = self.file_mac.to_be_bytes();
        aes_cbc_encrypt_u128(self.file_key.key, &mut file_mac_bytes);
        self.file_mac = u128::from_be_bytes(file_mac_bytes);

        let file_mac_bytes = self.file_mac.to_be_bytes();
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

        if final_file_mac != self.file_key.meta_mac {
            return Err(FileValidationError {
                expected_mac: self.file_key.meta_mac.to_be_bytes(),
                actual_mac: final_file_mac.to_be_bytes(),
            });
        }

        Ok(())
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
    use crate::test::*;

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

    #[test]
    fn file_validator() {
        let file_key = FileKey {
            key: TEST_FILE_KEY_KEY_DECODED,
            iv: TEST_FILE_KEY_IV_DECODED,
            meta_mac: TEST_FILE_META_MAC_DECODED,
        };

        let mut validator = FileValidator::new(file_key);
        validator.feed(TEST_FILE_BYTES);
        validator.finish().expect("invalid mac");
    }
}
