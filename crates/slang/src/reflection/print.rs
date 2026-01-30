use std::io::Write;
use std::sync::{Arc, RwLock};

pub struct PrintObject {
    newline: bool,
    depth: usize,
    buffer: Arc<RwLock<Vec<u8>>>,
}

impl PrintObject {
    pub fn new(depth: usize) -> Self {
        Self {
            newline: true,
            depth,
            buffer: Arc::new(RwLock::new(vec![])),
        }
    }

    pub fn read_buffer(&self) -> String {
        let buffer = self.buffer.read().unwrap();
        String::from_utf8(buffer.clone()).unwrap()
    }

    fn write_buffer(&self, value: &str) {
        print!("{}", value);
        let mut buffer = self.buffer.write().unwrap();
        write!(buffer, "{}", value).unwrap();
    }

    pub fn value(&mut self, name: &str, value: &str) {
        if self.newline {
            self.write_buffer(&format!("{}{}: {}\n", " ".repeat(self.depth), name, value));
        } else {
            self.write_buffer(&format!("{}: {}\n", name, value));
        }
        self.newline = true;
    }

    pub fn object(&self, name: &str) -> PrintObject {
        self.write_buffer(&format!("{}{}:\n", " ".repeat(self.depth), name));
        PrintObject {
            newline: true,
            depth: self.depth + 2,
            buffer: self.buffer.clone(),
        }
    }

    pub fn array(&self, name: &str) -> PrintArray {
        self.write_buffer(&format!("{}{}:\n", " ".repeat(self.depth), name));
        PrintArray {
            newline: true,
            depth: self.depth + 2,
            buffer: self.buffer.clone(),
        }
    }
}

pub struct PrintArray {
    newline: bool,
    depth: usize,
    buffer: Arc<RwLock<Vec<u8>>>,
}

impl PrintArray {
    fn write_buffer(&self, value: &str) {
        print!("{}", value);
        let mut buffer = self.buffer.write().unwrap();
        write!(buffer, "{}", value).unwrap();
    }

    pub fn value(&mut self, value: &str) {
        if self.newline {
            self.write_buffer(&format!("{}- {}\n", " ".repeat(self.depth), value));
        } else {
            self.write_buffer(&format!("- {}\n", value));
        }
        self.newline = true;
    }

    pub fn object(&self) -> PrintObject {
        self.write_buffer(&format!("{}- ", " ".repeat(self.depth)));
        PrintObject {
            newline: false,
            depth: self.depth + 2,
            buffer: self.buffer.clone(),
        }
    }

    pub fn array(&self) -> PrintArray {
        self.write_buffer(&format!("{}- ", " ".repeat(self.depth)));
        PrintArray {
            newline: false,
            depth: self.depth + 2,
            buffer: self.buffer.clone(),
        }
    }
}
