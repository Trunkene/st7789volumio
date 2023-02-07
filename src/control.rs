///
/// SPI Control parts.
///

use rppal::gpio::OutputPin;
use rppal::spi::Spi;

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum DisplayError {
    /// Invalid data format selected for interface selected
    InvalidFormatError,
    /// Unable to write to bus
    BusWriteError,
    /// Unable to assert or de-assert data/command switching signal
    DCError,
    /// Unable to assert chip select signal
    CSError,
    /// The requested DataFormat is not implemented by this display interface implementation
    DataFormatNotImplemented,
    /// Unable to assert or de-assert reset signal
    RSError,
    /// Attempted to write to a non-existing pixel outside the display's bounds
    OutOfBoundsError,
}

///
/// Data-type definitions.
///

// Use this if default CS used for specific spi.
pub struct SPIInterfaceAutoCS {
    spi: Spi,
    dc: OutputPin,
}

// Use this if not default CS port is need to switch manually.
// Note: Never use this if CS is default for spi.
pub struct SPIInterfaceManualCS {
    spi_no_cs: SPIInterfaceAutoCS,
    cs: OutputPin,
}

pub trait WriteOnlyDataCommand {
    /// Send a batch of commands to display
    fn send_command(&mut self, cmd: u8) -> Result<(), DisplayError>;

    /// Send pixel data to display
    fn send_data(&mut self, data: &[u8]) -> Result<(), DisplayError>;
}

impl SPIInterfaceManualCS {
    pub fn new(spi: Spi, dc: OutputPin, cs: OutputPin) -> Self {
        Self {
            spi_no_cs: SPIInterfaceAutoCS::new(spi, dc),
            cs,
        }
    }

    /// SPI operation with manual gpio switch.
    fn with_cs(
        &mut self,
        f: impl FnOnce(&mut SPIInterfaceAutoCS) -> Result<(), DisplayError>,
    ) -> Result<(), DisplayError> {
        // Assert chip select pin
        self.cs.set_low();

        let result = f(&mut self.spi_no_cs);

        // Deassert chip select pin
        self.cs.set_high();

        result
    }
}

impl WriteOnlyDataCommand for SPIInterfaceManualCS {
    fn send_command(&mut self, cmd: u8) -> Result<(), DisplayError> {
        self.with_cs(|spi_no_cs| spi_no_cs.send_command(cmd))
    }

    fn send_data(&mut self, buf: &[u8]) -> Result<(), DisplayError> {
        self.with_cs(|spi_no_cs| spi_no_cs.send_data(buf))
    }
}

impl SPIInterfaceAutoCS {
    pub fn new(spi: Spi, dc: OutputPin) -> Self {
        Self { spi, dc }
    }
}

impl WriteOnlyDataCommand for SPIInterfaceAutoCS {
    fn send_command(&mut self, cmd: u8) -> Result<(), DisplayError> {
        self.dc.set_low();
        self.spi
            .write(&[cmd])
            .map_err(|_| DisplayError::BusWriteError);
        Ok(())
    }

    fn send_data(&mut self, data: &[u8]) -> Result<(), DisplayError> {
        // Set DI low for command, high for data.
        self.dc.set_high();
        self.spi
            .write(data)
            .map_err(|_| DisplayError::BusWriteError);
        Ok(())
    }
}
