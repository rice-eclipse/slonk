/*
  slonk, a rocket engine controller.
  Copyright (C) 2022 Rice Eclipse.

  slonk is free software: you can redistribute it and/or modify
  it under the terms of the GNU General Public License as published by
  the Free Software Foundation, either version 3 of the License, or
  (at your option) any later version.

  slonk is distributed in the hope that it will be useful,
  but WITHOUT ANY WARRANTY; without even the implied warranty of
  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  GNU General Public License for more details.

  You should have received a copy of the GNU General Public License
  along with this program.  If not, see <http://www.gnu.org/licenses/>.
*/

//! Definitions for hardware devices.
//!
//! The goal of this module is to abstract away some of the details of exactly how our hardware
//! works so that we can focus on the business logic elsewhere.

pub mod spi;

use std::time::Duration;

use gpio_cdev::LineHandle;

use crate::ControllerError;

/// A trait for GPIO pins.
pub trait GpioPin {
    /// Perform a GPIO read on this pin.
    /// Returns `true` if the pin is pulled high, and `false` otherwise.
    ///
    /// # Errors
    ///
    /// This can return an error if the read failed.
    fn read(&mut self) -> Result<bool, gpio_cdev::Error>;

    /// Perform a GPIO write on this pin, setting the pin's logic level to `value`.
    ///
    /// # Errors
    ///
    /// This can return an error if the read failed.
    fn write(&mut self, value: bool) -> Result<(), gpio_cdev::Error>;
}

/// A generic trait for an ADC (Analog-to-Digital Converter).
///
/// This is primarily used for dependency injection testing in other parts of the engine controller.
pub trait Adc {
    /// Perform an ADC read.
    ///
    /// To account for multi-channel ADCs, `channel` is the index of the channel.
    /// On an 8-channel ADC, the valid values for the channel would be between 0 and 7.
    ///
    /// # Errors
    ///
    /// This function will return an error if we are unable to read the ADC value.
    fn read(&mut self, channel: u8) -> Result<u16, ControllerError>;
}

/// A structure for interfacing with the MCP3208 ADC.
///
/// The MCP3208 is an 8-channel SPI ADC with 12 bits of resolution, capable of sampling at up to
/// 50k samples per second.
/// It is the primary ADC used in Rice Eclipse's engine controllers.
/// For more information, refer to the
/// [datasheet](https://pdf1.alldatasheet.com/datasheet-pdf/view/74937/MICROCHIP/MCP3208.html).
pub struct Mcp3208<'a, P: GpioPin> {
    /// The SPI device associated with this ADC.
    device: spi::Device<'a, P>,
}

/// A structure for testing GPIO writes.
///
/// A `ListenerPin` stores the history of all writes to it.
/// When read from, a `ListenerPin` will return the last written value of the pin.
pub struct ListenerPin(Vec<bool>);

impl<'a, P: GpioPin> Mcp3208<'a, P> {
    /// The minimum frequency at which the SPI clock can operate for the MCP3208 to work correctly.
    pub const SPI_MIN_FREQUENCY: u64 = 10_000;

    #[must_use]
    /// Construct a new `Mcp3208`.
    /// This will also perform all necessary initialization steps for the ADC.
    /// Additionally, sanity checks are made to ensure that the device is correctly set up and
    /// cannot introduce extra errors.
    ///
    /// # Panics
    ///
    /// This function will panic if the clock period of `device` is less than or equal to 1.2ms,
    /// which is the minimum operating period of an MCP3208 ADC.
    pub fn new(device: spi::Device<'a, P>) -> Mcp3208<'a, P> {
        assert!(
            device.clock_period()
                < Duration::from_micros(1_000_000 / Mcp3208::<P>::SPI_MIN_FREQUENCY)
        );
        Mcp3208 { device }
    }
}

impl ListenerPin {
    #[must_use]
    /// Construct a new `ListenerPin` with only one reading in its history.
    pub fn new(last_value: bool) -> ListenerPin {
        ListenerPin(vec![last_value])
    }

    #[must_use]
    /// Get access to the history inside this pin.
    pub fn history(&self) -> &Vec<bool> {
        &self.0
    }
}

impl<P: GpioPin> Adc for Mcp3208<'_, P> {
    /// Perform an ADC read on channel `channel`.
    /// Returns the raw 12-bit ADC reading of the channel on the device.
    ///
    /// This operation is blocking.
    ///
    /// # Panics
    ///
    /// This function will panic if `channel` is not a legal channel (i.e. not a number from 0
    /// through 7).
    ///
    /// # Errors
    ///
    /// This function will return an error if something goes wrong with GPIO.
    /// For more information, check the documentation in `gpio_cdev`.
    fn read(&mut self, channel: u8) -> Result<u16, ControllerError> {
        assert!((0..8).contains(&channel));

        // We send two "high" bits, and then the channel ID to tell the ADC to use differential mode
        // and read from the channel
        let outgoing = [0x18 | channel, 0, 0];
        // this buffer will be populated with ADC data by the time we're done
        let mut incoming = [0; 3];

        // sanity check that our buffers are correctly sized
        assert_eq!(outgoing.len(), incoming.len());

        // perform an SPI transfer
        self.device.transfer(&outgoing, &mut incoming)?;

        // the back two bytes of `incoming` now have our data in big endian representation.
        Ok(u16::from_be_bytes([incoming[1], incoming[2]]))
    }
}

impl GpioPin for ListenerPin {
    fn read(&mut self) -> Result<bool, gpio_cdev::Error> {
        Ok(*self.0.last().unwrap())
    }

    fn write(&mut self, value: bool) -> Result<(), gpio_cdev::Error> {
        self.0.push(value);

        Ok(())
    }
}

impl GpioPin for LineHandle {
    fn read(&mut self) -> Result<bool, gpio_cdev::Error> {
        Ok(1 == self.get_value()?)
    }

    fn write(&mut self, value: bool) -> Result<(), gpio_cdev::Error> {
        let int_value = u8::from(value);
        self.set_value(int_value)?;

        Ok(())
    }
}
