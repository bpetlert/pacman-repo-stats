use std::collections::{
    BTreeSet,
    HashMap,
};

use alpm::{
    Alpm,
    SigLevel,
};
use anyhow::{
    Context,
    Result,
};
use pacmanconf::Config;
use serde::Serialize;
use tabled::{
    builder::Builder,
    settings::{
        Alignment,
        Format,
        Modify,
        Style,
        location::ByColumnName,
        object::{
            Columns,
            Object,
            Rows,
        },
    },
};

#[derive(Debug, Serialize)]
pub struct Summary {
    #[serde(rename = "RepoStats")]
    pub repo_stats: Vec<RepoStat>,

    #[serde(rename = "RepoTotal")]
    repo_total: u64,

    #[serde(rename = "RepoInstalledTotal")]
    repo_installed_total: u64,

    #[serde(rename = "LocalInstalledTotal")]
    local_installed_total: u64,

    #[serde(rename = "PackagesNotInRepo")]
    pkgs_not_in_repo: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoStat {
    #[serde(rename = "Name")]
    name: String,

    #[serde(rename = "Total")]
    total_pkgs: u64,

    #[serde(rename = "Installed")]
    installed_pkgs: u64,

    #[serde(rename = "InstalledPercentage")]
    installed_pkgs_percent: PercentageValue,

    #[serde(rename = "OverallPercentage")]
    overall_percent: PercentageValue,

    #[serde(rename = "PackageList")]
    paclist: BTreeSet<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PercentageValue(Option<f64>);

impl Summary {
    pub fn new() -> Self {
        Self {
            repo_stats: Vec::new(),
            repo_total: 0,
            repo_installed_total: 0,
            local_installed_total: 0,
            pkgs_not_in_repo: Vec::new(),
        }
    }

    /// Generate repo stats
    pub fn build(&mut self, paclist: bool) -> Result<()> {
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
            let mut stats: HashMap<String, RepoStat> = alpm
                .syncdbs()
                .iter()
                .map(|repo| {
                    (
                        repo.name().to_owned(),
                        RepoStat::new(repo.name(), repo.pkgs().len() as u64, 0, BTreeSet::new()),
                    )
                })
                .collect();

            // Count installed packages from each repo
            for local_installed in alpm.localdb().pkgs() {
                let mut found = false;
                'search_in_repos: for db in alpm.syncdbs().iter() {
                    if db.pkg(local_installed.name()).is_ok() {
                        // Increase count for this repo
                        stats.get_mut(db.name()).unwrap().add_installed();

                        // Add local installed pkg to this repo
                        if paclist {
                            stats
                                .get_mut(db.name())
                                .unwrap()
                                .add_installed_pkg(local_installed.name());
                        }

                        found = true;
                        break 'search_in_repos;
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
        self.repo_total = self.repo_stats.iter().map(|stats| stats.total_pkgs).sum();

        self.repo_installed_total = self
            .repo_stats
            .iter()
            .map(|stats| stats.installed_pkgs)
            .sum::<u64>();

        self.repo_stats.push(RepoStat::new(
            "",
            self.repo_total,
            self.repo_installed_total,
            BTreeSet::new(),
        ));

        self.installed_percentage();
        self.overall_percentage();

        Ok(())
    }

    fn repo_stats_to_table(&self) -> Result<String> {
        let mut table_builder = Builder::with_capacity(&self.repo_stats.len() + 1, 5);
        table_builder.push_record(["Name", "Total", "Installed", "% Installed", "% Overall"]);
        for stats in &self.repo_stats {
            table_builder.push_record([
                stats.name.to_owned(),
                stats.total_pkgs.to_string(),
                stats.installed_pkgs.to_string(),
                stats.installed_pkgs_percent.to_string(),
                stats.overall_percent.to_string(),
            ]);
        }

        let mut table = table_builder.build();
        table
            .with(Style::re_structured_text())
            .with(Style::re_structured_text())
            .with(Modify::new(ByColumnName::new("Name")).with(Alignment::left()))
            .with(Modify::new(ByColumnName::new("Total")).with(Alignment::right()))
            .with(Modify::new(ByColumnName::new("Installed")).with(Alignment::right()))
            .with(Modify::new(ByColumnName::new("% Installed")).with(Alignment::right()))
            .with(Modify::new(ByColumnName::new("% Overall")).with(Alignment::right()))
            .with(
                Modify::new(Rows::last().intersect(Columns::new(1..=4)))
                    .with(Format::content(|s| format!("({})", s))),
            );

        Ok(table.to_string())
    }
}

impl RepoStat {
    pub fn new(name: &str, total: u64, installed: u64, packages: BTreeSet<String>) -> Self {
        Self {
            name: name.to_owned(),
            total_pkgs: total,
            installed_pkgs: installed,
            installed_pkgs_percent: PercentageValue(None),
            overall_percent: PercentageValue(None),
            paclist: packages,
        }
    }

    /// Increase count of installed packages
    pub fn add_installed(&mut self) {
        self.installed_pkgs += 1;
    }

    /// Add installed pkg in list
    pub fn add_installed_pkg(&mut self, pkg_name: &str) {
        self.paclist.insert(pkg_name.into());
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
    fn installed_percentage(&mut self);
    fn overall_percentage(&mut self);
}

impl Percentage for RepoStat {
    fn installed_percentage(&mut self) {
        if self.total_pkgs == 0 {
            self.installed_pkgs_percent = PercentageValue(None);
            return;
        }

        self.installed_pkgs_percent = PercentageValue(Some(
            (self.installed_pkgs as f64) * 100_f64 / (self.total_pkgs as f64),
        ));
    }

    fn overall_percentage(&mut self) {
        unreachable!();
    }
}

impl Percentage for Summary {
    fn installed_percentage(&mut self) {
        self.repo_stats
            .iter_mut()
            .for_each(|repo| repo.installed_percentage());
    }

    fn overall_percentage(&mut self) {
        if self.repo_total == 0 {
            self.repo_stats
                .iter_mut()
                .for_each(|repo| repo.overall_percent = PercentageValue(None));
            return;
        }

        self.repo_stats.iter_mut().for_each(|repo| {
            let overall = (repo.installed_pkgs as f64) * 100_f64 / (self.repo_total as f64);
            repo.overall_percent = PercentageValue(Some(overall));
        });
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use tabled::assert::assert_table;

    use super::*;

    fn setup_figure() -> Summary {
        let mut summary = Summary::new();
        summary.repo_stats.push(RepoStat::new(
            "core",
            1234,
            234,
            BTreeSet::from([
                "acl".to_owned(),
                "archlinux-keyring".to_owned(),
                "attr".to_owned(),
                "audit".to_owned(),
                "autoconf".to_owned(),
                "automake".to_owned(),
                "base".to_owned(),
                "base-devel".to_owned(),
                "bash".to_owned(),
            ]),
        ));

        summary
            .repo_stats
            .push(RepoStat::new("community", 4567, 456, BTreeSet::new()));

        summary
            .repo_stats
            .push(RepoStat::new("extra", 8999, 555, BTreeSet::new()));

        let _ = summary.finalize();
        summary
    }

    #[test]
    fn test_repo_stats_to_table() {
        let summary = setup_figure();
        let table = summary.repo_stats_to_table().unwrap();
        assert_table!(
            table,
            "=========== ========= =========== ============= ==========="
            " Name          Total   Installed   % Installed   % Overall "
            "=========== ========= =========== ============= ==========="
            " core           1234         234         18.96        1.58 "
            " community      4567         456          9.98        3.08 "
            " extra          8999         555          6.17        3.75 "
            "             (14800)      (1245)        (8.41)      (8.41) "
            "=========== ========= =========== ============= ==========="
        );
    }

    #[test]
    fn test_report_to_json() {
        let summary = setup_figure();
        let summary_json = serde_json::to_string(&summary).unwrap();
        let mut expected = json!(
            {
                "RepoStats": [
                    {
                        "Name": "core",
                        "Total": 1234,
                        "Installed": 234,
                        "InstalledPercentage": 18.962722852512155,
                        "OverallPercentage": 1.5810810810810811,
                        "PackageList": [
                                "acl",
                                "archlinux-keyring",
                                "attr",
                                "audit",
                                "autoconf",
                                "automake",
                                "base",
                                "base-devel",
                                "bash"
                        ]
                    },
                    {
                        "Name": "community",
                        "Total": 4567,
                        "Installed": 456,
                        "InstalledPercentage": 9.984672651631268,
                        "OverallPercentage": 3.081081081081081,
                        "PackageList": []
                    },
                    {
                        "Name": "extra",
                        "Total": 8999,
                        "Installed": 555,
                        "InstalledPercentage": 6.167351927991999,
                        "OverallPercentage": 3.75,
                        "PackageList": []
                    },
                    {
                        "Name": "",
                        "Total": 14800,
                        "Installed": 1245,
                        "InstalledPercentage": 8.412162162162161,
                        "OverallPercentage": 8.412162162162161,
                        "PackageList": []
                    }
                ],
                "RepoTotal": 14800,
                "RepoInstalledTotal": 1245,
                "LocalInstalledTotal": 0,
                "PackagesNotInRepo": []
            }
        );
        assert_eq!(summary_json, expected.to_string());
    }
}
