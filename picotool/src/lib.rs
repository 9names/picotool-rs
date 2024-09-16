pub mod picoboot;
pub mod picotool_reset;

pub const PICO_PAGE_SIZE: usize = 256;
pub const PICO_SECTOR_SIZE: u32 = 4096;
pub const PICO_FLASH_START: u32 = 0x10000000;
pub const PICO_STACK_POINTER: u32 = 0x20042000;

use picoboot::usb::PicobootConnection;

use std::path::Path;
use uf2_decode::convert_from_uf2;

#[derive(Debug, Clone, Copy)]
pub enum TargetID {
    Rp2040,
    Rp2350,
}

pub fn uf2_pages(bytes: Vec<u8>) -> Result<Vec<Vec<u8>>, uf2_decode::Error> {
    let fw = convert_from_uf2(&bytes)?.0;
    let mut fw_pages: Vec<Vec<u8>> = vec![];
    let len = fw.len();
    for i in (0..len).step_by(PICO_PAGE_SIZE) {
        let size = std::cmp::min(len - i, PICO_PAGE_SIZE);
        let mut page = fw[i..i + size].to_vec();
        page.resize(PICO_PAGE_SIZE, 0);
        fw_pages.push(page);
    }
    Ok(fw_pages)
}

pub struct PicoTool {
    conn: PicobootConnection,
}

impl Default for PicoTool {
    fn default() -> Self {
        Self::new()
    }
}

impl PicoTool {
    pub fn new() -> Self {
        let mut conn = PicobootConnection::new().unwrap();
        conn.reset_interface();
        conn.access_exclusive_eject()
            .expect("failed to claim access");
        conn.exit_xip().expect("failed to exit from xip mode");
        PicoTool { conn }
    }

    pub fn flash_uf2(&mut self, uf2: &Path) {
        let fw = std::fs::read(uf2).unwrap();
        let fw_pages = uf2_pages(fw).unwrap();

        let mut erased_sectors = vec![];

        for (i, page) in fw_pages.iter().enumerate() {
            let addr = (i * PICO_PAGE_SIZE) as u32 + PICO_FLASH_START;
            let size = PICO_PAGE_SIZE as u32;

            // Erase is by sector. Addresses must be on sector boundary
            let sector_addr = addr - (addr % PICO_SECTOR_SIZE);
            if !erased_sectors.contains(&sector_addr) {
                // Sector containing this page hasn't been erased yet, erase it now
                self.conn
                    .flash_erase(addr, PICO_SECTOR_SIZE)
                    .expect("failed to erase flash");
                erased_sectors.push(sector_addr);
            }

            self.conn
                .flash_write(addr, page.to_vec())
                .expect("failed to write flash");

            let read = self
                .conn
                .flash_read(addr, size)
                .expect("failed to read flash");

            let matching = page.iter().zip(&read).filter(|&(a, b)| a == b).count();
            if matching != PICO_PAGE_SIZE {
                panic!(
                    "page failed to match (expected {}, got {})",
                    PICO_PAGE_SIZE, matching
                )
            }
        }

        match self.conn.get_device_type().expect("No known RP chip found") {
            TargetID::Rp2040 => {
                self.conn
                    .reboot(0x0, PICO_STACK_POINTER, 500)
                    .expect("failed to reboot device"); // sp is SRAM_END_RP2040
            }
            TargetID::Rp2350 => self
                .conn
                .reboot2_normal(500)
                .expect("failed to reboot device"),
        }
        println!("Flash success!");
    }
}
