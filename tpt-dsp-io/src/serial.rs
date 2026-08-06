//! Serial port I/O via [serialport](https://docs.rs/serialport) (pure Rust).
//!
//! Reads raw byte frames from a serial device — the typical transport for
//! microcontrollers and some SDR dongles. Pair with [`crate::iq`] to decode
//! IQ samples.
//!
//! Enabled by the `serial` feature.

use serialport::SerialPort;

/// A blocking serial byte reader.
pub struct SerialReader {
    port: Box<dyn SerialPort>,
}

impl SerialReader {
    /// Open the named serial port at `baud_rate` with 8 data bits, no parity
    /// and one stop bit (8N1).
    ///
    /// # Errors
    ///
    /// Returns an error if the port cannot be opened or configured.
    pub fn open(port_name: &str, baud_rate: u32) -> Result<Self, serialport::Error> {
        let port = serialport::new(port_name, baud_rate).open()?;
        Ok(Self { port })
    }

    /// Read up to `buf.len()` bytes, returning the number read. Blocks until at
    /// least one byte is available (or an error / EOF occurs).
    ///
    /// # Errors
    ///
    /// Returns the underlying I/O error.
    pub fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.port.read(buf)
    }

    /// The underlying port, for advanced configuration (timeouts, flow control).
    pub fn port(&mut self) -> &mut dyn SerialPort {
        self.port.as_mut()
    }
}
