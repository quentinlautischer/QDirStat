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

#[cfg(target_os = "linux")]
const OPEN_HELP: &str = "\t open: Opens a terminal in the current directory";

#[cfg(not(target_os = "linux"))]
const OPEN_HELP: &str = "\t open: Opens current directory in the file explorer";

#[cfg(target_os = "windows")]
const FILE_MANAGER: &str = "explorer";

#[cfg(target_os = "macos")]
const FILE_MANAGER: &str = "open";

/// Wait on a spawned child in the background. Nothing here cares what it exits with,
/// but something has to collect it or it stays a zombie for the life of the session.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn reap(mut child: std::process::Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

/// Off Linux, "open" still means the desktop's file manager.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn open_directory(fse: &FileSystemEntry) {
    let path = fse.path_string.to_string();
    match std::process::Command::new(FILE_MANAGER).arg(path).spawn() {
        Ok(child) => reap(child),
        Err(e) => utils::log_w(format!("Failed to open the directory: {}", e).as_str()),
    }
}

// Detach a child into its own session.
//
// Without this the terminal we open stays in QDirStat's process group and dies of the
// HUP that arrives when QDirStat exits -- observed as foot reporting "slave exited
// with signal 1 (Hangup)" the instant the window appeared.
//
// Declared by hand rather than pulled from a crate, as the Windows call in
// volume::usage is. That one came with a warning about hand-declaring anything with a
// struct in it; setsid takes no arguments and returns a pid_t, which is i32 on every
// Linux target, so there is no layout to get wrong.
#[cfg(target_os = "linux")]
extern "C" {
    fn setsid() -> i32;
}

/// Arguments that start `terminal` in `path`.
///
/// Empty for most of them: a terminal inherits the working directory of whatever
/// spawned it, and `current_dir` below sets it. The exceptions hand the request to an
/// already-running server process that does not share our directory, so they have to
/// be told in as many words.
#[cfg(target_os = "linux")]
fn working_directory_arguments(terminal: &str, path: &str) -> Vec<String> {
    match terminal {
        "gnome-terminal" | "xfce4-terminal" | "mate-terminal" | "tilix" => {
            vec![format!("--working-directory={}", path)]
        }
        "konsole" => vec!["--workdir".to_string(), path.to_string()],
        "terminator" => vec!["--working-directory".to_string(), path.to_string()],
        "wezterm" => vec!["start".to_string(), "--cwd".to_string(), path.to_string()],
        _ => Vec::new(),
    }
}

/// Open a terminal in the scanned directory, which is what someone standing in a
/// directory listing usually wants next -- to go and deal with what they just found.
#[cfg(target_os = "linux")]
fn open_directory(fse: &FileSystemEntry) {
    use std::os::unix::process::CommandExt;

    let path = fse.path_string.as_str();

    // Checked up front because a missing directory would otherwise fail every spawn
    // below with NotFound, which reads identically to "that terminal is not installed"
    // and would end in a wrong diagnosis.
    if !std::path::Path::new(path).is_dir() {
        utils::log_w(format!("No longer a directory: {}", path).as_str());
        return;
    }

    // Only what the desktop actually says. $TERMINAL is the long-standing convention,
    // and xdg-terminal-exec is the freedesktop utility that resolves the user's chosen
    // terminal the way xdg-open resolves a file handler. Guessing past those would mean
    // shipping a list of emulators that goes stale and can still open the wrong one.
    let preferred = std::env::var("TERMINAL").unwrap_or_default();
    let candidates = std::iter::once(preferred.as_str())
        .filter(|terminal| !terminal.is_empty())
        .chain(std::iter::once("xdg-terminal-exec"));

    for terminal in candidates {
        // The file name, so a $TERMINAL given as an absolute path still matches above.
        let name = std::path::Path::new(terminal)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(terminal);

        let mut command = std::process::Command::new(terminal);
        command
            .args(working_directory_arguments(name, path))
            .current_dir(path)
            // A new terminal must not write into the one we are drawing in.
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        // SAFETY: pre_exec runs between fork and exec, where only async-signal-safe
        // calls are allowed. setsid is a bare syscall and takes no arguments, so it
        // qualifies. It fails only when the caller already leads a process group,
        // which a freshly forked child does not, and a failure here is survivable.
        unsafe {
            command.pre_exec(|| {
                setsid();
                Ok(())
            });
        }

        let spawned = command.spawn();

        match spawned {
            Ok(child) => {
                reap(child);
                return;
            }
            // Not installed, which is the expected answer for most of the list.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                utils::log_w(format!("Failed to open {}: {}", name, e).as_str());
                return;
            }
        }
    }

    utils::log_w("No terminal to open. Set $TERMINAL, or install xdg-terminal-exec.");
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
                utils::log(OPEN_HELP);
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
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn most_terminals_inherit_the_directory_rather_than_being_told() {
        // These take the working directory from the process that spawned them, so
        // passing arguments would only be a way to get the quoting wrong.
        for terminal in ["ghostty", "alacritty", "kitty", "foot", "xterm", "urxvt"] {
            assert!(
                working_directory_arguments(terminal, "/tmp/x").is_empty(),
                "{} inherits the directory",
                terminal
            );
        }
    }

    #[test]
    fn server_backed_terminals_are_told_the_directory() {
        // A running server does not share our working directory, so these have to be
        // handed the path, each in its own spelling.
        assert_eq!(
            working_directory_arguments("gnome-terminal", "/tmp/x"),
            vec!["--working-directory=/tmp/x"]
        );
        assert_eq!(
            working_directory_arguments("konsole", "/tmp/x"),
            vec!["--workdir", "/tmp/x"]
        );
        assert_eq!(
            working_directory_arguments("wezterm", "/tmp/x"),
            vec!["start", "--cwd", "/tmp/x"]
        );
    }

    #[test]
    fn a_path_in_the_directory_is_carried_through_verbatim() {
        // A directory with a space in it must not arrive as two arguments.
        let arguments = working_directory_arguments("konsole", "/tmp/two words");
        assert_eq!(arguments, vec!["--workdir", "/tmp/two words"]);
    }
}
