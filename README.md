# srix4k

This crate is a higher-level way to read/write to SRIX4K tags.

The most interesting feature about this crate is the `Srix4kCached` struct. This struct caches memory accesses to the connected SRIX4K tag, speeding up read/write operations. To write data to the tag call the `.sync()` method on the `Srix4kCached` struct.

## Example

```rust
use nfc1::{Result};
use srix4k::{Srix4kCached, mem};

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
```
