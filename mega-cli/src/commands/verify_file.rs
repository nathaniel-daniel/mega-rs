use anyhow::Context;
use clap::Parser;
use mega::Url;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

#[derive(Debug, Parser)]
#[command(about = "Verify a file")]
pub struct Options {
    input: PathBuf,

    #[arg(short = 'u', long = "url", help = "The url where this file came from")]
    url: String,
}

pub async fn exec(_client: &mega::EasyClient, options: &Options) -> anyhow::Result<()> {
    let url = Url::parse(&options.url)?;
    let parsed_url = mega::ParsedMegaUrl::try_from(&url).context("failed to parse mega url")?;
    let parsed_url = parsed_url.as_file_url().context("url must be a file url")?;

    let mut file_validator = mega::FileValidator::new(parsed_url.file_key.clone());
    let mut file = File::open(&options.input).await?;

    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        file_validator.feed(&buffer[..n]);
    }
    file_validator.finish()?;

    println!("Ok");

    Ok(())
}
