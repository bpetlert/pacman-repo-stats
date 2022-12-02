mod args;
mod report;

use std::{
    io::{self, Write},
    process::ExitCode,
};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use tracing::{debug, error};
use tracing_subscriber::EnvFilter;

use crate::{args::Arguments, report::Summary};

fn run() -> Result<()> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or(EnvFilter::try_new("pacman_repo_stats=warn")?);
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .without_time()
        .with_writer(io::stderr)
        .try_init()
        .map_err(|err| anyhow!("{err:#}"))
        .context("Failed to initialize tracing subscriber")?;

    let arguments = Arguments::parse();
    debug!("Run with {:?}", arguments);

    let mut summary = Summary::new();
    summary.build()?;
    summary.finalize()?;

    let mut stdout = io::BufWriter::new(io::stdout().lock());

    if arguments.json {
        let json = serde_json::to_string(&summary)?;
        write!(stdout, "{json}")?;
        return Ok(());
    }

    writeln!(stdout, "{summary}")?;

    Ok(())
}

fn main() -> ExitCode {
    if let Err(err) = run() {
        error!("{err:#}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
