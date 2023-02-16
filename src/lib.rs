//
// Library for ST7789
//

pub mod control;

use crate::control::WriteOnlyDataCommand;
use image::RgbaImage;
use rppal::gpio::OutputPin;
use std::{cmp, thread, time::Duration};

///
/// Constants
///

const ST7789_NOP: u8 = 0x00;
const ST7789_SWRESET: u8 = 0x01;
const ST7789_RDDID: u8 = 0x04;
const ST7789_RDDST: u8 = 0x09;

const ST7789_SLPIN: u8 = 0x10;
const ST7789_SLPOUT: u8 = 0x11;
const ST7789_PTLON: u8 = 0x12;
const ST7789_NORON: u8 = 0x13;

const ST7789_INVOFF: u8 = 0x20;
const ST7789_INVON: u8 = 0x21;
const ST7789_DISPOFF: u8 = 0x28;
const ST7789_DISPON: u8 = 0x29;

const ST7789_CASET: u8 = 0x2A;
const ST7789_RASET: u8 = 0x2B;
const ST7789_RAMWR: u8 = 0x2C;
const ST7789_RAMRD: u8 = 0x2E;

const ST7789_PTLAR: u8 = 0x30;
const ST7789_VSCRDER: u8 = 0x33;
const ST7789_TEOFF: u8 = 0x34;
const ST7789_TEON: u8 = 0x35;

const ST7789_MADCTL: u8 = 0x36;
const ST7789_VSCAD: u8 = 0x37;

const ST7789_COLMOD: u8 = 0x3A;

const ST7789_FRMCTR1: u8 = 0xB1;
const ST7789_FRMCTR2: u8 = 0xB2;
const ST7789_FRMCTR3: u8 = 0xB3;
const ST7789_INVCTR: u8 = 0xB4;
const ST7789_DISSET5: u8 = 0xB6;

const ST7789_GCTRL: u8 = 0xB7;
const ST7789_GTADJ: u8 = 0xB8;
const ST7789_VCOMS: u8 = 0xBB;

const ST7789_LCMCTRL: u8 = 0xC0;
const ST7789_IDSET: u8 = 0xC1;
const ST7789_VDVVRHEN: u8 = 0xC2;
const ST7789_VRHS: u8 = 0xC3;
const ST7789_VDVS: u8 = 0xC4;
const ST7789_VMCTR1: u8 = 0xC5;
const ST7789_FRCTRL2: u8 = 0xC6;
const ST7789_CABCCTRL: u8 = 0xC7;

const ST7789_RDID1: u8 = 0xDA;
const ST7789_RDID2: u8 = 0xDB;
const ST7789_RDID3: u8 = 0xDC;
const ST7789_RDID4: u8 = 0xDD;

const ST7789_GMCTRP1: u8 = 0xE0;
const ST7789_GMCTRN1: u8 = 0xE1;

const ST7789_PWCTR6: u8 = 0xFC;

const CHUNK_SIZE: u32 = 4096;

///
/// Data-type definitions.
///

#[derive(Debug)]
pub enum Error {
    DisplayError,
}

#[repr(u8)]
#[derive(Copy, Clone)]
pub enum ROTATION {
    Rot0 = 0x00u8,
    Rot90 = 0x60u8,
    Rot180 = 0xc0u8,
    Rot270 = 0xa0u8,
}

pub struct St7789Img {
    width: u32,
    height: u32,
    img_buff: Vec<u8>,
}

pub struct St7789<DI>
where
    DI: WriteOnlyDataCommand,
{
    di: DI,
    pin_rst: Option<OutputPin>,
    pin_backlight: Option<OutputPin>,
    width: u32,
    height: u32,
    rotation: ROTATION,
    x0: u16,
    y0: u16,
    x1: u16,
    y1: u16,
}

impl St7789Img {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            img_buff: vec![0; (width * height * 2) as usize],
        }
    }

    // Convert image to Rgb565 byte array.
    pub fn set_image(&mut self, image: &mut RgbaImage) {
        // Good to check equality of size between image and self
        // but omit for performance.

        // Convert Rgba to Rgb565 ignoring alpha-channel
        //        let data: &mut [u8] = &mut self.img_buff;
        let mut k = 0;
        for i in 0..self.height {
            for j in 0..self.width {
                let p = image.get_pixel_mut(j, i);
                self.img_buff[k] = (p[0] & 0xf8u8) | ((p[1] >> 5) & 0x07u8);
                k = k + 1;
                self.img_buff[k] = ((p[1] << 3) & 0xe0u8) | ((p[2] >> 3) & 0x1fu8);
                k = k + 1;
            }
        }
    }
}

impl<DI> St7789<DI>
where
    DI: WriteOnlyDataCommand,
{
    pub fn new(
        di: DI,
        pin_rst: Option<OutputPin>,
        pin_backlight: Option<OutputPin>,
        width: u32,
        height: u32,
        rotation: ROTATION,
    ) -> Self {
        let mut x_offset = 0u16;
        let mut y_offset = 0u16;
        let mut row_offset = 0u16;
        let mut col_offset = 0u16;

        if width >= 240 {
            // 240x320 and 240x240 display
            row_offset = (320 - height) as u16;
            col_offset = (240 - width) as u16;
        }
        match rotation {
            ROTATION::Rot0 => {}
            ROTATION::Rot90 => {
                x_offset = row_offset;
                y_offset = col_offset;
                if width != height {
                    panic!();
                }
            }
            ROTATION::Rot180 => {
                x_offset = col_offset;
                y_offset = row_offset;
            }
            ROTATION::Rot270 => {
                if width != height {
                    panic!();
                }
            }
        }
        Self {
            di,
            pin_rst,
            pin_backlight,
            width,
            height,
            rotation,
            x0: 0u16 + x_offset,
            y0: 0u16 + y_offset,
            x1: width as u16 + x_offset - 1u16,
            y1: height as u16 + y_offset - 1u16,
        }
    }

    pub fn get_width(&self) -> u32 {
        match self.rotation {
            ROTATION::Rot90 | ROTATION::Rot270 => self.width,
            _ => self.height,
        }
    }

    pub fn get_height(&self) -> u32 {
        match self.rotation {
            ROTATION::Rot90 | ROTATION::Rot270 => self.height,
            _ => self.width,
        }
    }

    pub fn send_command(&mut self, command: u8) -> Result<(), Error> {
        self.di
            .send_command(command)
            .map_err(|_| Error::DisplayError)
    }

    pub fn send_data(&mut self, data: &[u8]) -> Result<(), Error> {
        self.di.send_data(&data).map_err(|_| Error::DisplayError)
    }

    // Reset the display, if reset pin is connected.
    pub fn reset(&mut self) -> Result<(), Error> {
        // Setup reset as output (if provided).
        if let Some(pin) = self.pin_rst.as_mut() {
            pin.set_high();
            thread::sleep(Duration::from_millis(1)); // 1msec
            pin.set_low();
            thread::sleep(Duration::from_millis(1)); // 1msec
            pin.set_high();
            thread::sleep(Duration::from_millis(1)); // 1msec
        }
        Ok(())
    }

    // Initialize the display.
    pub fn init(&mut self) -> Result<(), Error> {
        if let Some(bl) = self.pin_backlight.as_mut() {
            bl.set_low();
            thread::sleep(Duration::from_millis(10)); // 0.01sec
            bl.set_high();
        }
        self.reset()?;

        self.send_command(ST7789_SWRESET)?; // reset display
        thread::sleep(Duration::from_millis(200));
        self.send_command(ST7789_SLPOUT)?; // turn off sleep
        thread::sleep(Duration::from_millis(200));
        self.send_command(ST7789_VSCRDER)?; // vertical scroll definition
        self.send_data(&[0x00u8, 0x00u8, 0x14u8, 0x00u8, 0x00u8, 0x00u8])?; // 0 TSA, 320 VSA, 0 BSA
        self.send_command(ST7789_NORON)?; // turn on display
        thread::sleep(Duration::from_millis(10));
        self.send_command(ST7789_INVON)?; // back?

        self.set_rotation(self.rotation)?;

        self.send_command(ST7789_COLMOD)?; // 16bit 65k color
        self.send_data(&[0x55u8])?;
        self.send_command(ST7789_DISPON)?; // turn on display
        thread::sleep(Duration::from_millis(200));

        Ok(())
    }

    // Set bthe backlight on/off
    pub fn set_backlight(&mut self, is_on: bool) -> Result<(), Error> {
        if let Some(pin) = self.pin_backlight.as_mut() {
            if is_on {
                pin.set_high();
            } else {
                pin.set_low();
            }
            thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }

    // Set display rotation
    pub fn set_rotation(&mut self, rotation: ROTATION) -> Result<(), Error> {
        self.send_command(ST7789_MADCTL)?; // reset display
        self.send_data(&[rotation as u8])?;
        self.rotation = rotation;
        Ok(())
    }

    // Set the pixel address window for proceeding drawing commands.
    // x0 and x1 should define the minimum and muximum x pixel bounds.
    // y0 and y1 should define the minimum and maximum y pixel bounds.
    pub fn set_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) -> Result<(), Error> {
        self.send_command(ST7789_CASET)?; // Column addr set
        self.send_data(&x0.to_be_bytes())?;
        self.send_data(&x1.to_be_bytes())?;
        self.send_command(ST7789_RASET)?; // Row addr set
        self.send_data(&y0.to_be_bytes())?;
        self.send_data(&y1.to_be_bytes())?;
        Ok(())
    }

    // Write the provided image to the hardware
    pub fn display_img(&mut self, img: &St7789Img) -> Result<(), Error> {
        // Set address bounds to entire display
        self.set_window(self.x0, self.y0, self.x1, self.y1)?;

        self.send_command(ST7789_RAMWR)?; // Write to RAM
                                          // Write data to H/W
        let mut i = 0;
        let n = img.img_buff.len();

        while i < n {
            let end = cmp::min(i + CHUNK_SIZE as usize, n);
            let slice = &img.img_buff[i..end];
            self.send_data(slice)?;
            i = end;
        }
        Ok(())
    }
}
