pub mod filesystem_entry_type;
pub mod filesystem_entry_extensions;
pub mod scan_progress;
pub mod volume;

use filesystem_entry_type::FileSystemEntryType;
use filesystem_entry_extensions::*;
use scan_progress::ScanProgress;

use std::fs;
use std::sync::Arc;

/// What an entry actually occupies on disk, which is the question this tool is asked.
///
/// Not `metadata.len()`: that is the apparent size, and on a pseudo-filesystem it is
/// fiction. /proc/kcore is a window onto the kernel's address space and reports 128 TiB
/// while occupying nothing, which alone was 99.98% of a scan of /. Every sysfs attribute
/// likewise claims 4096 bytes and occupies none. Blocks also make sparse files honest.
#[cfg(unix)]
fn allocated_size(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    // st_blocks is in 512-byte units by POSIX, whatever the filesystem's own block size.
    metadata.blocks().saturating_mul(512)
}

/// Windows has no allocated-size equivalent in std, so the apparent size stands.
#[cfg(not(unix))]
fn allocated_size(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}

/// Whether an entry is skipped outright, before it is even stat'd.
///
/// Nothing is, on unix. A leading dot means "hidden", not "not worth counting": this
/// machine keeps ~5.5 GB in .local, .rustup, .cache and .config alone, and hiding it
/// from a tool whose whole job is to find what fills a disk defeats the tool.
#[cfg(unix)]
fn is_excluded(_filename: &str) -> bool {
    false
}

/// Windows keeps volume bookkeeping in the root of every drive -- $MFT, $Recycle.Bin,
/// System Volume Information. It is not the user's data and is mostly unreadable anyway.
#[cfg(not(unix))]
fn is_excluded(filename: &str) -> bool {
    filename.starts_with('$')
        || filename.eq("System Volume Information")
        || filename.starts_with('.')
}

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
        utils::log_i("Starting scan...");

        // Scanning always starts at a drive root, so the volume holding it is the whole
        // of what there is to scan. None off Windows, which just drops the percentage.
        let volume_usage = volume::usage(&self.path_string);

        let progress = Arc::new(ScanProgress::new(volume_usage));
        let rendered = Arc::clone(&progress);
        let ticker_thread = std::thread::spawn(move || scan_progress::render_loop(&rendered));

        let started = std::time::Instant::now();
        self.calculate_children(&progress);
        let elapsed = started.elapsed();

        // Stop the renderer and wait for it before writing anything else, otherwise the
        // status line lands on top of the summary.
        progress.finish();
        if ticker_thread.join().is_err() {
            utils::log_e("Failed to join the progress thread");
        }

        utils::log_s(progress.summary(elapsed).as_str());

        // How full the disk is, which is what the eye is looking for here. Kept separate
        // from scan coverage below: they are different numbers and were once conflated.
        if let Some(line) = progress.volume_line() {
            utils::log(line.as_str());
        }

        if let Some(warning) = progress.coverage_warning() {
            utils::log_w(warning.as_str());
        }

        // Reported once here rather than per file: on a system drive this warning used to
        // fire thousands of times, straight through the progress line.
        if let Some(warning) = progress.skipped_warning() {
            utils::log_w(warning.as_str());
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

    fn calculate_children(&mut self, progress: &ScanProgress) {
        match self.entry_type {
            FileSystemEntryType::File => {

            },
            FileSystemEntryType::Directory => {

                progress.enter_directory(&self.path_string);

                let mut directory_items : Vec::<FileSystemEntry> = Vec::<FileSystemEntry>::new();

                match fs::read_dir(&self.path_string) {
                    Err(_e) => {
                        // Unreadable directory: it contributes nothing rather than aborting the scan.
                        progress.record_skipped();
                    },
                    Ok(entry) => {
                        for e in entry {
                            match e {
                                Err(_e) => {
                                    progress.record_skipped();
                                    continue;
                                },
                                Ok(e) => {
                                    let entry : &fs::DirEntry = &e;
                                    let filename : String = String::from(entry.file_name().to_str().unwrap());

                                    // Filtered before the metadata call: no reason to pay for a
                                    // stat on an entry that is about to be dropped.
                                    if is_excluded(&filename) {
                                        continue;
                                    }

                                    let entry_descriptor : FileSystemEntryType;
                                    let mut size : u64 = 0;
                                    match entry.metadata() {
                                        Err(_e) => {
                                            progress.record_skipped();
                                            entry_descriptor = FileSystemEntryType::File;
                                        },
                                        Ok(metadata) => {
                                            entry_descriptor = if metadata.is_dir() {FileSystemEntryType::Directory} else {FileSystemEntryType::File};
                                            size = allocated_size(&metadata);
                                            if metadata.is_dir() {
                                                progress.record_directory();
                                            } else {
                                                progress.record_file(size);
                                            }
                                        }
                                    }
                                    let descended = entry_descriptor == FileSystemEntryType::Directory;

                                    let mut new_entry = FileSystemEntry::new(&filename, &entry.path().as_path(), entry_descriptor, size);
                                    new_entry.calculate_children(progress);
                                    directory_items.push(new_entry);

                                    if descended {
                                        // Only a directory child overwrites the status path, and
                                        // files outnumber directories heavily -- no reason to take
                                        // the lock for every one of them.
                                        progress.enter_directory(&self.path_string);
                                    }
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

        // Smallest last: the biggest entries end up nearest the prompt, where the eye already is.
        let mut sorted = children.iter().collect::<Vec::<&FileSystemEntry>>();
        sorted.sort_by_key(|entry| entry.len);

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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A file with a hole: `set_len` grows the apparent size without allocating anything.
    fn sparse_file(name: &str, apparent: u64) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("qdirstat-{}-{}", name, std::process::id()));
        let file = fs::File::create(&path).expect("create the test file");
        file.set_len(apparent).expect("grow the test file");
        path
    }

    #[test]
    fn a_sparse_file_costs_what_it_occupies_not_what_it_claims() {
        let path = sparse_file("sparse", 8 * 1024 * 1024 * 1024);
        let metadata = fs::metadata(&path).expect("stat the test file");

        assert_eq!(metadata.len(), 8 * 1024 * 1024 * 1024, "8 GB of apparent size");
        assert!(
            allocated_size(&metadata) < 1024 * 1024,
            "a hole occupies nothing, got {} bytes",
            allocated_size(&metadata)
        );

        fs::remove_file(&path).ok();
    }

    #[test]
    fn a_real_file_still_counts() {
        let path = sparse_file("solid", 0);
        fs::write(&path, vec![7u8; 64 * 1024]).expect("write the test file");
        let metadata = fs::metadata(&path).expect("stat the test file");

        let allocated = allocated_size(&metadata);
        assert!(allocated > 0, "written bytes occupy disk");
        // Compression can store 64 KB in less, so this only bounds the waste above.
        assert!(allocated <= 2 * metadata.len(), "no wild over-count, got {}", allocated);

        fs::remove_file(&path).ok();
    }

    /// The bug this whole helper exists for: /proc/kcore reports the size of the kernel's
    /// address space, which was 99.98% of a scan of /.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_pseudo_file_costs_nothing() {
        let metadata = match fs::metadata("/proc/kcore") {
            Ok(metadata) => metadata,
            // Not every kernel exposes it; nothing to assert if it is absent.
            Err(_) => return,
        };

        assert!(metadata.len() > 1024 * 1024 * 1024 * 1024, "kcore claims terabytes");
        assert_eq!(allocated_size(&metadata), 0, "and occupies none of them");
    }

    #[test]
    fn hidden_entries_are_still_counted_on_unix() {
        // The dotfile skip was Windows bookkeeping that also swallowed ~5.5 GB of real
        // data here: ~/.local, ~/.rustup, ~/.cache, ~/.config and /etc/skel.
        assert!(!is_excluded(".cache"), "a dotdir is data like any other");
        assert!(!is_excluded(".bashrc"));
        assert!(!is_excluded("$Recycle.Bin"), "not a special name on unix");
        assert!(!is_excluded("Documents"));
    }
}
