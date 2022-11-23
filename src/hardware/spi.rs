//! Structures and tools for interfacing via Serial Peripheral Interface (SPI).

use std::{thread::sleep, time::Duration};

use super::GpioPin;

/// An SPI bus.
/// This structure contains enough information to talk on SPI, but contains no
/// device data.
pub struct Bus<P: GpioPin> {
    /// The clock period.
    /// The clock period is the time between two rising edges on the clock.
    /// Therefore the length of a pulse (the time between a rising and falling
    /// edge) is half this period.
    pub period: Duration,
    /// The clock pin.
    /// This pin will be actuated on a regular timescale
    pub pin_clk: P,
    /// The Master Output - Slave Input pin.
    /// This pin is used to send messages to slave devices.
    pub pin_mosi: P,
    /// The Master Input - Slave Output pin.
    /// This pin is used to receive messages from slave devices.
    pub pin_miso: P,
}

/// An SPI device.
/// This structure is actually a wrapper for a single chip-selection pin for SPI
/// communication.
pub struct Device<'a, P: GpioPin> {
    /// A reference to the bus that this device lives "inside" of.
    bus: &'a Bus<P>,
    /// The chip selection pin.
    pin_cs: P,
}

impl<'a, P: GpioPin> Device<'a, P> {
    /// Construct a new device, registering its line with the OS.
    ///
    /// # Errors
    ///
    /// This function may return an error if we are unable to acquire the line
    /// from the OS.
    pub fn new(bus: &'a Bus<P>, pin_cs: P) -> Device<'a, P> {
        Device { bus, pin_cs }
    }

    #[must_use]
    /// Get the clock period of this device.
    pub fn clock_period(&self) -> Duration {
        self.bus.period
    }

    /// Perform an SPI transfer operation on this device.
    /// This transfer is big-endian, that is, the most significant bit of each
    /// byte will be transferred first, and the least significant bit of each
    /// byte will be transferred last in the transmission of the byte.
    ///
    /// # Inputs
    ///
    /// * `consumer`: A string describing the consuming process.
    ///     Under most circumstances, it should be a human readable name for the
    ///     thread responsible for this IO.
    ///     If `consumer` is more than 31 characters long, it will be truncated.
    ///
    /// * `outgoing`: The buffer of bytes which will be sent out to the device.
    ///
    /// * `incoming`: The buffer that will be populated with bytes from the
    ///     device.
    ///
    /// # Panics
    ///
    /// This function will panic if the lengths of `outgoing` and `incoming` are
    /// not equal.
    ///
    /// # Errors
    ///
    /// This function will return an error if it is unable to correctly
    /// interface with the GPIO pins.
    pub fn transfer(
        &self,
        consumer: &str,
        outgoing: &[u8],
        incoming: &mut [u8],
    ) -> Result<(), gpio_cdev::Error> {
        assert_eq!(outgoing.len(), incoming.len());

        // pull chip select down to begin talking
        self.pin_cs.write(consumer, false)?;

        for (byte_out, byte_in) in outgoing.iter().zip(incoming.iter_mut()) {
            // Iterate in reverse because we are performing a big endian
            // transfer
            for bit_idx in (0..8).rev() {
                self.bus
                    .pin_mosi
                    .write(consumer, (1 << bit_idx & byte_out) != 0)?;
                // perform half a clock wait
                sleep(self.bus.period / 2);
                // rising edge on the clock corresponds to read from device
                self.bus.pin_clk.write(consumer, true)?;
                // read the incoming bit
                let bit_in = u8::from(self.bus.pin_miso.read(consumer)?);
                *byte_in |= bit_in << bit_idx;

                // perform half a clock wait
                sleep(self.bus.period / 2);
                // falling edge on the clock corresponds to write to device
                self.bus.pin_clk.write(consumer, false)?;
            }
        }

        // bring chip select back up to let it know that we're done talking
        self.pin_cs.write(consumer, true)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::hardware::ListenerPin;

    use super::*;

    #[test]
    fn transfer_byte_zeros() {
        let bus = Bus {
            period: Duration::from_micros(1),
            pin_mosi: ListenerPin::new(false),
            pin_miso: ListenerPin::new(true),
            pin_clk: ListenerPin::new(false),
        };
        let dev = Device::new(&bus, ListenerPin::new(true));
        let mut incoming = [0; 1];

        dev.transfer("test", &[0xAC], &mut incoming).unwrap();

        assert_eq!(incoming, [0xFF]);
        let hist_guard = bus.pin_mosi.history();
        let readout: &[bool] = hist_guard.as_ref();
        assert_eq!(
            readout,
            &[false, true, false, true, false, true, true, false, false]
        );
    }
}
