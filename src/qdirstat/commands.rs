pub enum Commands {
    ListDirectory,
    ChangeDirectory,
    Open,
    Scan,
    Help,
    Quit,
}

pub struct Command {
    pub command: Commands,
    pub args: Vec::<String>,
}

pub trait ToCommand {
    fn to_command(&self) -> Command;
}

impl ToCommand for String {
    fn to_command(&self) -> Command {
        let mut cmd = Command {
            command: Commands::Help,
            args: Vec::<String>::new(),
        };

        let mut string_cmd = str_ext::strip_trailing_newline(self.as_str()).to_string();
        let args : Vec::<String> = string_cmd.split(" ").map(|s| String::from(s)).collect();
        if args.len() > 0 {
            string_cmd = args[0].to_string();
            cmd.args = args[1..].to_vec();
        }
        
        if string_cmd.eq("q") || string_cmd.eq("quit") || string_cmd.eq("exit") {
            cmd.command = Commands::Quit;
        }    

        if string_cmd.eq("ls") || string_cmd.eq("list") || string_cmd.eq("print") {
            cmd.command =  Commands::ListDirectory;
        }

        if string_cmd.eq("cd") {
            cmd.command =  Commands::ChangeDirectory;

            // This is a little hacky too tired to do it right
            let mut sentence: String = String::new();
            for word in &args[1..] {
                if sentence != "".to_string() {
                    sentence.push(' ')
                }
                sentence.push_str(word)
            }
            cmd.args[0] = sentence;
        }

        if string_cmd.eq("h") || string_cmd.eq("help") || string_cmd.eq("?") {
            cmd.command =  Commands::Help;
        }

        if string_cmd.eq("scan") {
            cmd.command =  Commands::Scan;
        }

        if string_cmd.eq("open") || string_cmd.eq("start") {
            cmd.command =  Commands::Open;
        }

        cmd
    }
}

mod str_ext {
    pub fn strip_trailing_newline(input: &str) -> &str {
        return input.strip_suffix("\r\n")
            .or(input.strip_suffix("\n"))
            .unwrap_or(&input);
    }
}
