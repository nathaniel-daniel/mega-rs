use anyhow::Context;
use anyhow::bail;
use mega::Url;
use std::collections::HashSet;
use std::io::Write;

#[derive(Debug, Default)]
enum OutputFormat {
    #[default]
    Human,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            _ => bail!("unknown output format \"{input}\""),
        }
    }
}

#[derive(argh::FromArgs)]
#[argh(subcommand, name = "ls", description = "list a folder")]
pub struct Options {
    #[argh(positional)]
    input: String,

    #[argh(switch, description = "whether to list recursively")]
    recursive: bool,

    #[argh(switch, description = "whether to leave the output unsorted")]
    unsorted: bool,

    #[argh(
        option,
        long = "output-format",
        description = "specify the output format",
        default = "Default::default()"
    )]
    output_format: OutputFormat,
}

#[derive(Debug, serde::Serialize)]
struct Entry {
    id: String,
    name: String,
    kind: mega::FetchNodesNodeKind,
    parent_id: String,
    key: mega::FileOrFolderKey,
    public_url: Option<String>,
}

pub async fn exec(client: &mega::EasyClient, options: &Options) -> anyhow::Result<()> {
    let url = Url::parse(options.input.as_str()).context("invalid url")?;

    let parsed_url = mega::ParsedMegaUrl::try_from(&url).context("failed to parse folder url")?;
    let parsed_url = parsed_url
        .as_folder_url()
        .context("url must be a folder url")?;
    let parent_id = match parsed_url.child_data.as_ref() {
        Some(child_data) if !child_data.is_file => Some(child_data.node_id.as_str()),
        Some(_child_data) => bail!("cannot ls a file node"),
        None => None,
    };

    let response = client
        .fetch_nodes(
            Some(&parsed_url.folder_id),
            options.recursive || parent_id.is_some(),
        )
        .await
        .context("failed to fetch")?;

    let mut children = HashSet::new();
    if options.recursive
        && let Some(parent_id) = parent_id
    {
        let mut stack = vec![parent_id];
        while let Some(node_id) = stack.pop() {
            for node in response.nodes.iter() {
                if !children.contains(node.parent_id.as_str()) {
                    continue;
                }
                if !node.kind.is_dir() {
                    continue;
                }

                stack.push(&node.id);
            }
            children.insert(node_id);
        }
    }

    let mut entries = Vec::with_capacity(response.nodes.len());
    for node in response.nodes.iter() {
        if let Some(parent_id) = parent_id
            && ((options.recursive && !children.contains(node.parent_id.as_str()))
                || (node.parent_id != parent_id))
        {
            continue;
        }

        let decoded_attributes = node.decode_attributes(&parsed_url.folder_key)?;
        let key = node.decrypt_key(&parsed_url.folder_key)?;

        let kind_str = match node.kind {
            mega::FetchNodesNodeKind::File => Some("file"),
            mega::FetchNodesNodeKind::Directory => Some("folder"),
            _ => None,
        };

        let public_url = kind_str.map(|kind_str| {
            format!(
                "https://mega.nz/folder/{}#{}/{}/{}",
                parsed_url.folder_id, parsed_url.folder_key, kind_str, node.id
            )
        });

        entries.push(Entry {
            id: node.id.clone(),
            name: decoded_attributes.name,
            kind: node.kind,
            parent_id: node.parent_id.clone(),
            key,
            public_url,
        });
    }
    if !options.unsorted {
        entries.sort_by(|a, b| a.name.cmp(&b.name));
    }

    match options.output_format {
        OutputFormat::Human => {
            for entry in entries {
                println!("Id: {}", entry.id);
                println!("Name: {}", entry.name);
                println!("Type: {:?}", entry.kind);
                println!("Parent ID: {}", entry.parent_id);
                println!("Key: {}", entry.key);
                if let Some(public_url) = entry.public_url {
                    println!("Public Url: {public_url}",);
                }
                println!();
                println!();
            }
        }
        OutputFormat::Json => {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            serde_json::to_writer(&mut lock, &entries)?;
            lock.flush()?;
        }
    }

    Ok(())
}
