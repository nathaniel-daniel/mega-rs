use mega::Url;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

#[derive(argh::FromArgs)]
#[argh(subcommand, name = "verify-file", description = "verify a file")]
pub struct Options {
    #[argh(positional)]
    input: PathBuf,

    #[argh(option, description = "the url where this file came from")]
    url: String,
}

pub async fn exec(_client: &mega::EasyClient, options: &Options) -> anyhow::Result<()> {
    let url = Url::parse(&options.url)?;
    let parsed_url = mega::parse_file_url(&url)?;

    let mut file_validator = mega::FileValidator::new(parsed_url.file_key);
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
