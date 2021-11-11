enum ByteUnits {
    #[allow(dead_code)]
    Bytes,
    Kilobytes,
    Megabytes,
    Gigabytes,
}

pub trait BytesExt {
    fn bytes_to_readable(self) -> String;
}

impl BytesExt for u64 {
    fn bytes_to_readable(self) -> String {
        bytes_to_readable(self)
    }
}

pub fn bytes_to_readable(num_of_bytes: u64) -> String {
    let mut readable: String = format!("{} Bytes", num_of_bytes.to_string()).to_string();

    let mut val = get(ByteUnits::Gigabytes, num_of_bytes);
    if val > 0 {
        readable = format!("{} GB", val).to_string();
        return readable
    }

    val = get(ByteUnits::Megabytes, num_of_bytes);
    if val > 0 {
        readable = format!("{} MB", val).to_string();
        return readable
    }

    val = get(ByteUnits::Kilobytes, num_of_bytes);
    if val > 0 {
        readable = format!("{} KB", val).to_string();
        return readable
    }

    readable
}

fn get(x: ByteUnits, num_of_bytes: u64) -> u64 {
    match x {
        ByteUnits::Gigabytes => divide(3, num_of_bytes),
        ByteUnits::Megabytes => divide(2, num_of_bytes),
        ByteUnits::Kilobytes => divide(1, num_of_bytes),
        ByteUnits::Bytes => divide(0, num_of_bytes),
    }
}

fn divide(multiplier: u32, num_of_bytes: u64) -> u64 {
    let res = u64::pow(1024, multiplier);
    num_of_bytes / res
}
