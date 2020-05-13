use crate::apt_repo::fetch_repo;
use crate::deb_file::{visit_files, DebVisitor};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{copy, Read, Result, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::vec::Vec;
use zip::write::{FileOptions, ZipWriter};

struct TeeReader<'a, R: 'a + Read, W: 'a + Write> {
    reader: &'a mut R,
    writer: &'a mut W,
}

impl<'a, R: Read, W: Write> Read for TeeReader<'a, R, W> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = self.reader.read(buf)?;
        self.writer.write_all(&buf[..n])?;
        Ok(n)
    }
}

pub struct CreateBootstrapVisitor {
    zip_writer: ZipWriter<File>,
    conffiles: Vec<u8>,
    symlinks_txt: Vec<u8>,
}

fn write_zip_file(
    zip_writer: &mut ZipWriter<File>,
    file_name: &str,
    file_contents: &mut impl Read,
) {
    zip_writer
        .start_file(file_name, FileOptions::default())
        .unwrap_or_else(|err| panic!("Error starting {} zip entry: {}", file_name, err));
    copy(file_contents, zip_writer)
        .unwrap_or_else(|err| panic!("Error writing {} zip entry: {}", file_name, err));
}

impl DebVisitor for CreateBootstrapVisitor {
    fn visit_control(&mut self, _fields: HashMap<String, String>) {}

    fn visit_conffiles(&mut self, file: &mut tar::Entry<impl Read>) {
        copy(file, &mut self.conffiles).expect("Error copying conffiles");
    }

    fn visit_file(&mut self, file: &mut tar::Entry<impl Read>) {
        {
            let header = file.header();
            let is_symlink = header.entry_type() == tar::EntryType::Symlink;
            let is_regular = header.entry_type() == tar::EntryType::Regular;
            if !(is_regular || is_symlink) {
                return;
            }

            let pp = file.path().unwrap();
            let file_path = pp.to_str().unwrap();
            let relative_path = &file_path[33..];

            if is_symlink {
                self.symlinks_txt
                    .write_all(
                        format!(
                            "{}←{}\n",
                            header.link_name().unwrap().unwrap().to_str().unwrap(),
                            relative_path
                        )
                        .as_bytes(),
                    )
                    .expect("Error appending to SYMLINKS.txt");
                return;
            }

            self.zip_writer
                .start_file(relative_path, FileOptions::default())
                .expect("Error writing to zip");
        }

        let mut md5_context = md5::Context::new();
        {
            let mut tee = TeeReader {
                reader: file,
                writer: &mut md5_context,
            };
            copy(&mut tee, &mut self.zip_writer).expect("Error copying from tar to zip");
        }
    }
}

pub fn create(output: &str, version: u16) {
    let path = PathBuf::from(output);

    let bootstrap_packages = Arc::new(vec![
        "bash",
        "busybox",
        "ca-certificates",
        "coreutils",
        "curl",
        "dash",
        "grep",
        "less",
        "libandroid-support",
        "libbz2",
        "libcurl",
        "libgmp",
        "libiconv",
        "liblzma",
        "libnghttp2",
        "libtalloc",
        "ncurses",
        "openssl",
        "proot",
        "readline",
        "sed",
        "termux-am",
        "termux-exec",
        "termux-tools",
        "zlib",
    ]);

    let arch_all_packages = Arc::new(fetch_repo("all"));

    let mut join_handles = Vec::new();

    for arch in &["arm", "aarch64", "i686", "x86_64"] {
        let my_path = path.clone();
        let my_arch_all_packages = Arc::clone(&arch_all_packages);
        let my_bootstrap_packages = Arc::clone(&bootstrap_packages);
        join_handles.push(thread::spawn(move || {
            let http_client = reqwest::blocking::Client::new();

            let output_zip_path =
                my_path.join(format!("android10-v{}-bootstrap-{}.zip", version, arch));
            let output_zip_file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(output_zip_path)
                .expect("Cannot open zip for writing");

            let mut visitor = CreateBootstrapVisitor {
                zip_writer: ZipWriter::new(output_zip_file),
                conffiles: Vec::new(),
                symlinks_txt: Vec::new(),
            };

            // The app needs directories to appear before files.
            for dir in vec!["tmp/"].iter() {
                visitor
                    .zip_writer
                    .add_directory(*dir, FileOptions::default())
                    .expect("Error creating dir");
            }

            // This needs to be an empty file, else removing a package fails:
            visitor
                .zip_writer
                .start_file("var/lib/dpkg/available", FileOptions::default())
                .expect("Unable to create var/lib/dpkg/available");

            let packages = fetch_repo(arch);
            for bootstrap_package_name in my_bootstrap_packages.iter() {
                let bootstrap_package = packages
                    .get(*bootstrap_package_name)
                    .or_else(|| my_arch_all_packages.get(*bootstrap_package_name))
                    .unwrap_or_else(|| panic!("Cannot find package '{}'", bootstrap_package_name));

                let package_url = bootstrap_package.package_url();

                let mut response = http_client
                    .get(&package_url)
                    .send()
                    .unwrap_or_else(|_| panic!("Failed fetching {}", package_url));

                visitor.conffiles.clear();
                visit_files(&mut response, &mut visitor);

                {
                    if !visitor.conffiles.is_empty() {
                        let conffiles_path =
                            format!("var/lib/dpkg/info/{}.conffiles", bootstrap_package_name);
                        write_zip_file(
                            &mut visitor.zip_writer,
                            conffiles_path.as_str(),
                            &mut &visitor.conffiles[..],
                        );
                    }
                }
            }

            write_zip_file(
                &mut visitor.zip_writer,
                "SYMLINKS.txt",
                &mut &visitor.symlinks_txt[..],
            );
        }));
    }

    for handle in join_handles {
        handle.join().unwrap();
    }
}
