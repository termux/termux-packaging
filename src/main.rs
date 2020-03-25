extern crate ar;
extern crate libflate;
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
mod cmd_package_apk;
mod deb_file;

#[derive(StructOpt, Debug)]
#[structopt(name = "termux-packaging")]
#[structopt(setting(structopt::clap::AppSettings::ColoredHelp))]
/// Termux packaging tools.
enum Opt {
    #[structopt(name = "bootstraps")]
    /// Create Android 10 bootstrap zips using packages from bintray
    Bootstraps {
        /// Version number to use in generated zip files
        version: u16,
        /// Output directory to create the zip files in
        directory: String,
    },
    #[structopt(name = "checkrepo")]
    /// Check a local repository for problems
    CheckRepo {
        /// Path to directory containing binary-* files
        directory: String,
    },
    #[structopt(name = "debinfo")]
    /// Show information about a deb file
    DebInfo {
        /// The .deb file to inspect
        #[structopt(name = "DEBFILE")]
        file: String,
    },
    #[structopt(name = "notfound")]
    /// Update the command-not-found headers
    NotFound {
        /// A directory containing packages to scan for binaries
        repo: String,
        /// The directory where the commands-$ARCH.h files will be created
        output: String,
    },
    #[structopt(name = "package-apk")]
    /// Create Android 10 package APK file
    PackageApk {
        /// If gradle installDebug should be executed immediately
        #[structopt(short, long)]
        install: bool,
        /// The package name
        package: String,
        /// The directory where the generated project will be created
        output: String,
    },
}

fn main() {
    match Opt::from_args() {
        Opt::Bootstraps { directory, version } => cmd_bootstraps::create(&directory, version),
        Opt::CheckRepo { directory } => cmd_checkrepo::check(&directory),
        Opt::DebInfo { file } => cmd_debinfo::print(&file),
        Opt::NotFound { repo, output } => cmd_notfound::update(repo, &output),
        Opt::PackageApk {
            install,
            package,
            output,
        } => cmd_package_apk::create_apk(&package, &output, install),
    }
}
