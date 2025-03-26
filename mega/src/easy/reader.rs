use crate::FileKey;
use crate::FileValidator;
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

pin_project! {
    /// A reader for a file.
    pub struct FileDownloadReader<R> {
        #[pin]
        reader: R,
        cipher: Aes128Ctr128BE,
        validator: Option<FileValidator>,
    }
}

impl<R> FileDownloadReader<R> {
    /// Make a new reader.
    pub(crate) fn new(reader: R, file_key: &FileKey, validate: bool) -> Self {
        let cipher = Aes128Ctr128BE::new(
            &file_key.key.to_be_bytes().into(),
            &file_key.iv.to_be_bytes().into(),
        );
        let validator = if validate {
            Some(FileValidator::new(file_key.clone()))
        } else {
            None
        };

        Self {
            reader,
            cipher,
            validator,
        }
    }
}

impl<R> AsyncRead for FileDownloadReader<R>
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
        let new_bytes_len = new_bytes.len();
        this.cipher.apply_keystream(new_bytes);
        if let Some(validator) = this.validator.as_mut() {
            if new_bytes_len == 0 {
                validator.finish().map_err(std::io::Error::other)?;
            } else {
                validator.feed(new_bytes);
            }
        }
        // Safety: This was already initialized via the unfilled_buf sub-buffer.
        unsafe {
            buf.assume_init(new_bytes_len);
        }
        buf.advance(new_bytes_len);

        Poll::Ready(Ok(()))
    }
}
