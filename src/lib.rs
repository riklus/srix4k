extern crate log;
extern crate nfc1;

use std::convert::TryInto;
use log::{debug, info, trace};
use nfc1::{Result, Timeout};

/// SRIX4K memory mapping.
pub mod mem {
    use std::ops::Range;

    /// Total number of blocks.
    pub const BLOCK_COUNT: usize = 128;
    /// Size of a single block in bytes.
    pub const BLOCK_SIZE: usize = 4;
    /// Size of the UID in bytes.
    pub const UID_SIZE: u8 = 8;

    /// Entire EEPROM.
    pub const EEPROM: Range<usize> = Range {
        start: 0,
        end: BLOCK_COUNT,
    };

    /// *Resettable OTP bits* region.
    pub const OTP: Range<usize> = Range { start: 0, end: 5 };
    /// *Count down Counter* region.
    pub const COUNTDOWN: Range<usize> = Range { start: 5, end: 7 };
    /// *Lockable EEPROM* region.
    pub const LOCKABLE: Range<usize> = Range { start: 7, end: 16 };
    /// *EEPROM* region.
    pub const GENERIC: Range<usize> = Range {
        start: 16,
        end: BLOCK_COUNT,
    };
    /// *System OTP bits* block.
    pub const SYSTEM_ADDR: usize = 255;
}

/// Commands that can be received by SRIX4K tag in ready state.
pub enum Command {
    /// `ReadBlock(block_address)`
    /// From 0 to 127, or 255 for system. Block Data(LSB)
    ReadBlock(u8),
    /// `WriteBlock(block_address, block_data)`
    /// From 0 to 127, or 255 for system. Block Data(LSB)
    WriteBlock(u8, u32),
    /// UID of tag.
    GetUid,
}

impl From<Command> for Vec<u8> {
    /// Convert command variant to frame that will be sent to the tag.
    fn from(value: Command) -> Self {
        match value {
            Command::ReadBlock(address) => {
                let mut frame = vec![0x08];
                frame.extend(address.to_le_bytes());
                frame
            }
            Command::WriteBlock(address, block_data) => {
                let mut frame = vec![0x09];
                frame.extend(address.to_le_bytes());
                frame.extend(block_data.to_le_bytes());
                frame
            }
            Command::GetUid => vec![0x0B],
        }
    }
}

/// Wrapper structure for a device connected to SRIX4K.
/// Used to send commands.
pub struct Srix4k<'a> {
    /// Reader that is connected to the tag.
    device: nfc1::Device<'a>,
}

impl Srix4k<'_> {
    /// Select SRIX4K near device and connect to it.
    pub fn connect_from<'a>(
        mut device: nfc1::Device<'a>,
    ) -> Result<Srix4k<'a>> {
        debug!("Connecting to target from device {}", device.name());
        device.initiator_list_passive_targets(
            &nfc1::Modulation {
                modulation_type: nfc1::ModulationType::Iso14443b,
                baud_rate: nfc1::BaudRate::Baud106,
            },
            1,
        )?;
        device.initiator_select_passive_target(&nfc1::Modulation {
            modulation_type: nfc1::ModulationType::Iso14443b2sr,
            baud_rate: nfc1::BaudRate::Baud106,
        })?;

        info!("Connected to target from device {}", device.name());

        Ok(Srix4k { device })
    }
}

impl Srix4k<'_> {
    /// Send `ReadBlock` command to the tag with specified block address
    /// and return the block data.
    pub fn send_read_block(&mut self, block_address: u8) -> Result<u32> {
        let frame: Vec<u8> = Command::ReadBlock(block_address).into();
        let response = self.device.initiator_transceive_bytes(
            &frame,
            mem::BLOCK_SIZE.into(),
            Timeout::None,
        )?;
        trace!("Reading block {:#04X}", block_address);

        let block_data = u32::from_le_bytes(
            response
                .try_into()
                .map_err(|_| nfc1::Error::RfTransmissionError)?,
        );

        trace!("{:#04X}: {:#010X}", block_address, block_data);

        Ok(block_data)
    }
    /// Send `WriteBlock` command to the tag
    /// with specified block address and block data.
    pub fn send_write_block(
        &mut self,
        block_address: u8,
        block_data: u32,
    ) -> Result<()> {
        trace!(
            "Writing {:#010X} to block {:#04X}",
            block_data,
            block_address
        );
        let frame: Vec<u8> =
            Command::WriteBlock(block_address, block_data).into();
        self.device.target_send_bytes(&frame, Timeout::None)?;
        Ok(())
    }
    /// Send `GetUID` command to the tag and return UID.
    pub fn send_get_uid(&mut self) -> Result<u64> {
        let frame: Vec<u8> = Command::GetUid.into();
        let response = self.device.initiator_transceive_bytes(
            &frame,
            mem::UID_SIZE.into(),
            Timeout::None,
        )?;
        Ok(u64::from_le_bytes(
            response
                .try_into()
                .map_err(|_| nfc1::Error::RfTransmissionError)?,
        ))
    }
}

/// This structure keeps a copy of the original blocks
/// and a cache to access and modify the tag.  
///
/// To write the modified blocks to the tag call the `sync` method.
pub struct Srix4kCached<'a> {
    /// [0 to 127] EEPROM containing original and the modified value.
    eeprom: [Option<(u32, u32)>; 128],
    /// [225] System OTP bits
    system: Option<(u32, u32)>,
    /// [UID0, UID1] ROM
    uid: Option<u64>,
    /// Connected tag.
    tag: Srix4k<'a>,
}

impl Srix4kCached<'_> {
    /// Select SRIX4K near device and connect to it.
    pub fn connect_from<'a>(
        device: nfc1::Device<'a>,
    ) -> Result<Srix4kCached<'a>> {
        Ok(Srix4kCached {
            eeprom: [None; 128],
            system: None,
            uid: None,
            tag: Srix4k::connect_from(device)?,
        })
    }
}

impl Srix4kCached<'_> {
    /// Get specified block.
    pub fn eeprom_get(&mut self, i: usize) -> Result<u32> {
        match self.eeprom[i] {
            Some(block_data) => Ok(block_data.1),
            None => {
                let block_data = self.tag.send_read_block(i as u8)?;
                self.eeprom[i] = Some((block_data, block_data));
                Ok(block_data)
            }
        }
    }
    /// Get specified block mut.
    pub fn eeprom_get_mut(&mut self, i: usize) -> Result<&mut u32> {
        if self.eeprom[i].is_none() {
            let block_data = self.tag.send_read_block(i as u8)?;
            self.eeprom[i as usize] = Some((block_data, block_data));
        }

        Ok(&mut self.eeprom[i as usize].as_mut().unwrap().1)
    }
    /// Get the System OTP bits.
    pub fn system_get(&mut self) -> Result<u32> {
        match self.system {
            Some(system) => Ok(system.1),
            None => {
                let system =
                    self.tag.send_read_block(mem::SYSTEM_ADDR as u8)?;
                self.system = Some((system, system));
                Ok(system)
            }
        }
    }
    /// Get the System OTP bits mut.
    pub fn system_get_mut(&mut self) -> Result<&mut u32> {
        if self.system.is_none() {
            let system = self.tag.send_read_block(mem::SYSTEM_ADDR as u8)?;
            self.system = Some((system, system));
        }

        Ok(&mut self.system.as_mut().unwrap().1)
    }
    /// Get the UID.
    pub fn uid_get(&mut self) -> Result<u64> {
        match self.uid {
            Some(uid) => Ok(uid),
            None => {
                let uid = self.tag.send_get_uid()?;
                self.uid = Some(uid);
                Ok(uid)
            }
        }
    }
    /// Write modified data to the tag and sync the cache.
    pub fn sync(&mut self) -> Result<()> {
        debug!("Syncing tag {}", self.tag.device.name());
        for (block_address, block_data) in self.eeprom.iter_mut().enumerate() {
            if let Some((original, edited)) = block_data {
                // Write data only if it changed.
                if original != edited {
                    self.tag.send_write_block(block_address as u8, *edited)?;
                    *original = *edited;
                }
            }
        }
        if let Some((original, edited)) = self.system.as_mut() {
            // Write data only if it changed.
            if original != edited {
                self.tag.send_write_block(mem::SYSTEM_ADDR as u8, *edited)?;
                *original = *edited;
            }
        }

        Ok(())
    }
}
