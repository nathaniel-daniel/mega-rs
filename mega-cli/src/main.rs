mod commands;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = "A CLI for mega")]
struct Options {
    #[command(subcommand)]
    subcommand: Subcommand,
}

#[derive(Parser, Debug)]
enum Subcommand {
    #[command(name = "get")]
    Get(self::commands::get::Options),

    #[command(name = "verify-file")]
    VerifyFile(self::commands::verify_file::Options),

    #[command(name = "ls")]
    Ls(self::commands::ls::Options),

    #[command(name = "generate-completions")]
    GenerateCompletions(self::commands::generate_completions::Options),
}

fn main() -> anyhow::Result<()> {
    let options = Options::parse();
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
        Subcommand::GenerateCompletions(options) => {
            self::commands::generate_completions::exec(options)
        }
    }
}
