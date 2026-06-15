use clap::Parser;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Arguments {
    /// Output to JSON format
    #[arg(long)]
    pub json: bool,

    /// List all packages installed of each repo to JSON
    #[arg(long, requires = "json")]
    pub paclist: bool,
}
