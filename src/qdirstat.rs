mod filesystem_entry;
mod commands;

use filesystem_entry::{
    FileSystemEntry,
    filesystem_entry_type::FileSystemEntryType,
};

use commands::*;

use std::io::*;
use dialoguer::{
    Select,
    theme::ColorfulTheme
};
use console::Term;

#[cfg(target_os = "windows")]
const FILE_MANAGER: &str = "explorer";

#[cfg(target_os = "macos")]
const FILE_MANAGER: &str = "open";

#[cfg(target_os = "linux")]
const FILE_MANAGER: &str = "xdg-open";

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn open_directory(fse: &FileSystemEntry) {
    let path = fse.path_string.to_string();
    match std::process::Command::new(FILE_MANAGER).arg(path).spawn() {
        Ok(_child) => {},
        Err(e) => utils::log_w(format!("Failed to open the directory: {}", e).as_str()),
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn open_directory(_fse: &FileSystemEntry) {
    utils::log_w("Opening a directory is not supported on this platform.");
}

/// This method will return a vector of all drives which exist on the windows filesystem
/// Currently will not return drives represented by two characters (e.g. drive AA:)
fn list_of_available_drives() -> Vec<String> {
    let mut list = Vec::<String>::new();

    // Keep in mind that some drives have double chars e.g: this is not covered yet AA:
    for c in (b'A' ..= b'Z').map(char::from) {
        let drive = format!("{}:", c.to_string());
        let path = std::path::Path::new(&drive);
        if path.is_dir() {
            list.push(drive)
        }
    }

    list
}

fn select_drive() -> std::io::Result<String> {
    let items : Vec<String> = list_of_available_drives();
    let selection = Select::with_theme(&ColorfulTheme::default())
    .items(&items)
    .default(0)
    .interact_on_opt(&Term::stderr())?;

    match selection {
        Some(index) => {
            Ok(items[index].to_string())
        },
        // Backing out of the drive picker is a normal way to leave, not a crash.
        None => {
            println!("\n No drive selected. Session terminated.");
            std::process::exit(0);
        }
    }
}

fn get_root_drive() -> String {
    if cfg!(windows) {
        format!("{}\\", select_drive().unwrap()) 
    } else if cfg!(unix) {
        String::from("/")
    } else {
        panic!("Unknown OS.")
    }
}

// Takes a partial string and attempts to match it across possible_items returning the first match
fn tab_complete(current_str: &str, possible_items: Vec::<String>) -> String {
    let mut result = String::new();
    for item in possible_items {
        if item.to_ascii_lowercase() == current_str.to_ascii_lowercase() {
            continue;
        }

        if item.to_ascii_lowercase().starts_with(current_str.to_ascii_lowercase().as_str()) {
            result = item;
            break;
        }
    }

    if result.is_empty() {
        result = current_str.to_string();
    }

    result
}

fn tab(cmd: &mut String, current: &FileSystemEntry) {
    match current.children() {
        Some(children) => {
            match cmd.to_command().command {
                Commands::ChangeDirectory => {
                    if cmd.len() < 4 {
                        return;
                    }

                    // Need to save a pre-tab-completed str for skipping and advancing the tab complete with other options
                    // at this point we've asserted its a cd command
                    let tab_completed = tab_complete(&cmd[3..], children.into_iter()
                    .map(|item: &FileSystemEntry| item.identifier.to_string()).collect());
                    *cmd = format!("cd {}", tab_completed).to_string();
                },
                _ => return // ignore the tab
            }    
        },
        None => {}
    }
}

// Replaces whatever has been typed with a quit command so the caller's normal Quit
// handling takes over. Used for Ctrl+C/Ctrl+D and for an unreadable terminal.
fn quit(cmd: &mut String) {
    cmd.clear();
    cmd.push_str("quit");
}

fn get_next_command(cmd: &mut String, _current: &FileSystemEntry) {

    let mut term = Term::stdout();

    // Without a terminal, read_key() returns Ok(Key::Unknown) immediately and forever,
    // which would spin this loop at 100% CPU. There is no interactive session to have.
    if !term.is_term() {
        utils::log_e("QDirStat needs an interactive terminal.");
        quit(cmd);
        return;
    }

    loop {
        match term.read_key() {
            Ok(key) => {
                match key {
                    console::Key::Backspace => {
                        if cmd.is_empty() {
                            continue;
                        }
                        // pop() is character aware; remove(len-1) splits multi-byte characters.
                        cmd.pop();
                        term.clear_line().expect("failed to clear terminal");
                        term.write_all(cmd.as_bytes()).expect("failed to write to terminal");
                        continue;
                    },
                    console::Key::Tab => {
                        tab(cmd, _current);
                        term.clear_line().expect("failed to clear terminal");
                        term.write_all(cmd.as_bytes()).expect("failed to write to terminal");
                    },
                    console::Key::Enter => {
                        term.write_line("").expect("failed to write to terminal");
                        return;
                    }
                    console::Key::Char(c) => {
                        match c {
                            '\t' => {
                                tab(cmd, _current);
                                term.clear_line().expect("failed to clear terminal");
                                term.write_all(cmd.as_bytes()).expect("failed to write to terminal");
                                continue;
                            },
                            // Ctrl+C and Ctrl+D
                            '\u{3}' | '\u{4}' => {
                                term.write_line("").expect("failed to write to terminal");
                                quit(cmd);
                                return;
                            },
                            _ => {
                                cmd.push(c);
                                // A char is up to 4 bytes in UTF-8, and encode_utf8 hands back a
                                // str of exactly the right length, so nothing extra is echoed.
                                let mut buffer = [0; 4];
                                term.write_all(c.encode_utf8(&mut buffer).as_bytes()).expect("failed to write to terminal");
                                continue;
                            }
                        }

                    },
                    _ => { continue; }
                }
            },
            Err(e) => {
                // A closed or non-interactive stdin fails every read, so retrying here spins forever.
                utils::log_e(format!("Failed to read from the terminal: {:?}", e).as_str());
                quit(cmd);
                return;
            },
        }
    }
}

fn icmp(a: &String, b: &String) -> bool {
    return a.to_ascii_lowercase() == b.to_ascii_lowercase();
}

/// Why a session stopped: the user left the program, or asked to go back and pick
/// another volume.
enum SessionEnd {
    Quit,
    Reset,
}

#[allow(dead_code)]
pub fn run() {
    utils::log_i("QDirStat Terminal");

    // One session per chosen volume. The scanned tree is owned by `session`, so returning
    // from it drops the tree along with every borrow into it -- which is what makes
    // selecting a different volume possible without threading lifetimes through the loop.
    loop {
        match session() {
            SessionEnd::Quit => {
                println!("\n Session terminated.");
                return;
            },
            SessionEnd::Reset => {
                utils::log_i("Returning to volume selection...");
            },
        }
    }
}

fn session() -> SessionEnd {
    let mut zipper = Vec::<&FileSystemEntry>::new();
    let mut visited_entries = Vec::<&String>::new();
    let mut root : FileSystemEntry = FileSystemEntry::from_drive(get_root_drive().as_str());

   
    root.scan();
    
    
    root.print(&visited_entries);
    println!("");

    let mut current : &FileSystemEntry = &root;

    let mut command_string: String = String::new();

    loop {

        get_next_command(&mut command_string, current);

        let cmd : Command = command_string.to_command();
        // Cleared here rather than at the end of the loop so that no early `continue`
        // can leave the next command appended to this one.
        command_string.clear();

        match cmd.command {
            Commands::Help => {
                utils::log_i("QDirStat commands");
                utils::log("\t ls: List current directory");
                utils::log("\t cd: Change current directory. (e.g. cd .. or cd Program Files)");
                utils::log("\t scan: Recursive scan from current directory downward [Not Implemented]");
                utils::log("\t open: Opens current directory in the file explorer");
                utils::log("\t reset: Discard this scan and choose a volume again");
                utils::log("\t delete: ?? Crazy of you to think I'd take such responsibility. Open the folder and do it yourself!");
                utils::log("\t quit: Quit program");
            },
            Commands::Quit => {
                return SessionEnd::Quit;
            }
            Commands::Reset => {
                return SessionEnd::Reset;
            }
            Commands::Open => {
                open_directory(current);
            },
            Commands::ChangeDirectory => {
                if cmd.args.len() < 1 {
                    utils::log_w("Change directory command requires an additional argument.");
                    continue;
                }

                let target : String = cmd.args[0].to_ascii_lowercase();
                match target.as_str() {
                    ".." => {
                        match zipper.pop() {
                            None => {
                                utils::log_w("No parent directory exists");
                            },
                            Some(entry) => {
                                current = entry;
                                current.print(&visited_entries);
                            }
                        }
                    },
                    _ => {

                        if current.children().is_none() {
                            utils::log_w(format!("No entry matches target '{}'", target).as_str());
                            continue;
                        }

                        let children = current.children().expect("No children");
                        match children.iter().position(|c| icmp(&c.identifier, &target)) {
                            None => {
                                utils::log_w(format!("No entry matches target '{}'", target).as_str());
                                continue;
                            },
                            Some(idx) => { 
                                let matching_entry = &children[idx];
                                match matching_entry.entry_type {
                                    FileSystemEntryType::File => {
                                        utils::log_w("Change directory target is a file.");
                                    }
                                    FileSystemEntryType::Directory => {
                                        if !visited_entries.contains(&&matching_entry.path_string) {
                                            visited_entries.push(&&matching_entry.path_string);
                                        }
                                        zipper.push(&current);
                                        current = matching_entry;
                                        current.print(&visited_entries);
                                        println!("");
                                    }
                                }                    
                            },
                        }
                    }
                }
            },
            Commands::ListDirectory => {
                println!("Path: {}", current.path_string);
                current.print(&visited_entries);
                println!("");
            },
            Commands::Scan => {
                utils::log_w("Recursive scan from the current directory is not implemented yet.");
            },
        }
    }
   
}