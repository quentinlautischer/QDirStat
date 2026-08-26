pub mod filesystem_entry_type;
pub mod filesystem_entry_extensions;

use filesystem_entry_type::FileSystemEntryType;
use filesystem_entry_extensions::*;

use std::fs;

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

    /// The size of this entry's subtree. `calculate_children` rolls the totals up as it
    /// scans, so this is already accurate for directories.
    pub fn size(&self) -> u64 {
        self.len
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

                match fs::read_dir(&self.path_string) {
                    Err(_e) => {
                        // Unreadable directory: it contributes nothing rather than aborting the scan.
                    },
                    Ok(entry) => {
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
                                    directory_items.push(new_entry);
                                }
                            }
                        }
                    }
                }

                self.children = directory_items;
                // Every child already carries its own subtree total, so this roll-up is one level deep.
                self.len = self.children.iter().map(|c| c.len).sum();
            }
        }
    }

    pub fn print(&self, visited_list: &Vec::<&String>) {
        let children = match self.children() {
            Some(children) => children,
            None => {
                utils::log_w("Not a directory");
                return;
            }
        };

        if children.is_empty() {
            utils::log("No directories");
            return;
        }

        // Largest first: the whole point is spotting what is eating the drive.
        let mut sorted = children.iter().collect::<Vec::<&FileSystemEntry>>();
        sorted.sort_by_key(|entry| std::cmp::Reverse(entry.len));

        utils::log("");
        utils::log(format!("\tDirectory: {}", self.path_string).as_str());
        utils::log("");
        for entry in sorted {
            let line = format!(" {:?}  {} ({})", entry.entry_type, entry.identifier, entry.len.bytes_to_readable());
            if visited_list.contains(&&entry.path_string) {
                utils::log_s(line.as_str());
            } else {
                utils::log(line.as_str());
            }
        }
    }
}

impl std::fmt::Display for FileSystemEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, " {:?}  {} ({})", self.entry_type, self.identifier, self.size().bytes_to_readable())
    }
}
