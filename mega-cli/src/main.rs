mod commands;

#[derive(argh::FromArgs)]
#[argh(description = "a CLI for mega")]
struct Options {
    #[argh(subcommand)]
    subcommand: Subcommand,
}

#[derive(argh::FromArgs)]
#[argh(subcommand)]
enum Subcommand {
    Get(self::commands::get::Options),
    VerifyFile(self::commands::verify_file::Options),
    Ls(self::commands::ls::Options),
}

fn main() -> anyhow::Result<()> {
    let options = argh::from_env();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async_main(options))
}

async fn async_main(options: Options) -> anyhow::Result<()> {
    let client = mega::EasyClient::new();

    match options.subcommand {
        Subcommand::Get(options) => self::commands::get::exec(&client, &options).await,
        Subcommand::VerifyFile(options) => {
            self::commands::verify_file::exec(&client, &options).await
        }
        Subcommand::Ls(options) => self::commands::ls::exec(&client, &options).await,
    }
}
