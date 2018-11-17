use apt_repo::fetch_repo;
use deb_file::{visit_files, DebVisitor};
use md5;
use reqwest;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{copy, Read, Result, Write};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::thread;
use std::vec::Vec;
use tar;
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
    dpkg_status: Vec<u8>,
    symlinks_txt: Vec<u8>,
    package_digests: Vec<u8>,
    file_entries: HashSet<String>,
}

fn write_zip_file(zip_writer: &mut ZipWriter<File>, file_name: &str, file_contents: &mut Read) {
    zip_writer
        .start_file(file_name, FileOptions::default())
        .unwrap_or_else(|err| panic!("Error starting {} zip entry: {}", file_name, err));
    copy(file_contents, zip_writer)
        .unwrap_or_else(|err| panic!("Error writing {} zip entry: {}", file_name, err));
}

impl DebVisitor for CreateBootstrapVisitor {
    fn visit_control(&mut self, fields: HashMap<String, String>) {
        self.package_digests.clear();
        self.file_entries.clear();

        for (key, value) in &fields {
            match key.as_str() {
                "Filename" | "MD5Sum" | "SHA1" | "SHA256" | "Size" => {
                    continue;
                }
                _ => {
                    self.dpkg_status
                        .write_all(format!("{}: {}\n", key, value).as_bytes())
                        .expect("Error writing to dpkg/status");
                }
            }
        }
        self.dpkg_status
            .write_all(b"Status: install ok installed\n\n")
            .expect("Error writing to dpkg/status")
    }

    fn visit_file<T>(&mut self, file: &mut tar::Entry<T>)
    where
        T: Read,
    {
        let file_path_full: String;
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
            file_path_full = String::from(&file_path[2..]);

            if is_symlink {
                self.symlinks_txt
                    .write_all(
                        format!(
                            "{}←{}\n",
                            header.link_name().unwrap().unwrap().to_str().unwrap(),
                            relative_path
                        ).as_bytes(),
                    ).expect("Error appending to SYMLINKS.txt");
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

        self.file_entries.insert(file_path_full.clone());

        let digest = md5_context.compute();
        self.package_digests
            .write_all(format!("{:x}  {}\n", digest, file_path_full).as_bytes())
            .expect("Error writing to package digest");
    }
}

pub fn create(output: &str) {
    let path = PathBuf::from(output);

    let bootstrap_packages = [
        // Having bash as shell:
        "bash",
        "readline",
        "ncurses",
        "command-not-found",
        "termux-tools",
        // Needed for bin/sh:
        "dash",
        // For use by dpkg and apt:
        "liblzma",
        // Needed by dpkg:
        "libandroid-support",
        // dpkg uses tar (and wants 'find' in path for some operations):
        "busybox",
        // apt uses STL:
        "libc++",
        // apt now includes apt-transport-https:
        "ca-certificates",
        "openssl",
        "libnghttp2",
        "libcurl",
        // gnupg for package verification:
        "gpgv",
        "libgcrypt",
        "libgpg-error",
        "libbz2",
        // termux-exec fixes shebangs (and apt depends on it):
        "termux-exec",
        // Everyone needs a working "am" (and termux-tools depends on it):
        "termux-am",
        // For package management:
        "dpkg",
        "apt",
    ];

    let arch_all_packages = fetch_repo("all");
    let arch_all_packages = Arc::new(RwLock::new(arch_all_packages));

    let mut join_handles = Vec::new();

    for arch in &["arm", "aarch64", "i686", "x86_64"] {
        let my_arch_all_packages = Arc::clone(&arch_all_packages);
        let my_path = path.clone();
        join_handles.push(thread::spawn(move || {
            let http_client = reqwest::Client::new();

            let output_zip_path = my_path.join(format!("bootstrap-{}.zip", arch));
            let output_zip_file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(output_zip_path)
                .expect("Cannot open zip for writing");

            let mut visitor = CreateBootstrapVisitor {
                zip_writer: ZipWriter::new(output_zip_file),
                dpkg_status: Vec::new(),
                symlinks_txt: Vec::new(),
                package_digests: Vec::new(),
                // All files, directories and symlinks added, for "var/lib/dpkg/info/$PKG.list".
                file_entries: HashSet::new(),
            };

            // The app needs directories to appear before files.
            for dir in vec![
                "etc/apt/preferences.d/",
                "etc/apt/apt.conf.d/",
                "var/cache/apt/archives/partial/",
                "var/log/apt/",
                "tmp/",
                "var/lib/dpkg/triggers/",
                "var/lib/dpkg/updates/",
            ].iter()
            {
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
            for bootstrap_package_name in &bootstrap_packages {
                let arch_all = my_arch_all_packages.read().unwrap();
                let bootstrap_package = packages
                    .get(*bootstrap_package_name)
                    .or_else(|| arch_all.get(*bootstrap_package_name))
                    .unwrap_or_else(|| panic!("Cannot find package '{}'", bootstrap_package_name));

                let package_url = bootstrap_package.package_url();

                let mut response = http_client
                    .get(&package_url)
                    .send()
                    .unwrap_or_else(|_| panic!("Failed fetching {}", package_url));

                visit_files(&mut response, &mut visitor);

                {
                    let mut added_paths: HashSet<&str> = HashSet::new();
                    let mut list_buffer: Vec<u8> = Vec::new();
                    for path in &visitor.file_entries {
                        if added_paths.insert(path) {
                            list_buffer
                                .write_all(&format!("/{}\n", path).as_bytes())
                                .expect("Error writing to list buffer");
                        }
                    }

                    // dpkg wants folders to be present as well:
                    for path in &visitor.file_entries {
                        let mut slash_search = path.find('/');
                        while let Some(index) = slash_search {
                            let sub_string = &path[0..index];
                            if added_paths.insert(sub_string) {
                                list_buffer
                                    .write_all(&format!("/{}\n", sub_string).as_bytes())
                                    .expect("Error writing to list buffer");
                            }
                            slash_search = path[(index + 1)..].find('/');
                            if let Some(new_index) = slash_search {
                                slash_search = Some(new_index + index + 1);
                            }
                        }
                    }

                    let list_path = format!("var/lib/dpkg/info/{}.list", bootstrap_package_name);
                    write_zip_file(
                        &mut visitor.zip_writer,
                        list_path.as_str(),
                        &mut &list_buffer[..],
                    );
                }

                let digests_file_path =
                    format!("var/lib/dpkg/info/{}.md5sums", bootstrap_package_name);
                write_zip_file(
                    &mut visitor.zip_writer,
                    digests_file_path.as_str(),
                    &mut &visitor.package_digests[..],
                );
            }

            write_zip_file(
                &mut visitor.zip_writer,
                "var/lib/dpkg/status",
                &mut &visitor.dpkg_status[..],
            );
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
