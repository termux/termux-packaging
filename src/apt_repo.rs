use std::collections::HashMap;
use std::io::{BufRead, BufReader};

pub struct Package {
    pub fields: HashMap<String, String>,
}

impl Package {
    pub fn package_url(&self) -> String {
        return format!(
            "https://dl.bintray.com/termux/termux-packages-24/{}",
            self.fields.get("Filename").expect("No 'Filename")
        );
    }
}

pub fn fetch_repo(arch: &str) -> HashMap<String, Package> {
    let url = format!(
        "https://dl.bintray.com/termux/termux-packages-24/dists/stable/main/binary-{}/Packages",
        arch
    );

    match reqwest::blocking::get(&url) {
        Ok(response) => {
            let reader = BufReader::new(response);
            parse_packages(reader)
        }
        Err(error) => panic!("Error fetching {}: {:?}", url, error),
    }
}

fn parse_packages(reader: impl BufRead) -> HashMap<String, Package> {
    let mut result: HashMap<String, Package> = HashMap::new();
    let mut current_package: HashMap<String, String> = HashMap::new();
    for line in reader.lines() {
        let line = line.expect("Failed reading packages");
        if line.is_empty() {
            let package_name = current_package["Package"].clone();
            let package = Package {
                fields: current_package,
            };
            result.insert(package_name, package);
            current_package = HashMap::new();
        } else if line.starts_with(' ') {
            // Ignore multiline (probably description) field for now.
        } else {
            let mut parts = line.splitn(2, ':');
            let key = parts.next().unwrap().to_string();
            let value = parts.next().unwrap().to_string().trim().to_string();
            current_package.insert(key, value);
        }
    }

    if !current_package.is_empty() {
        let package_name = current_package["Package"].clone();
        let package = Package {
            fields: current_package,
        };
        result.insert(package_name, package);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_packages() {
        let packages_str = "Package: aapt
Architecture: aarch64
Installed-Size: 2772
Maintainer: Fredrik Fornwall @fornwall
Version: 7.1.2.33-2
Description: Android Asset Packaging Tool
Homepage: http://elinux.org/Android_aapt
Depends: libexpat, libpng, libzopfli
Filename: dists/stable/main/binary-aarch64/aapt_7.1.2.33-2_aarch64.deb
Size: 1750040
SHA256: a149c1783e06784584a57ccf73a7f27a179367e602a9dc21d95f6adb65809af2

Package: abduco
Architecture: aarch64
Installed-Size: 68
Maintainer: Fredrik Fornwall <fredrik@fornwall.net>
Version: 0.6
Description: Clean and simple terminal session manager
Homepage: http://www.brain-dump.org/projects/abduco/
Depends: libutil,dvtm
Filename: dists/stable/main/binary-aarch64/abduco_0.6_aarch64.deb
Size: 8964
SHA256: 1adc99a0257cec154bdc89a1c6974ecc373b6bd4018e5851d15d28e7cdf57ad3";
        let cursor = Cursor::new(packages_str);
        let packages = parse_packages(cursor);

        let aapt_package = &packages["aapt"];
        assert_eq!("aapt", aapt_package.fields["Package"]);
        assert_eq!("2772", aapt_package.fields["Installed-Size"]);

        let abduco_package = &packages["abduco"];
        assert_eq!("abduco", abduco_package.fields["Package"]);
        assert_eq!("68", abduco_package.fields["Installed-Size"]);
    }

    #[test]
    fn test_fetch_repo() {
        let packages = fetch_repo("aarch64");

        let abduco_package = &packages["abduco"];
        assert_eq!(
            "Clean and simple terminal session manager",
            abduco_package.fields["Description"]
        );
    }
}
