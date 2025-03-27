use anyhow::Context;
use anyhow::ensure;
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

    let parsed_url = mega::ParsedMegaUrl::try_from(&url).context("failed to parse folder url")?;
    let parsed_url = parsed_url
        .as_folder_url()
        .context("url must be a folder url")?;
    ensure!(
        parsed_url.child_data.is_none(),
        "folder urls with child data are currently unsupported"
    );

    let response = client
        .fetch_nodes(Some(&parsed_url.folder_id), options.recursive)
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
