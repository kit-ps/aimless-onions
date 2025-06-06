use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use color_eyre::Result;
use regex::Regex;

pub struct Relay {
    pub weight: u32,
}

pub fn read<P: AsRef<Path>>(path: P) -> Result<Vec<Relay>> {
    let mut result = Vec::new();

    let re = Regex::new("Bandwidth=(\\d+)").unwrap();

    let mut file = BufReader::new(File::open(path)?);
    let mut line = String::new();

    while let Ok(bytes_read) = file.read_line(&mut line) {
        if bytes_read == 0 {
            break;
        }

        if let Some(capture) = re.captures(&line) {
            let weight = capture[1].parse().unwrap();
            result.push(Relay { weight });
        }

        line.clear();
    }

    Ok(result)
}
