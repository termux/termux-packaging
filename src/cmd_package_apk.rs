use crate::apt_repo::fetch_repo;
use crate::deb_file::{visit_files, DebVisitor};
use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::fs::{rename, File};
use std::io::{copy, ErrorKind, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, RwLock};
use std::thread;

pub struct CreateApkVisitor {
    output_directory: String,
    counter: u32,
    file_mapping: String,
    symlinks: String,
}

impl DebVisitor for CreateApkVisitor {
    fn visit_control(&mut self, _: HashMap<String, String, RandomState>) {
        // Do nothing.
    }

    fn visit_file(&mut self, file: &mut tar::Entry<impl Read>) {
        //let file_path_full: String;
        let header = file.header();
        let is_symlink = header.entry_type() == tar::EntryType::Symlink;
        let is_regular = header.entry_type() == tar::EntryType::Regular;
        if !(is_regular || is_symlink) {
            return;
        }

        let pp = file.path().unwrap();
        let file_path = pp.to_str().unwrap();
        let relative_path = &file_path[33..];
        //file_path_full = String::from(&file_path[2..]);
        if is_symlink {
            if !self.symlinks.is_empty() {
                self.symlinks = format!("{}\n", self.symlinks);
            }
            self.symlinks = format!(
                "{}{}←{}",
                self.symlinks,
                header.link_name().unwrap().unwrap().to_str().unwrap(),
                relative_path
            );
        } else {
            if !self.file_mapping.is_empty() {
                self.file_mapping = format!("{}\n", self.file_mapping);
            }
            self.file_mapping =
                format!("{}{}.so←{}", self.file_mapping, self.counter, relative_path);

            let file_path = format!("{}/{}.so", self.output_directory, self.counter);
            let mut output = File::create(file_path).unwrap();
            copy(file, &mut output).unwrap();
            self.counter += 1;
        }
    }
}

fn write_bytes_to_file(path: &str, file_content: &[u8]) {
    let mut output = File::create(path).unwrap();
    output.write_all(file_content).unwrap();
}

fn write_string_to_file(path: &str, file_content: &str) {
    write_bytes_to_file(path, file_content.as_bytes());
}

fn create_dir(path: &str) {
    match std::fs::create_dir_all(path) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            eprintln!("Output directory already exists: {}", path);
            std::process::exit(1);
        }
        Err(error) => panic!("{}", error.to_string()),
    }
}

pub fn create_apk(package_name: &str, output_dir: &str, install: bool) {
    create_dir(output_dir);
    create_dir(&format!("{}/app/src/main", output_dir));
    create_dir(&format!("{}/gradle/wrapper", output_dir));

    let android_manifest = include_str!("AndroidManifest.xml");
    // Android package names cannot have dashes in them:
    let android_package_name = &package_name.replace("-", "");
    let android_manifest = android_manifest.replace("PACKAGE_NAME", android_package_name);

    write_bytes_to_file(
        &format!("{}/build.gradle", output_dir),
        include_bytes!("build.gradle"),
    );
    let gradlew_path = format!("{}/gradlew", output_dir);
    write_bytes_to_file(&gradlew_path, include_bytes!("gradlew"));
    let mut perms = std::fs::metadata(&gradlew_path).unwrap().permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(gradlew_path, perms).unwrap();

    write_bytes_to_file(
        &format!("{}/gradle/wrapper/gradle-wrapper.jar", output_dir),
        include_bytes!("gradle-wrapper.jar"),
    );
    write_bytes_to_file(
        &format!("{}/gradle/wrapper/gradle-wrapper.properties", output_dir),
        include_bytes!("gradle-wrapper.properties"),
    );
    write_string_to_file(&format!("{}/settings.gradle", output_dir), "include ':app'");
    write_bytes_to_file(
        &format!("{}/app/dev_keystore.jks", output_dir),
        include_bytes!("dev_keystore.jks"),
    );
    write_string_to_file(
        &format!("{}/app/build.gradle", output_dir),
        &include_str!("app-build.gradle").replace("PACKAGE_NAME", android_package_name),
    );
    write_string_to_file(
        &format!("{}/app/src/main/AndroidManifest.xml", output_dir),
        &android_manifest,
    );

    let arch_all_packages = fetch_repo("all");
    let arch_all_packages = Arc::new(RwLock::new(arch_all_packages));

    let mut join_handles = Vec::new();
    for arch in &["arm", "aarch64", "i686", "x86_64"] {
        // x86', 'x86_64', 'armeabi-v7a', 'arm64-v8a
        let android_abi_name = match *arch {
            "arm" => "armeabi-v7a",
            "aarch64" => "arm64-v8a",
            "i686" => "x86",
            "x86_64" => "x86_64",
            _ => {
                panic!();
            }
        };
        create_dir(&format!(
            "{}/app/src/main/jniLibs/{}",
            output_dir, android_abi_name
        ));
        let my_arch_all_packages = Arc::clone(&arch_all_packages);

        let output_dir = output_dir.to_string();
        let package_name = package_name.to_string();
        join_handles.push(thread::spawn(move || {
            let http_client = reqwest::blocking::Client::new();
            let packages = fetch_repo(arch);
            let arch_all = my_arch_all_packages.read().unwrap();
            let bootstrap_package = packages
                .get(&package_name)
                .or_else(|| arch_all.get(&package_name))
                .unwrap_or_else(|| panic!("Cannot find package '{}'", package_name));
            let package_url = bootstrap_package.package_url();

            let mut response = http_client
                .get(&package_url)
                .send()
                .unwrap_or_else(|_| panic!("Failed fetching {}", package_url));
            let mut visitor = CreateApkVisitor {
                output_directory: format!(
                    "{}/app/src/main/jniLibs/{}",
                    output_dir, android_abi_name
                ),
                counter: 100,
                file_mapping: String::new(),
                symlinks: String::new(),
            };
            visit_files(&mut response, &mut visitor);

            write_string_to_file(
                &format!(
                    "{}/app/src/main/jniLibs/{}/files.so",
                    output_dir, android_abi_name
                ),
                &visitor.file_mapping,
            );
            write_string_to_file(
                &format!(
                    "{}/app/src/main/jniLibs/{}/symlinks.so",
                    output_dir, android_abi_name
                ),
                &visitor.symlinks,
            );
        }));
    }
    for handle in join_handles {
        handle.join().unwrap();
    }

    let path_to_gradlew = std::fs::canonicalize(format!("{}/gradlew", output_dir)).unwrap();
    if install {
        println!("Executing {:?}", path_to_gradlew);
        std::process::Command::new(path_to_gradlew)
            .args(&["installDebug"])
            .current_dir(output_dir)
            .spawn()
            .expect("failed to execute process");
    } else {
        println!("Executing {:?}", path_to_gradlew);
        std::process::Command::new(path_to_gradlew)
            .args(&["assembleDebug"])
            .current_dir(output_dir)
            .spawn()
            .expect("failed to execute process")
            .wait()
            .expect("failed to wait on child");
        rename(
            format!("{}/app/build/outputs/apk/debug/app-debug.apk", output_dir),
            format!("{}.apk", output_dir),
        )
        .expect("failed renaming apk file");
        println!("Created {}.apk", output_dir);
    }
}
