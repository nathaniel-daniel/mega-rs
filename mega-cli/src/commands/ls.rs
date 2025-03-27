use anyhow::Context;
use mega::Url;

#[derive(argh::FromArgs)]
#[argh(subcommand, name = "ls", description = "list a folder")]
pub struct Options {
    #[argh(positional)]
    input: String,

    #[argh(switch, description = "whether to list recursively")]
    recursive: bool,
}

pub async fn exec(client: &mega::EasyClient, options: &Options) -> anyhow::Result<()> {
    let url = Url::parse(options.input.as_str()).context("invalid url")?;

    let parsed_url = mega::parse_folder_url(&url).context("failed to parse folder url")?;

    let response = client
        .fetch_nodes(Some(parsed_url.folder_id), options.recursive)
        .await
        .context("failed to fetch")?;

    for node in response.files.iter() {
        let decoded_attributes = node.decode_attributes(&parsed_url.folder_key)?;
        let key = node.decrypt_key(&parsed_url.folder_key)?;

        println!("Id: {}", node.id);
        println!("Name: {}", decoded_attributes.name);
        println!("Type: {:?}", node.kind);
        println!("Parent ID: {}", node.parent_id);
        println!("Key: {}", key);
        println!();
        println!();
    }

    Ok(())
}
