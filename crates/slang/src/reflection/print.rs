pub struct PrintObject {
    newline: bool,
    depth: usize,
}

impl PrintObject {
    pub fn new(depth: usize) -> Self {
        Self {
            newline: true,
            depth,
        }
    }

    pub fn value(&mut self, name: &str, value: &str) {
        if self.newline {
            println!("{}{}: {}", " ".repeat(self.depth), name, value);
        } else {
            println!("{}: {}", name, value);
        }
        self.newline = true;
    }

    pub fn object(&self, name: &str) -> PrintObject {
        println!("{}{}:", " ".repeat(self.depth), name);
        PrintObject {
            newline: true,
            depth: self.depth + 2,
        }
    }

    pub fn array(&self, name: &str) -> PrintArray {
        println!("{}{}:", " ".repeat(self.depth), name);
        PrintArray {
            newline: true,
            depth: self.depth + 2,
        }
    }
}

pub struct PrintArray {
    newline: bool,
    depth: usize,
}

impl PrintArray {
    pub fn new(depth: usize) -> Self {
        Self {
            newline: true,
            depth,
        }
    }

    pub fn value(&mut self, value: &str) {
        if self.newline {
            println!("{}- {}", " ".repeat(self.depth), value);
        } else {
            println!("- {}", value);
        }
        self.newline = true;
    }

    pub fn object(&self) -> PrintObject {
        print!("{}- ", " ".repeat(self.depth));
        PrintObject {
            newline: false,
            depth: self.depth + 2,
        }
    }

    pub fn array(&self) -> PrintArray {
        print!("{}- ", " ".repeat(self.depth));
        PrintArray {
            newline: false,
            depth: self.depth + 2,
        }
    }
}
