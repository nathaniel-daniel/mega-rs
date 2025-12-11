use crate::Options as RootOptions;
use anyhow::Context;
use clap::CommandFactory;
use clap::Parser;
use clap_complete::Shell;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(about = "Generate shell completions")]
pub struct Options {
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,

    #[arg(short = 's', long = "shell")]
    shell: Option<Shell>,
}

pub fn exec(options: Options) -> anyhow::Result<()> {
    let mut command = RootOptions::command();

    let shell = options
        .shell
        .map(Ok)
        .unwrap_or_else(|| Shell::from_env().context("failed to determine shell"))?;

    let command_name = command.get_name().to_string();

    let stdout = std::io::stdout();
    let mut output: Box<dyn Write> = match options.output {
        Some(output) => {
            let file = File::create(&output)
                .with_context(|| format!("failed to create file \"{}\"", output.display()))?;
            Box::new(file)
        }
        None => {
            let lock = stdout.lock();
            Box::new(lock)
        }
    };

    clap_complete::generate(shell, &mut command, command_name, &mut output);

    Ok(())
}
