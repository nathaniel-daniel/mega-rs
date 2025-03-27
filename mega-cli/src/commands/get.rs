use anyhow::Context;
use mega::Url;
use std::path::Path;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(argh::FromArgs)]
#[argh(subcommand, name = "get", description = "download a file")]
pub struct Options {
    #[argh(positional)]
    input: String,

    #[argh(positional)]
    output: Option<PathBuf>,

    #[argh(option, description = "the file key")]
    key: Option<String>,

    #[argh(option, description = "the reference node id")]
    reference_node_id: Option<String>,
}

pub async fn exec(client: &mega::EasyClient, options: &Options) -> anyhow::Result<()> {
    // If it starts with a url, assume it's a url.
    // Otherwise, assume it's a raw id.
    let mut public_file_id = None;
    let mut node_id = None;
    let mut file_key = None;
    if options.input.starts_with("https://mega.nz") {
        let url = Url::parse(options.input.as_str()).context("invalid url")?;
        let parsed_url = mega::parse_file_url(&url).context("failed to parse file url")?;

        public_file_id = Some(parsed_url.file_id.to_string());
        file_key = Some(parsed_url.file_key.clone());
    } else {
        node_id = Some(options.input.as_str());
    };

    let file_key = options
        .key
        .clone()
        .map(|key| key.parse::<mega::FileKey>())
        .transpose()?
        .or(file_key)
        .context("missing file key")?;

    let mut builder = mega::EasyGetAttributesBuilder::new();
    builder.include_download_url(true);
    if let Some(public_file_id) = public_file_id {
        builder.public_file_id(public_file_id);
    }
    if let Some(node_id) = node_id {
        builder.node_id(node_id);
    }
    if let Some(reference_node_id) = options.reference_node_id.as_ref() {
        builder.reference_node_id(reference_node_id);
    }

    let attributes = client
        .get_attributes(builder)
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
