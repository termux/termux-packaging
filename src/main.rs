extern crate ar;
extern crate lzma;
extern crate md5;
extern crate reqwest;
extern crate structopt;
extern crate tar;
extern crate walkdir;
extern crate zip;

use structopt::StructOpt;

mod apt_repo;
mod cmd_bootstraps;
mod cmd_checkrepo;
mod cmd_debinfo;
mod cmd_notfound;
mod deb_file;

#[derive(StructOpt, Debug)]
#[structopt(name = "termux-packaging", author = "")]
#[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
/// Termux packaging tools.
enum Opt {
    #[structopt(name = "bootstraps", author = "")]
    /// Create bootstrap zips using packages from termux.net
    Bootstraps {
        /// Output directory to create the zip files in
        directory: String,
    },
    #[structopt(name = "checkrepo", author = "")]
    /// Check a local repository for problems
    CheckRepo {
        /// Path to directory containing binary-* files
        directory: String,
    },
    #[structopt(name = "debinfo", author = "")]
    /// Show information about a deb file
    DebInfo {
        /// The .deb file to inspect
        #[structopt(name = "DEBFILE")]
        file: String,
    },
    #[structopt(name = "notfound", author = "")]
    /// Update the command-not-found headers
    NotFound {
        /// A directory containing packages to scan for binaries
        repo: String,
        /// The directory where the commands-$ARCH.h files will be created
        output: String,
    },
}

fn main() {
    match Opt::from_args() {
        Opt::Bootstraps { directory } => cmd_bootstraps::create(&directory),
        Opt::CheckRepo { directory } => cmd_checkrepo::check(&directory),
        Opt::DebInfo { file } => cmd_debinfo::print(&file),
        Opt::NotFound { repo, output } => cmd_notfound::update(repo, &output),
    }
}
