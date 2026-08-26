#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FileSystemEntryType {
    Directory,
    File,
}

impl std::fmt::Debug for FileSystemEntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileSystemEntryType::Directory=> {
                let mut s : String = String::from("");
                s.push('🗀');
                write!(f, "{}", s)
            },
            FileSystemEntryType::File => {
                let mut s : String = String::from("");
                s.push('🗋');
                write!(f, "{}", s)
            },
        }
    }
}
