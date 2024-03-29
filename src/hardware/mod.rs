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

/// Dummy ADC structure for testing.
///
/// When read from, it will always return the same number every time.
///
/// # Examples
///
/// ```
/// # use slonk::ControllerError;
/// #
/// # fn main() -> Result<(), ControllerError> {
/// use slonk::hardware::{ReturnsNumber, Adc};
///
/// let mut adc = ReturnsNumber(4);
///
/// assert_eq!(adc.read(0)?, 4);
/// assert_eq!(adc.read(1)?, 4);
/// assert_eq!(adc.read(2)?, 4);
///
/// # Ok(())
/// # }
/// ```
pub struct ReturnsNumber(pub u16);

/// A structure for testing GPIO writes.
///
/// A `ListenerPin` stores the history of all writes to it.
/// When read from, a `ListenerPin` will return the last written value of the pin.
///
/// # Examples
///
/// ```
/// # use gpio_cdev;
/// #
/// # fn main() -> Result<(), gpio_cdev::Error> {
///
/// use slonk::hardware::{ListenerPin, GpioPin};
///
/// let mut pin = ListenerPin::new(false);
///
/// assert_eq!(pin.read()?, false);
///
/// pin.write(true);
/// assert_eq!(pin.read()?, true);
///
/// assert_eq!(pin.history(), &vec![false, true]);
///
/// # Ok(())
/// # }
/// ```
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

        // First byte sent:
        // 5 zeros (don't tell the ADC to start just yet)
        // Start bit: 1 (tell ADC to start listening)
        // SGL/DIFF bit: 1
        // D2: highest bit of channel ID
        // --
        // Second byte sent:
        // D1: second-highest bit of channel ID
        // D0: LSB of channel ID
        // 6 zeros (don't matter)
        // --
        // Third byte sent:
        // 8 zeros (don't matter)
        let outgoing = [0x6 | channel >> 2, (channel & 0x3) << 6, 0];
        // this buffer will be populated with ADC data by the time we're done
        let mut incoming = [0; 3];

        // sanity check that our buffers are correctly sized
        assert_eq!(outgoing.len(), incoming.len());

        // perform an SPI transfer
        self.device.transfer(&outgoing, &mut incoming)?;

        // First byte received:
        // 8 high-Z values
        // --
        // Second byte received:
        // 3 high-Z values
        // 1 zero (null)
        // B11..=B8 (high 4 bits of ADC reading)
        // --
        // Third byte received:
        // B7..=B0 (low 8 bits of ADC reading)

        // Verify that we receieved a null bit (implies the ADC is actually any good)
        if incoming[1] & 0x10 != 0 {
            return Err(ControllerError::Hardware(
                "no null bit received from ADC - is it connected?",
            ));
        }

        // Mask out high-Z data in incoming bytes
        incoming[1] &= 0x0F;

        // the back two bytes of `incoming` now have our data in big endian representation.
        Ok(u16::from_be_bytes([incoming[1], incoming[2]]))
    }
}

impl Adc for ReturnsNumber {
    fn read(&mut self, _: u8) -> Result<u16, crate::ControllerError> {
        Ok(self.0)
    }
}

impl<T: GpioPin + ?Sized> GpioPin for Box<T> {
    fn read(&mut self) -> Result<bool, gpio_cdev::Error> {
        self.as_mut().read()
    }

    fn write(&mut self, value: bool) -> Result<(), gpio_cdev::Error> {
        self.as_mut().write(value)
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::{
        spi::{Bus, Device},
        *,
    };

    /// A GPIO spoof pin which reads off a vector of values, and which cannot be written to.
    struct VectorPin {
        values: Vec<bool>,
        index: usize,
    }

    impl GpioPin for VectorPin {
        fn read(&mut self) -> Result<bool, gpio_cdev::Error> {
            let value = self.values[self.index];
            self.index += 1;
            self.index %= self.values.len();
            Ok(value)
        }

        fn write(&mut self, _value: bool) -> Result<(), gpio_cdev::Error> {
            panic!("cannot write to vector pin");
        }
    }
    #[test]
    /// Test a successful MCP3208 ADC read with spoofed gpio pins.
    fn mcp3208_read() {
        let bus = Mutex::new(Bus::<Box<dyn GpioPin>> {
            period: Duration::from_micros(1),
            pin_mosi: Box::new(ListenerPin::new(false)),
            pin_miso: Box::new(VectorPin {
                values: vec![
                    true,  // MSB of first byte read
                    true,  //
                    true,  //
                    true,  //
                    true,  //
                    true,  //
                    true,  //
                    true,  // LSB of first byte read
                    true,  // MSB of second byte read
                    true,  //
                    true,  //
                    false, // null bit
                    true,  // MSB of ADC read value
                    false, //
                    true,  //
                    false, // LSB of second byte read
                    true,  // MSB of third byte read
                    false, //
                    false, //
                    true,  //
                    false, //
                    false, //
                    true,  //
                    false, // LSB of third byte read / LSB of read value
                ],
                index: 0,
            }),
            pin_clk: Box::new(ListenerPin::new(false)),
        });
        let dev = Device::new(&bus, Box::new(ListenerPin::new(true)));
        let mut adc = Mcp3208::new(dev);

        assert_eq!(adc.read(0).unwrap(), 2706);
    }

    #[test]
    /// Test that reading the ADC fails if the null bit is bad.
    fn mcp3208_bad_null_bit() {
        let bus = Mutex::new(Bus::<Box<dyn GpioPin>> {
            period: Duration::from_micros(1),
            pin_mosi: Box::new(ListenerPin::new(false)),
            pin_miso: Box::new(VectorPin {
                values: vec![
                    true,  // MSB of first byte read
                    true,  //
                    true,  //
                    true,  //
                    true,  //
                    true,  //
                    true,  //
                    true,  // LSB of first byte read
                    true,  // MSB of second byte read
                    true,  //
                    true,  //
                    true,  // null bit
                    true,  // MSB of ADC read value
                    false, //
                    true,  //
                    false, // LSB of second byte read
                    true,  // MSB of third byte read
                    false, //
                    false, //
                    true,  //
                    false, //
                    false, //
                    true,  //
                    false, // LSB of third byte read / LSB of read value
                ],
                index: 0,
            }),
            pin_clk: Box::new(ListenerPin::new(false)),
        });
        let dev = Device::new(&bus, Box::new(ListenerPin::new(true)));
        let mut adc = Mcp3208::new(dev);

        assert!(adc.read(0).is_err());
    }
}
