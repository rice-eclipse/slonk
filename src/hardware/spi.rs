//! Structures and tools for interfacing via Serial Peripheral Interface (SPI).

use std::{thread::sleep, time::Duration};

use gpio_cdev::{Line, LineRequestFlags};

/// An SPI bus.
/// This structure contains enough information to talk on SPI, but contains no
/// device data.
pub struct Bus {
    /// The clock period.
    /// The clock period is the time between two rising edges on the clock.
    /// Therefore the length of a pulse (the time between a rising and falling
    /// edge) is half this period.
    period: Duration,
    /// The clock pin.
    /// This pin will be actuated on a regular timescale
    pin_clk: Line,
    /// The Master Output - Slave Input pin.
    /// This pin is used to send messages to slave devices.
    pin_mosi: Line,
    /// The Master Input - Slave Output pin.
    /// This pin is used to receive messages from slave devices.
    pin_miso: Line,
}

/// An SPI device.
/// This structure is actually a wrapper for a single chip-selection pin for SPI
/// communication.
pub struct Device<'a> {
    /// A reference to the bus that this device lives "inside" of.
    bus: &'a Bus,
    /// The chip selection pin.
    pin_cs: Line,
}

impl<'a> Device<'a> {
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

        // collect handles for all pins

        // Chip select is output and defaults to high
        let handle_cs = self.pin_cs.request(LineRequestFlags::OUTPUT, 1, consumer)?;

        // Clock is output and defaults to low
        let handle_clk = self
            .bus
            .pin_clk
            .request(LineRequestFlags::OUTPUT, 0, consumer)?;

        // MOSI is output and defaults to low
        let handle_mosi = self
            .bus
            .pin_mosi
            .request(LineRequestFlags::OUTPUT, 0, consumer)?;

        // MISO is input and defaults to low
        let handle_miso = self
            .bus
            .pin_miso
            .request(LineRequestFlags::INPUT, 0, consumer)?;

        // pull chip select down to begin talking
        handle_cs.set_value(0)?;

        for (byte_out, byte_in) in outgoing.iter().zip(incoming.iter_mut()) {
            // Iterate in reverse because we are performing a big endian
            // transfer
            for bit_idx in (0..8).rev() {
                if (1 << bit_idx) & byte_out == 0 {
                    // write a low bit to MOSI
                    handle_mosi.set_value(0)?;
                } else {
                    // write a high bit to MOSI
                    handle_mosi.set_value(1)?;
                }
                // perform half a clock wait
                sleep(self.bus.period / 2);
                // rising edge on the clock corresponds to read from device
                handle_clk.set_value(1)?;
                // read the incoming bit
                let bit_in = handle_miso.get_value()?;
                *byte_in |= bit_in << bit_idx;

                // perform half a clock wait
                sleep(self.bus.period / 2);
                // falling edge on the clock corresponds to write to device
                handle_clk.set_value(0)?;
            }
        }

        // bring chip select back up to let it know that we're done talking
        handle_cs.set_value(1)?;

        Ok(())
    }
}
