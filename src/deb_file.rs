use ar;
use libflate;
use lzma;
use std;
use std::collections::HashMap;
use std::io::{BufRead, Read};
use tar;

pub trait DebVisitor {
    fn visit_control(&mut self, fields: HashMap<String, String>);
    fn visit_conffiles(&mut self, _file: &mut tar::Entry<impl Read>) {
        // Default implementation does nothing.
    }
    fn visit_file(&mut self, file: &mut tar::Entry<impl Read>);
}

enum ControlTarEntryType {
    Control,
    Conffiles,
    Other,
}

fn parse_control_ar_entry(
    xz_encoded: bool,
    ar_entry: ar::Entry<impl std::io::Read>,
    visitor: &mut impl DebVisitor,
) -> HashMap<String, String> {
    let mut map = HashMap::new();

    let reader: Box<dyn Read> = if xz_encoded {
        Box::new(lzma::reader::LzmaReader::new_decompressor(ar_entry).expect("Error decompressing"))
    } else {
        Box::new(libflate::gzip::Decoder::new(ar_entry).expect("Error decompressing"))
    };
    let mut control_tar = tar::Archive::new(reader);
    for file in control_tar.entries().unwrap() {
        let mut file = file.unwrap();

        let entry_type = {
            let path = file.path().expect("Error reading path");
            let path_str = path.to_str().expect("Could not read path");
            match path_str {
                "./control" => ControlTarEntryType::Control,
                "./conffiles" => ControlTarEntryType::Conffiles,
                _ => ControlTarEntryType::Other,
            }
        };
        match entry_type {
            ControlTarEntryType::Control => {
                for line in std::io::BufReader::new(file).lines() {
                    let line = line.unwrap();
                    if !line.starts_with(' ') {
                        let mut splitter = line[..].splitn(2, ": ");
                        let key = splitter.next().unwrap();
                        let value = splitter.next().unwrap();
                        map.insert(String::from(key), String::from(value));
                    };
                }
            }
            ControlTarEntryType::Conffiles => {
                visitor.visit_conffiles(&mut file);
            }
            _ => {}
        }
    }
    map
}

fn visit_data_tar_files(ar_entry: ar::Entry<impl std::io::Read>, visitor: &mut impl DebVisitor) {
    let reader = lzma::reader::LzmaReader::new_decompressor(ar_entry).expect("Error decompressing");
    let mut control_tar = tar::Archive::new(reader);
    for file in control_tar.entries().unwrap() {
        let mut file = file.unwrap();
        visitor.visit_file(&mut file);
    }
}

pub fn visit_files(reader: &mut impl std::io::Read, visitor: &mut impl DebVisitor) {
    let mut archive = ar::Archive::new(reader);
    while let Some(entry_result) = archive.next_entry() {
        let entry = entry_result.unwrap();
        let mut control_tar = false;
        let mut data_tar = false;
        let mut control_tar_xz = false;

        let entry_name = std::str::from_utf8(entry.header().identifier()).unwrap();
        if "control.tar.gz" == entry_name || "control.tar.xz" == entry_name {
            control_tar = true;
            control_tar_xz = "control.tar.xz" == entry_name;
        } else if "data.tar.xz" == entry_name {
            data_tar = true;
        }

        if control_tar {
            let control = parse_control_ar_entry(control_tar_xz, entry, visitor);
            visitor.visit_control(control);
        } else if data_tar {
            visit_data_tar_files(entry, visitor);
        }
    }
}
