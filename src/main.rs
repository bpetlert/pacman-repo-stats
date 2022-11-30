use std::{
    collections::HashMap,
    io::{self, Write},
};

use alpm::{Alpm, SigLevel};
use anyhow::{Context, Result};
use clap::Parser;
use pacmanconf::Config;
use tabled::{
    format::Format,
    locator::ByColumnName,
    object::{Columns, Object, Rows},
    Alignment, Modify, Style, Table, Tabled,
};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Arguments {
    /// Output to JSON format
    #[arg(long)]
    json: bool,
}

#[derive(Debug)]
struct Summary {
    repo_stats: Vec<RepoStats>,
    local_installed_total: u64,
    pkgs_not_in_repo: Vec<String>,
}

#[derive(Debug, Clone, Tabled)]
struct RepoStats {
    #[tabled(rename = "Name")]
    name: String,

    #[tabled(rename = "Total")]
    total_pkgs: u64,

    #[tabled(rename = "Installed")]
    installed_pkgs: u64,

    #[tabled(rename = "Percentage")]
    installed_pkgs_percent: PercentageValue,
}

#[derive(Debug, Clone)]
struct PercentageValue(Option<f64>);

impl Summary {
    pub fn new() -> Self {
        Self {
            repo_stats: Vec::new(),
            local_installed_total: 0,
            pkgs_not_in_repo: Vec::new(),
        }
    }

    /// Generate repo stats
    pub fn build(&mut self) -> Result<()> {
        let alpm = {
            let pacman_conf = Config::new().context("Failed to load `pacman.conf`")?;
            let alpm = Alpm::new(pacman_conf.root_dir, pacman_conf.db_path)
                .context("Could not access ALPM")?;

            // Register repository database
            for repo in &pacman_conf.repos {
                alpm.register_syncdb(&*repo.name, SigLevel::USE_DEFAULT)
                    .with_context(|| format!("Could not register `{}`", repo.name))?;
            }

            alpm
        };

        self.local_installed_total = alpm.localdb().pkgs().len() as u64;

        self.repo_stats = {
            let mut stats: HashMap<String, RepoStats> = alpm
                .syncdbs()
                .iter()
                .map(|repo| {
                    (
                        repo.name().to_owned(),
                        RepoStats::new(repo.name(), repo.pkgs().len() as u64, 0),
                    )
                })
                .collect();

            // Count installed packages from each repo
            for local_installed in alpm.localdb().pkgs() {
                let mut found = false;
                for db in alpm.syncdbs().iter() {
                    if db.pkg(local_installed.name()).is_ok() {
                        stats.get_mut(db.name()).unwrap().add_installed();

                        found = true;
                        break;
                    }
                }

                if !found {
                    self.pkgs_not_in_repo
                        .push(local_installed.name().to_string());
                }
            }

            // Return the same order of DB as in pacman.conf
            alpm.syncdbs()
                .iter()
                .map(|repo| stats[&repo.name().to_owned()].clone())
                .collect()
        };

        Ok(())
    }

    /// Calculate total
    pub fn finalize(&mut self) -> Result<()> {
        let repo_total: u64 = self.repo_stats.iter().map(|stats| stats.total_pkgs).sum();

        let repo_installed_total: u64 = self
            .repo_stats
            .iter()
            .map(|stats| stats.installed_pkgs)
            .sum::<u64>();

        self.repo_stats
            .push(RepoStats::new("", repo_total, repo_installed_total));

        self.percentage();

        Ok(())
    }

    fn repo_stats_to_table(&self) -> Result<String> {
        let mut table = Table::new(&self.repo_stats);
        table
            .with(Style::re_structured_text())
            .with(Modify::new(ByColumnName::new("Name")).with(Alignment::left()))
            .with(Modify::new(ByColumnName::new("Total")).with(Alignment::right()))
            .with(Modify::new(ByColumnName::new("Installed")).with(Alignment::right()))
            .with(Modify::new(ByColumnName::new("Percentage")).with(Alignment::right()))
            .with(
                Modify::new(Rows::last().intersect(Columns::new(1..=3)))
                    .with(Format::new(|s| format!("({})", s))),
            );

        Ok(table.to_string())
    }
}

impl RepoStats {
    pub fn new(name: &str, total: u64, installed: u64) -> Self {
        Self {
            name: name.to_owned(),
            total_pkgs: total,
            installed_pkgs: installed,
            installed_pkgs_percent: PercentageValue(None),
        }
    }

    /// Increase count of installed packages
    pub fn add_installed(&mut self) {
        self.installed_pkgs += 1;
    }
}

impl std::fmt::Display for PercentageValue {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(value) = self.0 {
            write!(f, "{value:.2}")
        } else {
            write!(f, "N/A")
        }
    }
}

impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let table = self
            .repo_stats_to_table()
            .context("Failed to convert to display table")
            .map_err(|_| std::fmt::Error)?;
        write!(f, "{table}")?;

        write!(
            f,
            "\nLocal Installed Packages: {}",
            self.local_installed_total
        )?;

        write!(
            f,
            "\nInstalled Packages Not Found In Repo: {}",
            self.pkgs_not_in_repo.len()
        )?;

        for pkg in &self.pkgs_not_in_repo {
            write!(f, "\n    {}", pkg)?;
        }

        Ok(())
    }
}

trait Percentage {
    fn percentage(&mut self);
}

impl Percentage for RepoStats {
    fn percentage(&mut self) {
        if self.total_pkgs == 0 {
            self.installed_pkgs_percent = PercentageValue(None);
            return;
        }

        self.installed_pkgs_percent = PercentageValue(Some(
            (self.installed_pkgs as f64) * 100_f64 / (self.total_pkgs as f64),
        ));
    }
}

impl Percentage for Summary {
    fn percentage(&mut self) {
        self.repo_stats.iter_mut().for_each(|r| r.percentage());
    }
}

fn main() -> Result<()> {
    let _arguments = Arguments::parse();

    let mut summary = Summary::new();
    summary.build()?;
    summary.finalize()?;

    let mut stdout = io::BufWriter::new(io::stdout().lock());
    writeln!(stdout, "{summary}")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_stats_to_table() {
        let mut summary = Summary::new();
        summary.repo_stats.push(RepoStats::new("core", 1234, 234));
        summary
            .repo_stats
            .push(RepoStats::new("community", 4567, 456));
        summary.repo_stats.push(RepoStats::new("extra", 8999, 555));
        summary.finalize().unwrap();

        let table = summary.repo_stats_to_table().unwrap();
        assert_eq!(
            table,
            concat!(
                "=========== ========= =========== ============\n",
                " Name          Total   Installed   Percentage \n",
                "=========== ========= =========== ============\n",
                " core           1234         234        18.96 \n",
                " community      4567         456         9.98 \n",
                " extra          8999         555         6.17 \n",
                "             (14800)      (1245)       (8.41) \n",
                "=========== ========= =========== ============",
            ),
            "\n{table}"
        );
    }
}
