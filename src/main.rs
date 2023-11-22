extern crate nfc1;
extern crate srix4k;

use nfc1::Result;
use srix4k::{mem, Srix4kCached};

fn main() -> Result<()> {
    let mut context = nfc1::Context::new()?;
    let mut device = context.open()?;
    device.set_property_bool(nfc1::Property::InfiniteSelect, true)?;
    let mut tag = Srix4kCached::connect_from(device)?;

    println!("uid: 0x{:X}", tag.uid_get()?);
    let block00 = tag.eeprom_get_mut(mem::EEPROM.start)?;
    println!("block 00: {:#010X}", block00);
    *block00 = 0xDEADBEEF;
    tag.sync()?;

    Ok(())
}
