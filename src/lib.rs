#![no_std]

use embedded_hal::delay::DelayNs;
use one_wire_hal::address::Address;
use one_wire_hal::OneWire;

pub use resolution::Resolution;

use crate::error::Error;

pub const FAMILY_CODE: u8 = 0x28;

pub mod commands;
mod resolution;
pub mod error;

const READ_SLOT_DURATION_MICROS: u32 = 15;

/// All of the data that can be read from the sensor.
#[derive(Debug)]
pub struct SensorData {
    /// Temperature in degrees Celsius. Defaults to 85 on startup
    pub temperature: f32,

    /// The current resolution configuration
    pub resolution: Resolution,

    /// If the last recorded temperature is lower than this, the sensor is put in an alarm state
    pub alarm_temp_low: i8,

    /// If the last recorded temperature is higher than this, the sensor is put in an alarm state
    pub alarm_temp_high: i8,
}

pub struct Ds18b20<O> {
    one_wire: O,
    address: Address,
}

impl<O: OneWire> Ds18b20<O> {
    /// Checks that the given address contains the correct family code, reads
    /// configuration data, then returns a device
    pub fn new(one_wire: O, address: Address) -> Result<Ds18b20<O>, Error> {
        if address.family_code() == FAMILY_CODE {
            Ok(Ds18b20 { one_wire, address })
        } else {
            Err(Error::FamilyCodeMismatch)
        }
    }

    /// Returns the device address
    pub fn address(&self) -> &Address {
        &self.address
    }

    /// Starts a temperature measurement for just this device
    /// You should wait for the measurement to finish before reading the measurement.
    /// The amount of time you need to wait depends on the current resolution configuration
    pub fn start_temp_measurement(
        &mut self,
        delay: &mut impl DelayNs,
    ) -> Result<(), Error>
    {
        self.one_wire.send_command(commands::CONVERT_TEMP, Some(&self.address), delay)?;
        Ok(())
    }

    pub fn read_data(
        &self,
        one_wire: &mut impl OneWire,
        delay: &mut impl DelayNs,
    ) -> Result<SensorData, Error>
    {
        let data = read_data(&self.address, one_wire, delay)?;
        Ok(data)
    }

    pub fn set_config<E>(
        &mut self,
        alarm_temp_low: i8,
        alarm_temp_high: i8,
        resolution: Resolution,
        delay: &mut impl DelayNs,
    ) -> Result<(), Error>
    {
        self.one_wire.send_command(commands::WRITE_SCRATCHPAD, Some(&self.address), delay)?;
        self.one_wire.write_byte(alarm_temp_high.to_ne_bytes()[0], delay)?;
        self.one_wire.write_byte(alarm_temp_low.to_ne_bytes()[0], delay)?;
        self.one_wire.write_byte(resolution.to_config_register(), delay)?;
        Ok(())
    }

    pub fn save_to_eeprom<E>(
        &self,
        one_wire: &mut impl OneWire,
        delay: &mut impl DelayNs,
    ) -> Result<(), Error>
    {
        save_to_eeprom(Some(&self.address), one_wire, delay)
    }

    pub fn recall_from_eeprom<E>(
        &self,
        one_wire: &mut impl OneWire,
        delay: &mut impl DelayNs,
    ) -> Result<(), Error>
    {
        recall_from_eeprom(Some(&self.address), one_wire, delay)
    }
}

/// Starts a temperature measurement for all devices on this one-wire bus, simultaneously
pub fn start_simultaneous_temp_measurement(
    one_wire: &mut impl OneWire,
    delay: &mut impl DelayNs,
) -> Result<(), Error>
{
    one_wire.reset(delay)?;
    one_wire.skip_address(delay)?;
    one_wire.write_byte(commands::CONVERT_TEMP, delay)?;
    Ok(())
}

/// Read the contents of the EEPROM config to the scratchpad for all devices simultaneously.
pub fn simultaneous_recall_from_eeprom(
    one_wire: &mut impl OneWire,
    delay: &mut impl DelayNs,
) -> Result<(), Error>
{
    recall_from_eeprom(None, one_wire, delay)
}

/// Read the config contents of the scratchpad memory to the EEPROMfor all devices simultaneously.
pub fn simultaneous_save_to_eeprom(
    one_wire: &mut impl OneWire,
    delay: &mut impl DelayNs,
) -> Result<(), Error>
{
    save_to_eeprom(None, one_wire, delay)
}

pub fn read_scratchpad(
    address: &Address,
    one_wire: &mut impl OneWire,
    delay: &mut impl DelayNs,
) -> Result<[u8; 9], Error>
{
    one_wire.reset(delay)?;
    one_wire.match_address(address, delay)?;
    one_wire.write_byte(commands::READ_SCRATCHPAD, delay)?;
    let mut scratchpad = [0; 9];
    one_wire.read_bytes(&mut scratchpad, delay)?;
    // check_crc8(&scratchpad)?;
    Ok(scratchpad)
}

fn read_data(
    address: &Address,
    one_wire: &mut impl OneWire,
    delay: &mut impl DelayNs,
) -> Result<SensorData, Error>
{
    let scratchpad = read_scratchpad(address, one_wire, delay)?;

    let resolution = if let Some(resolution) = Resolution::from_config_register(scratchpad[4]) {
        resolution
    } else {
        return Err(Error::CrcMismatch);
    };
    let raw_temp = u16::from_le_bytes([scratchpad[0], scratchpad[1]]);
    let temperature = match resolution {
        Resolution::Bits12 => (raw_temp as f32) / 16.0,
        Resolution::Bits11 => (raw_temp as f32) / 8.0,
        Resolution::Bits10 => (raw_temp as f32) / 4.0,
        Resolution::Bits9 => (raw_temp as f32) / 2.0,
    };
    Ok(SensorData {
        temperature,
        resolution,
        alarm_temp_high: i8::from_le_bytes([scratchpad[2]]),
        alarm_temp_low: i8::from_le_bytes([scratchpad[3]]),
    })
}

fn recall_from_eeprom(
    address: Option<&Address>,
    one_wire: &mut impl OneWire,
    delay: &mut impl DelayNs,
) -> Result<(), Error>
{
    one_wire.send_command(commands::RECALL_EEPROM, address, delay)?;

    // wait for the recall to finish (up to 10ms)
    let max_retries = (10000 / READ_SLOT_DURATION_MICROS) + 1;
    for _ in 0..max_retries {
        if one_wire.read_bit(delay)? == true {
            return Ok(());
        }
    }
    Err(Error::Timeout)
}

fn save_to_eeprom(
    address: Option<&Address>,
    onewire: &mut impl OneWire,
    delay: &mut impl DelayNs,
) -> Result<(), Error>
{
    onewire.send_command(commands::COPY_SCRATCHPAD, address, delay)?;
    delay.delay_us(10000); // delay 10ms for the write to complete
    Ok(())
}
