pub enum FileSystemEntryType {
    #[allow(dead_code)]
    Directory,
    #[allow(dead_code)]
    File,
}

impl Clone for FileSystemEntryType {
    fn clone(&self) -> FileSystemEntryType {
        match self {
            FileSystemEntryType::Directory => FileSystemEntryType::Directory, 
            FileSystemEntryType::File => FileSystemEntryType::File,
        }
    }
}

impl PartialEq for FileSystemEntryType {
    fn eq(&self, other: &Self) -> bool {
        self == other
    }
}


impl std::fmt::Debug for FileSystemEntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileSystemEntryType::Directory=> {
                let mut s : String = String::from("");
                // s.push(' ');
                s.push('🗀');
                write!(f, "{}", s)
            },
            FileSystemEntryType::File => {
                let mut s : String = String::from("");
                // s.push(' ');
                s.push('🗋');
                write!(f, "{}", s)
            },
        }
    }
}