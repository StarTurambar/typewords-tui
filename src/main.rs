use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = typewords_tui::cli::Cli::parse();
    typewords_tui::run(cli)
}
