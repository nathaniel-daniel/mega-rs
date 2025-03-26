use anyhow::Context;
use anyhow::ensure;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use url::Url;

#[derive(argh::FromArgs)]
#[argh(subcommand, name = "get", description = "download a file")]
pub struct Options {
    #[argh(positional)]
    input: String,

    #[argh(positional)]
    output: Option<PathBuf>,
}

pub async fn exec(client: &mega::EasyClient, options: &Options) -> anyhow::Result<()> {
    let url = Url::parse(options.input.as_str()).context("invalid url")?;

    ensure!(url.host_str() == Some("mega.nz"));
    let mut path_iter = url.path_segments().context("missing path")?;
    ensure!(path_iter.next() == Some("file"));

    let file_id = path_iter.next().context("missing file id")?;
    ensure!(path_iter.next().is_none());

    let file_key_raw = url.fragment().context("missing file key")?;
    let file_key: mega::FileKey = file_key_raw.parse()?;

    let attributes_future = client.get_attributes(file_id, true);
    client.send_commands();

    let attributes = attributes_future
        .await
        .context("failed to get attributes")?;
    let decoded_attributes = attributes
        .decode_attributes(file_key.key)
        .context("failed to decode attributes")?;
    let download_url = attributes
        .download_url
        .as_ref()
        .context("missing download url")?;

    let output = match options.output.as_ref() {
        Some(output) => {
            if path_ends_with_sep(output) {
                output.join(decoded_attributes.name)
            } else {
                output.clone()
            }
        }
        None => PathBuf::from(decoded_attributes.name),
    };

    let temp_output = nd_util::with_push_extension(&output, "temp");
    let mut output_file = File::create(&temp_output)
        .await
        .with_context(|| format!("failed to open \"{}\"", temp_output.display()))?;
    let mut reader = client
        .download_file(&file_key, download_url.as_str())
        .await
        .context("failed to get download stream")?;
    tokio::io::copy(&mut reader, &mut output_file).await?;
    output_file.flush().await?;
    output_file.sync_all().await?;
    tokio::fs::rename(temp_output, output).await?;

    Ok(())
}

fn path_ends_with_sep(path: &Path) -> bool {
    path.as_os_str()
        .as_encoded_bytes()
        .last()
        .is_some_and(|b| std::path::is_separator(char::from(*b)))
}
