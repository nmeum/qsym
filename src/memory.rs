pub struct Memory {
    buf: Vec<u8>,
}

pub type Addr = usize;

impl Memory {
    pub fn new(size: usize) -> Memory {
        Memory {
            buf: vec![0; size]
        }
    }

    pub fn load_byte(&self, addr: Addr) -> u8 {
        self.buf[addr]
    }

    pub fn load_word(&self, addr: Addr) -> u32 {
        let b0: u32 = self.load_byte(addr).into();
        let b1: u32 = self.load_byte(addr+1).into();
        let b2: u32 = self.load_byte(addr+2).into();
        let b3: u32 = self.load_byte(addr+3).into();

        return b3 | b2 << 8 | b1 << 16 | b0 << 24
    }

    pub fn store_byte(&mut self, addr: Addr, value: u8) {
        self.buf[addr] = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn word() {
        let mut mem = Memory::new(32);
        mem.store_byte(0, 0xde);
        mem.store_byte(1, 0xad);
        mem.store_byte(2, 0xbe);
        mem.store_byte(3, 0xef);

        assert_eq!(mem.load_word(0), 0xdeadbeef);
    }
}
