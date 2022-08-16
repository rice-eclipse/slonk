pub mod spi;

use std::time::Duration;

/// A structure for interfacing with the MCP3208 ADC.
///
/// The MCP3208 is an 8-channel SPI ADC with 12 bits of resultion, capable of
/// sampling at up to 50k samples per second.
/// It is the primary ADC used in Rice Eclipse's engine controllers.
pub struct Mcp3208<'a> {
    consumer: &'a str,
    device: spi::Device<'a>,
}

impl<'a> Mcp3208<'a> {
    #[must_use]
    /// Construct a new `Mcp3208`.
    /// This will also perform all necessary initialization steps for the ADC.
    /// Additionally, sanity checks are made to ensure that the device is
    /// correctly set up and cannot introduce extra errors.
    ///
    /// # Panics
    ///
    /// This function will panic if the clock period of `device` is less than
    /// or equal to 1.2ms, which is the minimum operating period of an MCP3208
    /// ADC.
    pub fn new(consumer: &'a str, device: spi::Device<'a>) -> Mcp3208<'a> {
        assert!(device.clock_period() > Duration::from_micros(1200));
        Mcp3208 { consumer, device }
    }

    /// Perform an ADC read on channel `channel`. Returns the raw 12-bit ADC
    /// reading of the channel on the device.
    ///
    /// This operation is blocking.
    ///
    /// # Panics
    ///
    /// This function will panic if `channel` is not a legal channel (i.e. not a
    /// number from 0 through 7).
    ///
    /// # Errors
    ///
    /// This function will return an error if something goes wrong with GPIO.
    /// For more information, check the documentation in `gpio_cdev`.
    pub fn read(&self, channel: u8) -> Result<u16, gpio_cdev::Error> {
        assert!((0..8).contains(&channel));

        // We send two "high" bits, and then the channel ID to tell the ADC to
        // use differential mode and read from the channel
        let outgoing = [0x18 | channel, 0, 0];
        // this buffer will be populated with ADC data by the time we're done
        let mut incoming = [0; 3];

        // sanity check that our buffers are correctly sized
        assert_eq!(outgoing.len(), incoming.len());

        // perform an SPI transfer
        self.device
            .transfer(self.consumer, &outgoing, &mut incoming)?;

        // the back two bytes of `incoming` now have our data in big endian
        // representation.
        Ok(u16::from_be_bytes([incoming[1], incoming[2]]))
    }
}
