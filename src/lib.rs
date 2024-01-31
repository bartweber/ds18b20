#![no_std]

//! # Test Test
use embedded_hal::blocking::delay::DelayUs;
use one_wire_bus::{self, Address, one_wire_bb, OneWire};

pub use resolution::Resolution;

use crate::error::{DS18B20Error, DS18B20Result};

pub const FAMILY_CODE: u8 = 0x28;

pub mod commands;
mod resolution;
pub mod error;

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

pub struct Ds18b20 {
    address: Address,
}

impl Ds18b20 {
    /// Checks that the given address contains the correct family code, reads
    /// configuration data, then returns a device
    pub fn new<E>(address: Address) -> DS18B20Result<Ds18b20, E> {
        if address.family_code() == FAMILY_CODE {
            Ok(Ds18b20 { address })
        } else {
            Err(DS18B20Error::FamilyCodeMismatch)
        }
    }

    /// Returns the device address
    pub fn address(&self) -> &Address {
        &self.address
    }

    /// Starts a temperature measurement for just this device
    /// You should wait for the measurement to finish before reading the measurement.
    /// The amount of time you need to wait depends on the current resolution configuration
    pub fn start_temp_measurement<E>(
        &self,
        onewire: &mut impl OneWire<Error=E>,
        delay: &mut impl DelayUs<u16>,
    ) -> DS18B20Result<(), E>
    {
        onewire.send_command(commands::CONVERT_TEMP, Some(&self.address), delay)?;
        Ok(())
    }

    pub fn read_data<E>(
        &self,
        onewire: &mut impl OneWire<Error=E>,
        delay: &mut impl DelayUs<u16>,
    ) -> DS18B20Result<SensorData, E>
    {
        let data = read_data(&self.address, onewire, delay)?;
        Ok(data)
    }

    pub fn set_config<E>(
        &self,
        alarm_temp_low: i8,
        alarm_temp_high: i8,
        resolution: Resolution,
        onewire: &mut impl OneWire<Error=E>,
        delay: &mut impl DelayUs<u16>,
    ) -> DS18B20Result<(), E>
    {
        onewire.send_command(commands::WRITE_SCRATCHPAD, Some(&self.address), delay)?;
        onewire.write_byte(alarm_temp_high.to_ne_bytes()[0], delay)?;
        onewire.write_byte(alarm_temp_low.to_ne_bytes()[0], delay)?;
        onewire.write_byte(resolution.to_config_register(), delay)?;
        Ok(())
    }

    pub fn save_to_eeprom<E>(
        &self,
        onewire: &mut impl OneWire<Error=E>,
        delay: &mut impl DelayUs<u16>,
    ) -> DS18B20Result<(), E>
    {
        save_to_eeprom(Some(&self.address), onewire, delay)
    }

    pub fn recall_from_eeprom<E>(
        &self,
        onewire: &mut impl OneWire<Error=E>,
        delay: &mut impl DelayUs<u16>,
    ) -> DS18B20Result<(), E>
    {
        recall_from_eeprom(Some(&self.address), onewire, delay)
    }
}

/// Starts a temperature measurement for all devices on this one-wire bus, simultaneously
pub fn start_simultaneous_temp_measurement<E>(
    onewire: &mut impl OneWire<Error=E>,
    delay: &mut impl DelayUs<u16>,
) -> DS18B20Result<(), E>
{
    onewire.reset(delay)?;
    onewire.skip_address(delay)?;
    onewire.write_byte(commands::CONVERT_TEMP, delay)?;
    Ok(())
}

/// Read the contents of the EEPROM config to the scratchpad for all devices simultaneously.
pub fn simultaneous_recall_from_eeprom<E>(
    onewire: &mut impl OneWire<Error=E>,
    delay: &mut impl DelayUs<u16>,
) -> DS18B20Result<(), E>
{
    recall_from_eeprom(None, onewire, delay)
}

/// Read the config contents of the scratchpad memory to the EEPROMfor all devices simultaneously.
pub fn simultaneous_save_to_eeprom<E>(
    onewire: &mut impl OneWire<Error=E>,
    delay: &mut impl DelayUs<u16>,
) -> DS18B20Result<(), E>
{
    save_to_eeprom(None, onewire, delay)
}

pub fn read_scratchpad<E>(
    address: &Address,
    onewire: &mut impl OneWire<Error=E>,
    delay: &mut impl DelayUs<u16>,
) -> Result<[u8; 9], DS18B20Error<E>>
{
    onewire.reset(delay)?;
    onewire.match_address(address, delay)?;
    onewire.write_byte(commands::READ_SCRATCHPAD, delay)?;
    let mut scratchpad = [0; 9];
    onewire.read_bytes(&mut scratchpad, delay)?;
    // check_crc8(&scratchpad)?;
    Ok(scratchpad)
}

fn read_data<E>(
    address: &Address,
    onewire: &mut impl OneWire<Error=E>,
    delay: &mut impl DelayUs<u16>,
) -> Result<SensorData, DS18B20Error<E>>
{
    let scratchpad = read_scratchpad(address, onewire, delay)?;

    let resolution = if let Some(resolution) = Resolution::from_config_register(scratchpad[4]) {
        resolution
    } else {
        return Err(DS18B20Error::CrcMismatch);
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

fn recall_from_eeprom<E>(
    address: Option<&Address>,
    onewire: &mut impl OneWire<Error=E>,
    delay: &mut impl DelayUs<u16>,
) -> DS18B20Result<(), E>
{
    onewire.send_command(commands::RECALL_EEPROM, address, delay)?;

    // wait for the recall to finish (up to 10ms)
    let max_retries = (10000 / one_wire_bb::READ_SLOT_DURATION_MICROS) + 1;
    for _ in 0..max_retries {
        if onewire.read_bit(delay)? == true {
            return Ok(());
        }
    }
    Err(DS18B20Error::Timeout)
}

fn save_to_eeprom<E>(
    address: Option<&Address>,
    onewire: &mut impl OneWire<Error=E>,
    delay: &mut impl DelayUs<u16>,
) -> DS18B20Result<(), E>
{
    onewire.send_command(commands::COPY_SCRATCHPAD, address, delay)?;
    delay.delay_us(10000); // delay 10ms for the write to complete
    Ok(())
}
