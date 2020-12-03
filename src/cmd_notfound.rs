use crate::deb_file;
use std::collections::HashMap;
use std::fs::{metadata, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::exit;
use walkdir::WalkDir;

pub struct CommandsNotFoundVisitor {
    pub current_arch: String,
    pub arch_files: HashMap<String, File>,
    current_package: String,
    first_file: bool,
}

impl CommandsNotFoundVisitor {
    fn write_arch_line(&mut self, line: &str) {
        let writer = |arch_name: &str, arch_files: &mut HashMap<String, File>| {
            let file = arch_files.get_mut(&arch_name.to_string()).unwrap();
            if let Err(e) = file.write(line.as_bytes()) {
                eprintln!("Unable to write to file: {}", e);
                exit(1);
            }
        };

        if self.current_arch == "all" {
            for arch in &["arm", "aarch64", "i686", "x86_64"] {
                writer(arch, &mut self.arch_files);
            }
        } else {
            writer(&self.current_arch, &mut self.arch_files);
        }
    }
}

impl deb_file::DebVisitor for CommandsNotFoundVisitor {
    fn visit_control(&mut self, fields: HashMap<String, String>) {
        self.current_arch = fields["Architecture"].to_string();
        self.first_file = true;
        self.current_package = fields["Package"].clone();
    }

    fn visit_file(&mut self, file: &mut tar::Entry<impl Read>) {
        let header = file.header();
        if header.entry_type() != tar::EntryType::Regular
            && header.entry_type() != tar::EntryType::Symlink
        {
            return;
        }

        let pp = file.path().unwrap();
        let file_path = pp.to_str().unwrap();

        if let Some(file_name) = file_path.strip_prefix("./data/data/com.termux/files/usr/bin/") {
            if self.first_file {
                self.first_file = false;
                let line: &str = &format!("\"{}\",\n", &mut self.current_package);
                self.write_arch_line(line);
            }

            let file_name = if let Some(stripped) = file_name.strip_prefix("applets/") {
                stripped
            } else {
                file_name
            };
            let line: &str = &format!("\" {}\",\n", file_name);
            self.write_arch_line(line);
        }
    }
}

pub fn update(repo_dir: String, output_dir: &str) {
    match metadata(&output_dir) {
        Ok(attr) => {
            if !attr.is_dir() {
                eprintln!("Output dir '{}' is not a directory", output_dir);
                exit(1);
            }
        }
        Err(e) => {
            eprintln!("Output dir '{}' error: {}", output_dir, e);
            exit(1);
        }
    }
    let path = PathBuf::from(&output_dir);

    let mut output_files = HashMap::new();
    {
        let mut open_arch = |arch| {
            output_files.insert(
                String::from(arch),
                OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(path.join(format!("commands-{}.h", arch)))
                    .unwrap(),
            );
        };

        open_arch("arm");
        open_arch("aarch64");
        open_arch("i686");
        open_arch("x86_64");
    }

    let mut deb_visitor = CommandsNotFoundVisitor {
        current_arch: "arm".to_string(),
        arch_files: output_files,
        first_file: true,
        current_package: String::from("FIXME"),
    };

    for entry in WalkDir::new(repo_dir).sort_by(|a, b| a.file_name().cmp(b.file_name())) {
        let entry = entry.unwrap();
        if entry.file_name().to_str().unwrap().ends_with(".deb") {
            let mut deb_file = File::open(entry.path()).unwrap();
            deb_file::visit_files(&mut deb_file, &mut deb_visitor);
        }
    }
}
