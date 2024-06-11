pub struct Bar {
    left_pad: String,
    separator: String,
    right_pad: String,
    clear_char: char,
    expire_char: char,

    slots: Vec<String>,
}

impl Bar {
    pub fn new(
        n: usize,
        left_pad: &str,
        separator: &str,
        right_pad: &str,
        clear_char: char,
        expire_char: char,
    ) -> Self {
        let mut slots = Vec::with_capacity(n);
        for _ in 0..n {
            slots.push(String::new());
        }
        Self {
            left_pad: left_pad.to_string(),
            separator: separator.to_string(),
            right_pad: right_pad.to_string(),
            clear_char,
            expire_char,
            slots,
        }
    }

    pub fn set(&mut self, i: usize, data: &str) {
        self.slots[i] = data.to_string();
    }

    pub fn clear(&mut self, i: usize) {
        self.overwrite(i, self.clear_char)
    }

    pub fn expire(&mut self, i: usize) {
        self.overwrite(i, self.expire_char)
    }

    fn overwrite(&mut self, i: usize, c: char) {
        let new: String = (0..self.slots[i].len()).map(|_| c).collect();
        self.set(i, &new);
    }

    pub fn show(&self) -> String {
        [
            self.left_pad.to_string(),
            self.slots.join(&self.separator),
            self.right_pad.to_string(),
        ]
        .join("")
    }
}

#[cfg(test)]
mod tests {
    use super::Bar;

    #[test]
    fn basic() {
        let mut b = Bar::new(3, "[", "|", "]", ' ', '_');
        assert_eq!(["", "", ""], b.slots.as_slice());
        assert_eq!("[||]", b.show());

        b.set(1, "abc");
        assert_eq!(["", "abc", ""], b.slots.as_slice());
        assert_eq!("[|abc|]", b.show());

        b.set(2, "def");
        assert_eq!(["", "abc", "def"], b.slots.as_slice());
        assert_eq!("[|abc|def]", b.show());

        b.set(1, "");
        assert_eq!(["", "", "def"], b.slots.as_slice());
        assert_eq!("[||def]", b.show());

        b.set(0, "abc");
        b.set(1, "def");
        b.set(2, "ghi");
        assert_eq!(["abc", "def", "ghi"], b.slots.as_slice());
        assert_eq!("[abc|def|ghi]", b.show());

        b.clear(0);
        assert_eq!(["   ", "def", "ghi"], b.slots.as_slice());
        assert_eq!("[   |def|ghi]", b.show());

        b.expire(1);
        assert_eq!(["   ", "___", "ghi"], b.slots.as_slice());
        assert_eq!("[   |___|ghi]", b.show());
    }
}
