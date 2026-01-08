use anyhow::Context;
use anyhow::ensure;
use clap::Parser;
use mega::Url;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(Parser, Debug)]
#[command(about = "Download a file")]
pub struct Options {
    input: String,

    output: Option<PathBuf>,

    #[arg(short = 'k', long = "key", help = "The file key")]
    key: Option<String>,

    #[arg(long = "reference-node-id", help = "The reference node id")]
    reference_node_id: Option<String>,
}

fn path_ends_with_sep(path: &Path) -> bool {
    path.as_os_str()
        .as_encoded_bytes()
        .last()
        .is_some_and(|b| std::path::is_separator(char::from(*b)))
}

#[derive(Debug)]
struct DownloadFileArgs {
    file_key: mega::FileKey,
    public_node_id: Option<String>,
    node_id: Option<String>,
    reference_node_id: Option<String>,
}

async fn download_file(
    client: &mega::EasyClient,
    args: DownloadFileArgs,
    output: Option<&Path>,
) -> anyhow::Result<()> {
    let mut builder = mega::EasyGetAttributesBuilder::new();
    builder.include_download_url(true);
    if let Some(public_node_id) = args.public_node_id {
        builder.public_node_id(public_node_id);
    }
    if let Some(node_id) = args.node_id {
        builder.node_id(node_id);
    }
    if let Some(reference_node_id) = args.reference_node_id {
        builder.reference_node_id(reference_node_id);
    }

    let attributes = client
        .get_attributes(builder)
        .await
        .context("failed to get attributes")?;
    let decoded_attributes = attributes
        .decode_attributes(args.file_key.key)
        .context("failed to decode attributes")?;
    let download_url = attributes
        .download_url
        .as_ref()
        .context("missing download url")?;

    let output = match output.as_ref() {
        Some(output) => {
            if path_ends_with_sep(output) {
                output.join(decoded_attributes.name)
            } else {
                output.to_path_buf()
            }
        }
        None => PathBuf::from(decoded_attributes.name),
    };

    let temp_output = output.with_added_extension("temp");
    let mut output_file = File::create(&temp_output)
        .await
        .with_context(|| format!("failed to open \"{}\"", temp_output.display()))?;
    let mut reader = client
        .download_file(&args.file_key, download_url.as_str())
        .await
        .context("failed to get download stream")?;

    let progress_bar = indicatif::ProgressBar::new(attributes.size);
    let progress_bar_style_template = "[Time = {elapsed_precise} | ETA = {eta_precise} | Speed = {bytes_per_sec}] {wide_bar} {bytes}/{total_bytes}";
    let progress_bar_style = indicatif::ProgressStyle::default_bar()
        .template(progress_bar_style_template)
        .expect("invalid progress bar style template");
    progress_bar.set_style(progress_bar_style);

    let progress_bar_tick_handle = {
        let progress_bar = progress_bar.clone();
        tokio::spawn(async move {
            while !progress_bar.is_finished() {
                progress_bar.tick();
                tokio::time::sleep(Duration::from_millis(1_000)).await;
            }
        })
    };
    tokio::io::copy(
        &mut progress_bar.wrap_async_read(&mut reader),
        &mut output_file,
    )
    .await?;
    output_file.flush().await?;
    output_file.sync_all().await?;
    tokio::fs::rename(temp_output, output).await?;
    progress_bar.finish();

    progress_bar_tick_handle.await?;

    Ok(())
}

#[derive(Debug)]
struct DownloadFolderArgs {
    root_folder_name: String,
    root_folder_id: String,
    reference_node_id: String,
    children: HashMap<String, Vec<DownloadFolderArgsChild>>,
}

#[derive(Debug)]
struct DownloadFolderArgsChild {
    name: String,
    id: String,
    key: mega::FileOrFolderKey,
}

async fn download_folder(
    client: &mega::EasyClient,
    mut args: DownloadFolderArgs,
    output: Option<&Path>,
) -> anyhow::Result<()> {
    let output = match output.as_ref() {
        Some(output) => {
            if path_ends_with_sep(output) {
                output.join(&args.root_folder_name)
            } else {
                output.to_path_buf()
            }
        }
        None => PathBuf::from(&args.root_folder_name),
    };

    let temp_output = output.with_added_extension("temp");
    tokio::fs::create_dir_all(&temp_output)
        .await
        .context("failed to create temp folder")?;

    let mut id_to_path = HashMap::new();
    id_to_path.insert(args.root_folder_id.clone(), temp_output.clone());
    let mut stack = vec![args.root_folder_id.clone()];
    while let Some(parent_node_id) = stack.pop() {
        let children = match args.children.remove(&parent_node_id) {
            Some(children) => children,
            None => {
                continue;
            }
        };
        let parent_path = id_to_path
            .get(&parent_node_id)
            .context("missing parent path")?
            .clone();

        for child in children.into_iter() {
            let node_path = parent_path.join(child.name);
            let file_key = match child.key {
                mega::FileOrFolderKey::File(key) => key,
                mega::FileOrFolderKey::Folder(_key) => {
                    tokio::fs::create_dir_all(&node_path).await?;

                    id_to_path.insert(child.id.clone(), node_path);
                    stack.push(child.id);
                    continue;
                }
            };

            download_file(
                client,
                DownloadFileArgs {
                    file_key,
                    node_id: Some(child.id),
                    public_node_id: None,
                    reference_node_id: Some(args.reference_node_id.clone()),
                },
                Some(&node_path),
            )
            .await?;
        }
    }

    tokio::fs::rename(&temp_output, &output)
        .await
        .context("failed to rename temp folder")?;

    Ok(())
}

#[derive(Debug)]
enum DownloadType {
    File(DownloadFileArgs),
    Folder(DownloadFolderArgs),
}

async fn parse_input_arg(
    client: &mega::EasyClient,
    input: &str,
    options_file_key: Option<mega::FileKey>,
) -> anyhow::Result<DownloadType> {
    let mut public_node_id = None;
    let mut node_id = None;
    let mut file_key = None;
    let mut reference_node_id = None;

    // If it starts with a url, assume it's a url.
    // Otherwise, assume it's a raw id.
    if input.starts_with("https://mega.nz") {
        let url = Url::parse(input).context("invalid url")?;
        let parsed_url = mega::ParsedMegaUrl::try_from(&url).context("failed to parse mega url")?;

        match parsed_url {
            mega::ParsedMegaUrl::File(file_url) => {
                public_node_id = Some(file_url.file_id.to_string());
                file_key = Some(file_url.file_key.clone());
            }
            mega::ParsedMegaUrl::Folder(folder_url) => {
                reference_node_id = Some(folder_url.folder_id.clone());

                let fetch_nodes_response = client
                    .fetch_nodes(Some(&folder_url.folder_id), true)
                    .await?;

                let child_data = match folder_url.child_data.as_ref() {
                    Some(child_data) => child_data,
                    None => {
                        let mut children = HashMap::with_capacity(fetch_nodes_response.nodes.len());
                        let mut root_folder_name = None;
                        let mut root_folder_id = None;
                        for node_entry in fetch_nodes_response.nodes.iter() {
                            let node_decoded_attributes =
                                node_entry.decode_attributes(&folder_url.folder_key)?;

                            let (root_id, _) = node_entry
                                .key
                                .split_once(':')
                                .context("invalid key format")?;
                            if root_id == node_entry.id {
                                ensure!(root_folder_name.is_none());
                                root_folder_name = Some(node_decoded_attributes.name);
                                root_folder_id = Some(root_id.to_string());
                                continue;
                            }
                            let key = node_entry.decrypt_key(&folder_url.folder_key)?;

                            children
                                .entry(node_entry.parent_id.clone())
                                .or_insert(Vec::new())
                                .push(DownloadFolderArgsChild {
                                    name: node_decoded_attributes.name,
                                    id: node_entry.id.clone(),
                                    key,
                                });
                        }

                        return Ok(DownloadType::Folder(DownloadFolderArgs {
                            root_folder_name: root_folder_name
                                .context("missing root folder node entry")?,
                            root_folder_id: root_folder_id
                                .context("missing root folder node entry")?,
                            reference_node_id: folder_url.folder_id.clone(),
                            children,
                        }));
                    }
                };
                ensure!(
                    child_data.is_file,
                    "child folder downloads are currently unsupported"
                );

                let node_entry = fetch_nodes_response
                    .nodes
                    .iter()
                    .find(|node| node.id == child_data.node_id)
                    .context("missing file node in folder listing")?;
                let node_key = node_entry
                    .decrypt_key(&folder_url.folder_key)?
                    .take_file_key()
                    .context("folder downloads are currently unsupported")?;

                node_id = Some(child_data.node_id.to_string());
                file_key = Some(node_key.clone());
            }
        }
    } else {
        node_id = Some(input.to_string());
    };

    let file_key = options_file_key.or(file_key).context("missing file key")?;

    Ok(DownloadType::File(DownloadFileArgs {
        file_key,
        public_node_id,
        node_id,
        reference_node_id,
    }))
}

pub async fn exec(client: &mega::EasyClient, options: &Options) -> anyhow::Result<()> {
    let options_file_key = options
        .key
        .as_deref()
        .map(|file_key| file_key.parse::<mega::FileKey>())
        .transpose()?;
    let download_type = parse_input_arg(client, &options.input, options_file_key).await?;

    match download_type {
        DownloadType::File(mut args) => {
            if let Some(reference_node_id) = options.reference_node_id.clone() {
                args.reference_node_id = Some(reference_node_id);
            }

            download_file(client, args, options.output.as_deref()).await?;
        }
        DownloadType::Folder(args) => {
            download_folder(client, args, options.output.as_deref()).await?;
        }
    }

    Ok(())
}
