use crate::deb_file;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::Read;

pub fn print(file_path: &str) {
    let mut deb_file = File::open(file_path).unwrap();

    struct PrintControlVisitor {};
    impl deb_file::DebVisitor for PrintControlVisitor {
        fn visit_control(&mut self, fields: HashMap<String, String>) {
            let sorted_map: BTreeMap<_, _> = fields.iter().collect();
            for (key, value) in &sorted_map {
                println!("{}: {}", key, value);
            }
        }

        fn visit_file(&mut self, _: &mut tar::Entry<impl Read>) {
            // Ignore
        }
    };
    let mut visitor = PrintControlVisitor {};
    deb_file::visit_files(&mut deb_file, &mut visitor);
}
