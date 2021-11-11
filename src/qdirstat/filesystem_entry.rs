pub mod filesystem_entry_type;
pub mod filesystem_entry_extensions;

use filesystem_entry_type::FileSystemEntryType;
use filesystem_entry_extensions::*;

use std::fs;
use std::io;

use std::io::*;

pub struct FileSystemEntry {
    pub identifier: String,
    pub path_string: String,
    pub entry_type: FileSystemEntryType,
    pub len : u64,
    children: Vec::<FileSystemEntry>,
}

impl FileSystemEntry {
    pub fn new(name: &str, path: &std::path::Path, entry_type: FileSystemEntryType, size: u64) -> FileSystemEntry {
        FileSystemEntry {
            identifier: name.to_string(),
            path_string: path.as_os_str().to_str().expect("Could convert from OsString").to_string(),
            entry_type: entry_type,
            len: size,
            children: Vec::<FileSystemEntry>::new()
        }
    }

    // Must be a dir
    pub fn from_drive(drive: &str) -> FileSystemEntry {
        FileSystemEntry::new(&drive, std::path::Path::new(&drive), FileSystemEntryType::Directory, 0)
    }

    pub fn scan(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        let ticker_thread = std::thread::spawn(move||{
            loop {
                if rx.try_recv().is_ok() {
                    break;
                }
                print!(".");
                stdout().flush().expect("Failed to flush");
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        });
        print!("Starting scan...");

        self.calculate_children();

        tx.send("thread cancel").expect("Failed to send thread cancel");
        utils::log_s("...scan completed.");
        match ticker_thread.join() {
            Ok(_v) => {},
            Err(_e) => {
                utils::log_e("Failed to join");
            }
        }
    }

    pub fn size(&self) -> u64 {
        match self.entry_type {
            FileSystemEntryType::File => {
                self.len
            },
            FileSystemEntryType::Directory => {
                let mut sum : u64 = 0;
                for child in self.children().expect("directory has no children?") {
                    sum += child.size();
                }
                sum
            }
        }
    }

    pub fn children(&self) -> Option<&Vec::<FileSystemEntry>> {
        match self.entry_type {
            FileSystemEntryType::File => None,
            FileSystemEntryType::Directory => Some(&self.children)
        }
    }

    fn calculate_children(&mut self) {
        match self.entry_type {
            FileSystemEntryType::File => {

            },
            FileSystemEntryType::Directory => {

                let mut directory_items : Vec::<FileSystemEntry> = Vec::<FileSystemEntry>::new();

                let res : io::Result<fs::ReadDir> = fs::read_dir(&self.path_string);
                match res {
                    std::io::Result::Err(_e) => {
                        self.children = directory_items;
                    },
                    std::io::Result::Ok(entry) => {
                        for e in entry {
                            match e {
                                Err(_e) => {
                                    continue;
                                },
                                Ok(e) => {
                                    let entry : &fs::DirEntry = &e;
                                    let filename : String = String::from(entry.file_name().to_str().unwrap());
                                    let entry_descriptor : FileSystemEntryType;
                                    let mut size : u64 = 0;
                                    match entry.metadata() {
                                        Err(_e) => {
                                            utils::log_w("Failed to read metadata on file. Consider running as admin");
                                            entry_descriptor = FileSystemEntryType::File;
                                        },
                                        Ok(metadata) => {
                                            entry_descriptor = if metadata.is_dir() {FileSystemEntryType::Directory} else {FileSystemEntryType::File};
                                            size = metadata.len();
                                        }
                                    }
                                    if filename.starts_with('$') || filename.eq("System Volume Information") || filename.starts_with('.'){
                                        continue;
                                    }
                                    let mut new_entry = FileSystemEntry::new(&filename, &entry.path().as_path(), entry_descriptor, size);
                                    new_entry.calculate_children();
                                    // size() will iterate the children just aquired for file size
                                    new_entry.len = new_entry.size();
                                    directory_items.push(new_entry);
                                }
                                }
                            }
                        self.children = directory_items;
                    }
                }    
            }
        }
    }

    pub fn print(&self, visited_list: &Vec::<&String>) {
        let mut children_view = Vec::<FileSystemEntryChildrenView>::new();  
        for child in self.children().expect("I know you have a value") {
            let c1 = child.clone();
            children_view.push((c1.entry_type, c1.identifier, c1.len, c1.path_string));
        }

        if children_view.len() == 0 {
            utils::log("No directories");
        } else {
            utils::log("");
            utils::log(format!("\tDirectory: {}", self.path_string).as_str());
            utils::log("");
            children_view.sort_by_key(|k|k.2);
            for view_entry in children_view.iter() {
                if visited_list.contains(&&view_entry.3) {
                    utils::log_s(format!(" {:?}  {} ({})", view_entry.0, view_entry.1, view_entry.2.bytes_to_readable()).as_str());
                } else {
                    utils::log(format!(" {:?}  {} ({})", view_entry.0, view_entry.1, view_entry.2.bytes_to_readable()).as_str());
                }
            }
        }
    }
}

pub type FileSystemEntryChildrenView = (FileSystemEntryType, String, u64, String);

impl std::fmt::Display for FileSystemEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, " {:?}  {} ({})", self.entry_type, self.identifier, self.size().bytes_to_readable())
    }
}

impl Clone for FileSystemEntry {
    fn clone(&self) -> FileSystemEntry {
        FileSystemEntry {
            identifier: self.identifier.clone(),
            path_string: self.path_string.clone(),
            entry_type: self.entry_type.clone(),
            len: self.len,
            children: Vec::<FileSystemEntry>::new()
        }
    }
}