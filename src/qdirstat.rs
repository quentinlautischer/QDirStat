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

fn open_directory(fse: &FileSystemEntry) {
    let cmd_string;
    if cfg!(windows) {
        cmd_string = "explorer";
    } else if cfg!(osx) {
        cmd_string = "open";
    } else if cfg!(unix) {
        cmd_string = "xdg-open";
    } else {
        utils::log_e(format!("Could not match current OS with an open command. Here is the current path: {}", fse.path_string).as_str());
        return;
    }

    let path = fse.path_string.to_string();
    std::process::Command::new(cmd_string).arg(path).spawn().unwrap( );
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
    #[cfg(debug_assertions)]
    return Ok(String::from("C:"));

    #[allow(unreachable_code)]
    {
        let items : Vec<String> = list_of_available_drives();
        let selection = Select::with_theme(&ColorfulTheme::default())
        .items(&items)
        .default(0)
        .interact_on_opt(&Term::stderr())?;

        match selection {
            Some(index) => {
                Ok(items[index].to_string())
            },
            None => panic!("No drive selected")
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

fn get_next_command(cmd: &mut String, _current: &FileSystemEntry) {

    // the console crate doesn't work when debugging. So I've added this
    // #[cfg(debug_assertions)] / #[allow(unreachable_code)] to have diff logic
    // for release vs. debug

    #[cfg(debug_assertions)]
    {
        match std::io::stdin().read_line(cmd) {
            Ok(_bytes_read) => { },
            Err(_) => panic!("Failed to readline")
        }
        return;
    }

    #[allow(unreachable_code)]
    {
        let mut term = Term::stdout();
        loop {
            match term.read_key() {
                Ok(key) => {
                    match key {
                        console::Key::Backspace => {
                            if cmd.is_empty() {
                                continue;
                            }
                            cmd.remove(cmd.len()-1);
                            term.clear_line().expect("failed to clear terminal");
                            term.write(cmd.as_bytes()).expect("failed to write to terminal");
                            continue;
                        },
                        console::Key::Tab => {
                            tab(cmd, _current);
                            term.clear_line().expect("failed to clear terminal");
                            term.write(cmd.as_bytes()).expect("failed to write to terminal");
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
                                    term.write(cmd.as_bytes()).expect("failed to write to terminal");
                                    continue;
                                },
                                _ => {
                                    cmd.push(c);
                                    let mut b = [0; 2];
                                    c.encode_utf8(&mut b);
                                    term.write(&b).expect("failed to write to terminal");
                                    continue;
                                }
                            }
                            
                        },
                        _ => { continue; }
                    }
                },
                Err(e) => {
                    println!("{:?}", e);
                },
            }
        }
    }
}

fn icmp(a: &String, b: &String) -> bool {
    return a.to_ascii_lowercase() == b.to_ascii_lowercase();
}

#[allow(dead_code)]
pub fn run() {
    utils::log_i("QDirStat Terminal");

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

        match cmd.command {
            Commands::Help => {
                utils::log_i("QDirStat commands");
                utils::log("\t ls: List current directory");
                utils::log("\t cd: Change current directory. (e.g. cd .. or cd Program Files)");
                utils::log("\t scan: Recursive scan from current directory downward [Not Implemented]");
                utils::log("\t open: Opens current directory in the file explorer");
                utils::log("\t quit: Quit program");
            },
            Commands::Quit => {
                println!("\n Session terminated.");
                return;
            }
            Commands::Open => {
                open_directory(current);
            },
            Commands::ChangeDirectory => {
                if cmd.args.len() < 1 {
                    utils::log_w("Change directory command requires an additional argument.");
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
                            return;
                        }
                        
                        let children = current.children().expect("No children");
                        match children.iter().position(|c| icmp(&c.identifier, &target)) {
                            None => {
                                utils::log_w(format!("No entry matches target '{}'", target).as_str());
                                return; 
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
                // root.scan();
            },
        }

        command_string.clear();
    }
   
}