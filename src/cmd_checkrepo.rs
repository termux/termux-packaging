use crate::deb_file;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path;
use std::process;

struct CheckRepoVisitor {
    current_package_name: String,
    files_to_package: HashMap<String, String>,
}

impl CheckRepoVisitor {
    fn new() -> CheckRepoVisitor {
        CheckRepoVisitor {
            current_package_name: String::from(""),
            files_to_package: HashMap::new(),
        }
    }
}

impl deb_file::DebVisitor for CheckRepoVisitor {
    fn visit_control(&mut self, fields: HashMap<String, String>) {
        self.current_package_name = fields["Package"].clone();
    }

    fn visit_file(&mut self, file: &mut tar::Entry<impl Read>) {
        let path = String::from(file.path().unwrap().to_str().unwrap());
        let path_copy = path.clone();

        let entry_type = file.header().entry_type();
        if entry_type == tar::EntryType::Link {
            println!(
                "Invalid link {} in package {}",
                path, self.current_package_name
            );
            return;
        }

        if !(entry_type == tar::EntryType::Regular || entry_type == tar::EntryType::Symlink) {
            return;
        }

        match self
            .files_to_package
            .insert(path, self.current_package_name.clone())
        {
            None => {}
            Some(existing) => {
                println!(
                    "Duplicated file {} in both {} and {}",
                    path_copy, self.current_package_name, existing
                );
            }
        }
    }
}

pub fn check(path: &str) {
    let path = path::Path::new(path);
    if !path.is_dir() {
        eprintln!("Not a directory: {}", path.to_str().unwrap());
        process::exit(1);
    }
    let arches = ["arm", "aarch64", "i686", "x86_64", "all"];
    for arch in &arches {
        let mut visitor = CheckRepoVisitor::new();
        let arch_path = path.join(format!("binary-{}", arch));

        println!("Checking {:?}", arch_path);

        for entry in
            fs::read_dir(&arch_path).unwrap_or_else(|_| panic!("No such dir: {:?}", &arch_path))
        {
            let entry = entry.unwrap();
            if entry.file_name().to_str().unwrap().ends_with(".deb") {
                let path = entry.path();
                let mut deb_file = fs::File::open(entry.path()).unwrap();
                println!("Checking {:?}", path);
                deb_file::visit_files(&mut deb_file, &mut visitor);
            }
        }
    }
}
