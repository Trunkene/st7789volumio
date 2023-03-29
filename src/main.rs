//! Volumio TFT st7789 viewer

use st7789volumio::control::SPIInterfaceAutoCS;
use st7789volumio::{St7789, St7789Img, ROTATION};

use chrono::Local;
use image::imageops;
use image::imageops::FilterType;
use image::{GenericImageView, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use libc::{c_int, c_void, exit};
use rppal::spi;
use rppal::{
    gpio::Gpio,
    spi::{Bus, SlaveSelect, Spi},
};
use rusttype::{point, Font, Scale};
use serde::Deserialize;
use serde_aux::prelude::*;
use serde_with::*;
use spectrum_analyzer::scaling::divide_by_N;
use spectrum_analyzer::windows::hann_window;
use spectrum_analyzer::{samples_fft_to_spectrum, FrequencyLimit};
use std::{
    env,
    ffi::CString,
    fs,
    str::FromStr,
    thread,
    time::{Duration, Instant},
};

///
/// Constants
///

const INFO_FONT: &str = "/home/volumio/.local/share/fonts/TakaoPGothic.ttf";
const NUM_FONT: &str = "/home/volumio/.local/share/fonts/led_digital_7.ttf";

const INFO_INTERVAL_SEC: u64 = 2;
const DISP_INTERVAL_MSEC: u64 = 20;
const CLOCK_INTERVAL_MSEC: u64 = 1000;

const DISP_WIDTH: u32 = 240;
const DISP_HEIGHT: u32 = 240;

const DISP_AREA_WIDTH: u32 = 232;
const DISP_AREA_HEIGHT: u32 = 232;

const DISP_AREA_MARGIN_X: i32 = 4;
const DISP_AREA_MARGIN_Y: i32 = 4;

const THUMB_WIDTH: u32 = 120;
const THUMB_HEIGHT: u32 = 120;
const THUMB_X: i32 = 4;
const THUMB_Y: i32 = 116;

const SEEK_WIDTH: u32 = 232;
const SEEK_HEIGHT: u32 = 8;
const SEEK_X: i32 = 4;
const SEEK_Y: i32 = 42;

const CPU_THM_WIDTH: u32 = 106;
const CPU_THM_HEIGHT: u32 = 22;
const CPU_THM_X: i32 = 134;
const CPU_THM_Y: i32 = 192;
const CPU_THM_FILE: &str = "/sys/class/thermal/thermal_zone0/temp";

const AUDIO_WIDTH: u32 = 106;
const AUDIO_HEIGHT: u32 = 26;
const AUDIO_X: i32 = 134;
const AUDIO_Y: i32 = 214;

const TITLE_INFO_WIDTH: u32 = DISP_AREA_WIDTH;
const TITLE_INFO_HEIGHT: u32 = 30;
const TITLE_INFO_X: i32 = DISP_AREA_MARGIN_X;
const TITLE_INFO_Y: i32 = 10;

const ALBUM_INFO_WIDTH: u32 = DISP_AREA_WIDTH;
const ALBUM_INFO_HEIGHT: u32 = 26;
const ALBUM_INFO_X: i32 = DISP_AREA_MARGIN_X;
const ALBUM_INFO_Y: i32 = 58;

const ARTIST_INFO_WIDTH: u32 = DISP_AREA_WIDTH;
const ARTIST_INFO_HEIGHT: u32 = 26;
const ARTIST_INFO_X: i32 = DISP_AREA_MARGIN_X;
const ARTIST_INFO_Y: i32 = 84;

const DATE_INFO_X: i32 = 20;
const DATE_INFO_Y: i32 = 20;
const TIME_INFO_X: i32 = 40;
const TIME_INFO_Y: i32 = 80;

const MDP_BASE_URL: &str = "http://127.0.0.1:3000";
const GET_STATE_API: &str = "/api/v1/getstate";

const DEF_SPI_BUS: u8 = 0;
const DEF_CS_PIN: u8 = 0;
const DEF_GPIO_DC_PIN: u8 = 25;
const DEF_GPIO_RST_PIN: u8 = 27;
const DEF_GPIO_BLK_PIN: u8 = 24;

const SPI_MAXSPEED_HZ: u32 = 48_000_000;

const MPD_FIFO_FILE: &str = "/tmp/snapfifo";
const FQ: u32 = 44100;
const DATA_BIT_LEN: usize = 16;
const FQ_MAX: f64 = 20000.0f64;
const FQ_MIN: f64 = 50.0f64;
const NUM_SAMPLES: usize = 1024;
const CHANNELS: usize = 1;

const SP_X: i32 = 138;
const SP_Y: i32 = 116;
const SP_WIDTH: u32 = 108;
const SP_HEIGHT: u32 = 48;

const SP_BAR_WIDTH: i32 = 4;
const SP_BAR_MARGIN: i32 = 1;
const NUM_BARS: usize = 16;

const SIGNAL16_BUFFLEN: usize = FQ as usize;
const DEF_VZ_OFFSET: u32 = 500; // Default 500msec

///
/// Globals
///

static COLOR_BLACK: Rgba<u8> = Rgba::<u8>([0u8, 0u8, 0u8, 255u8]);
static COLOR_WHITE: Rgba<u8> = Rgba::<u8>([255u8, 255u8, 255u8, 255u8]);
static COLOR_GREY: Rgba<u8> = Rgba::<u8>([120u8, 120u8, 120u8, 255u8]);
static COLOR_LIGHTBLUE: Rgba<u8> = Rgba::<u8>([176u8, 224u8, 255u8, 255u8]);

static COLOR_SP_BAR: Rgba<u8> = Rgba::<u8>([0u8, 255u8, 120u8, 255u8]);

///
/// Data-type definitions.
///

/// Volumio info
#[serde_as]
#[derive(Debug, Deserialize, Clone)]
pub struct Info {
    pub status: String,
    #[serde(default)]
    #[serde_as(as = "DefaultOnNull")]
    pub title: String,
    #[serde(default)]
    #[serde_as(as = "DefaultOnNull")]
    pub album: String,
    #[serde(default)]
    #[serde_as(as = "DefaultOnNull")]
    pub artist: String,
    #[serde_as(as = "DefaultOnNull")]
    pub albumart: String,
    #[serde(default, deserialize_with = "deserialize_string_from_number")]
    pub samplerate: String,
    #[serde(default)]
    pub bitdepth: String,
    #[serde(default)]
    pub channels: u32,
    pub seek: u32,
    #[serde(default)]
    pub duration: u32,
}

impl Info {
    pub fn new() -> Info {
        Info {
            status: { String::new() },
            title: { String::new() },
            album: { String::new() },
            artist: { String::new() },
            albumart: { String::new() },
            samplerate: { String::new() },
            bitdepth: { String::new() },
            channels: 0,
            seek: 0,
            duration: 0,
        }
    }
}

impl Default for Info {
    fn default() -> Self {
        Self::new()
    }
}

/// RingBuffer for Signal (Capacity 1sec)
#[derive(Debug)]
pub struct RingSignal16Buffer {
    capacity: i32,
    tail: i32,
    length: i32,
    buffer: Vec<u8>,
}

impl RingSignal16Buffer {
    pub fn new(max_entry: usize) -> RingSignal16Buffer {
        RingSignal16Buffer {
            capacity: (max_entry * 2) as i32, // for 16bit (2byte)
            tail: 0,
            length: 0,
            buffer: vec![0u8; max_entry * 2],
        }
    }

    /// Clean up
    pub fn clean(&mut self) {
        self.tail = 0;
        self.length = 0;
    }

    /// Get max length to read once
    pub fn once_readable_len(&mut self) -> i32 {
        self.capacity - self.tail
    }

    /// Adjust after read to buffer
    pub fn after_read(&mut self, read_bytes: i32) {
        if self.length < self.capacity {
            self.length += read_bytes;
            if self.length > self.capacity {
                self.length = self.capacity;
            }
        }
        self.tail = (self.tail + read_bytes) % self.capacity;
    }

    /// Get buffer position before entry_num
    pub fn before_pos(&mut self, entry_num: i32) -> Option<i32> {
        if (entry_num <= self.length) && (entry_num <= self.capacity) {
            Some((self.capacity + self.tail - entry_num * 2) % self.capacity)
        } else {
            None
        }
    }
}

/// SpectrumVisualize info
#[derive(Debug)]
pub struct SpInfo {
    fifo_fd: c_int,
    in_amp_max: f64,
    out_amp_max: f64,
    cut_off: Vec<f64>,
    signal: Vec<f32>,
    signal16buff: RingSignal16Buffer,
    offset: u32,
}

impl SpInfo {
    pub fn new(fifo_fd: c_int, offset_msec: u32) -> SpInfo {
        let mut offset: u32 = offset_msec * FQ / 1000;
        if offset > SIGNAL16_BUFFLEN as u32 {
            offset = SIGNAL16_BUFFLEN as u32;
        }

        let mut sp_info = SpInfo {
            fifo_fd,
            in_amp_max: 0_f64,
            out_amp_max: 0_f64,
            cut_off: vec![0.0f64; NUM_BARS],
            signal: vec![0.0f32; NUM_SAMPLES],
            signal16buff: { RingSignal16Buffer::new(SIGNAL16_BUFFLEN * CHANNELS) },
            offset,
        };
        sp_info.in_amp_max = 2_f64.powf(DATA_BIT_LEN as f64) / 2.0;
        sp_info.out_amp_max = sp_info.in_amp_max / 2.0 / 2_f64.sqrt();

        let border_unit: f64 = (FQ_MAX.log10() - FQ_MIN.log10()) / (NUM_BARS as f64);
        for j in 0..NUM_BARS {
            sp_info.cut_off[j] = 10_f64.powf(FQ_MIN.log10() + border_unit * ((j + 1) as f64));
        }
        sp_info
    }

    pub fn fft(&mut self, bar_vals: &mut [f64]) {
        unsafe {
            let mut read_len: isize;

            while {
                let readable_len = self.signal16buff.once_readable_len();

                // do-while
                read_len = libc::read(
                    self.fifo_fd,
                    self.signal16buff.buffer[self.signal16buff.tail as usize..].as_mut_ptr()
                        as *mut c_void,
                    readable_len as usize,
                );
                if read_len > 0 {
                    self.signal16buff.after_read(read_len as i32);
                }
                read_len == readable_len as isize
            } {}
        }
        if let Some(head) = self
            .signal16buff
            .before_pos((self.offset * CHANNELS as u32) as i32)
        {
            for i in 0..NUM_SAMPLES {
                let j = ((head + (i * CHANNELS * 2) as i32) % self.signal16buff.capacity) as usize;
                // little endian for Intel / Arm
                self.signal[i] = ((((self.signal16buff.buffer[j + 1] as i16) << 8) & -256i16)
                    | (self.signal16buff.buffer[j] & 0x0ff) as i16)
                    as f32
                    / 32767.0;
            }
        } else {
            for (_, bar) in bar_vals.iter_mut().enumerate().take(NUM_BARS) {
                *bar = 0.0f64;
            }
            return;
        }

        let hann_window = hann_window(&self.signal[..]);
        // calc spectrum
        let spectrum_hann_window = samples_fft_to_spectrum(
            // (windowed) samples
            &hann_window,
            // sampling rate
            FQ,
            // optional frequency limit: e.g. only interested in frequencies 50 <= f <= 150?
            FrequencyLimit::Range(FQ_MIN as f32, FQ_MAX as f32),
            //FrequencyLimit::All,
            // optional scale
            Some(&divide_by_N),
        )
        .unwrap();

        let data = spectrum_hann_window.data();
        let f_num = data.len();

        let mut i: usize = 0;
        for (j, bar) in bar_vals.iter_mut().enumerate().take(NUM_BARS) {
            let mut flg: bool = true;
            let mut k = 0;
            *bar = 0.0f64;
            while {
                if i < f_num {
                    let (fr, fr_val) = data[i];
                    if ((fr.val() as f64) < self.cut_off[j]) || (k == 0) {
                        *bar += fr_val.val() as f64;
                        i += 1;
                        k += 1;
                    } else {
                        flg = false;
                    }
                } else {
                    flg = false;
                }
                flg
            } {}
            // Calc average
            if k > 0 {
                *bar /= k as f64;
            }
        }
    }
}

/// Global status
#[derive(Debug)]
pub struct State<'a> {
    pre_info: Info,
    mpd_status_change: bool,

    baseimg: RgbaImage,

    title_txt_img: Option<RgbaImage>,
    album_txt_img: Option<RgbaImage>,
    artist_txt_img: Option<RgbaImage>,

    title_x: u32,
    album_x: u32,
    artist_x: u32,

    seek_pos: u32,

    scale_xl: Scale,
    scale_l: Scale,
    scale_m: Scale,
    scale_s: Scale,

    font_i: Font<'a>,
    font_n: Font<'a>,

    bar_vals: Vec<f64>,
}

impl State<'_> {
    pub fn new() -> State<'static> {
        State {
            pre_info: Info::default(),
            mpd_status_change: true,
            baseimg: {
                let mut baseimg = RgbaImage::new(DISP_WIDTH, DISP_HEIGHT);
                draw_filled_rect_mut(
                    &mut baseimg,
                    Rect::at(0, 0).of_size(DISP_WIDTH, DISP_HEIGHT),
                    COLOR_BLACK,
                );
                baseimg
            },
            title_txt_img: None,
            album_txt_img: None,
            artist_txt_img: None,
            title_x: 0,
            album_x: 0,
            artist_x: 0,
            seek_pos: 0,

            scale_xl: Scale { x: 48.0, y: 48.0 },
            scale_l: Scale { x: 26.0, y: 26.0 },
            scale_m: Scale { x: 22.0, y: 22.0 },
            scale_s: Scale { x: 14.0, y: 14.0 },

            font_i: Font::try_from_vec(fs::read(INFO_FONT).unwrap()).unwrap(),
            font_n: Font::try_from_vec(fs::read(NUM_FONT).unwrap()).unwrap(),

            bar_vals: vec![0.0f64; NUM_BARS],
        }
    }

    /// Calc horizontal and vertical size for text to be draw.
    /// only for single line text.
    fn calc_text_size(font: &Font, text: &str, scale: Scale) -> (u32, u32) {
        if text.is_empty() {
            (0u32, 0u32)
        } else {
            let v_metrics = font.v_metrics(scale);
            let glyphs: Vec<_> = font.layout(text, scale, point(0.0, 0.0)).collect();
            let glyphs_height = (v_metrics.ascent - v_metrics.descent).ceil() as u32;
            let glyphs_width = {
                let max_x = glyphs
                    .last()
                    .map(|g| g.pixel_bounding_box().unwrap().max.x)
                    .unwrap();
                max_x as u32
            };
            (glyphs_width, glyphs_height)
        }
    }

    /// Get image for text.
    fn get_text_img(
        font: &Font,
        text: &str,
        scale: Scale,
        col: image::Rgba<u8>,
    ) -> Option<RgbaImage> {
        if text.is_empty() {
            None
        } else {
            let t_w: u32;
            let t_h: u32;

            // Title text image
            (t_w, t_h) = Self::calc_text_size(font, text, scale);

            let w = if t_w <= DISP_AREA_WIDTH {
                t_w
            } else {
                t_w + 20 + DISP_AREA_WIDTH
            };
            let mut img = RgbaImage::new(w, t_h);
            draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(w, t_h), COLOR_BLACK);
            draw_text_mut(&mut img, col, 0, 0, scale, font, text);
            if t_w > DISP_AREA_WIDTH {
                draw_text_mut(&mut img, col, t_w + 20, 0, scale, font, text);
            }
            Some(img)
        }
    }

    /// Get Information from Volumio.
    pub fn update_state(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // get MDP status
        if let Ok(res) = reqwest::blocking::get(format!("{MDP_BASE_URL}{GET_STATE_API}")) {
            if let Ok(info) = res.json::<Info>() {
                let baseimg = &mut self.baseimg;
                let pre_info = &mut self.pre_info;

                if !info.status.eq(&pre_info.status) {
                    draw_filled_rect_mut(
                        baseimg,
                        Rect::at(DISP_AREA_MARGIN_X, DISP_AREA_MARGIN_Y)
                            .of_size(DISP_AREA_WIDTH, DISP_AREA_HEIGHT),
                        COLOR_BLACK,
                    );

                    self.mpd_status_change = true;
                }

                // Title changed
                if !info.title.eq(&pre_info.title) {
                    self.title_x = 0;
                    self.title_txt_img = Self::get_text_img(
                        &self.font_i,
                        &info.title,
                        self.scale_l,
                        COLOR_LIGHTBLUE,
                    );
                    draw_filled_rect_mut(
                        baseimg,
                        Rect::at(TITLE_INFO_X, TITLE_INFO_Y)
                            .of_size(TITLE_INFO_WIDTH, TITLE_INFO_HEIGHT),
                        COLOR_BLACK,
                    );
                }
                // Album changed
                if !info.album.eq(&pre_info.album) {
                    self.album_x = 0;
                    self.album_txt_img =
                        Self::get_text_img(&self.font_i, &info.album, self.scale_m, COLOR_WHITE);
                    draw_filled_rect_mut(
                        baseimg,
                        Rect::at(ALBUM_INFO_X, ALBUM_INFO_Y)
                            .of_size(ALBUM_INFO_WIDTH, ALBUM_INFO_HEIGHT),
                        COLOR_BLACK,
                    );
                }
                // Artist changed
                if !info.artist.eq(&pre_info.artist) {
                    self.artist_x = 0;
                    self.artist_txt_img =
                        Self::get_text_img(&self.font_i, &info.artist, self.scale_m, COLOR_WHITE);
                    draw_filled_rect_mut(
                        baseimg,
                        Rect::at(ARTIST_INFO_X, ARTIST_INFO_Y)
                            .of_size(ARTIST_INFO_WIDTH, ARTIST_INFO_HEIGHT),
                        COLOR_BLACK,
                    );
                }
                // Albumart changed
                if !info.albumart.eq(&pre_info.albumart) || self.mpd_status_change {
                    // Thumbnail
                    let img_bytes = if info.albumart.starts_with("http") {
                        reqwest::blocking::get(info.albumart.to_string())?.bytes()?
                    } else {
                        reqwest::blocking::get(format!("{}{}", MDP_BASE_URL, &info.albumart))?
                            .bytes()?
                    };
                    let img = image::load_from_memory(&img_bytes).unwrap();

                    let resized_img = img.resize(THUMB_WIDTH, THUMB_HEIGHT, FilterType::Triangle);

                    let x_of: i32 = if resized_img.width() >= THUMB_WIDTH {
                        0
                    } else {
                        ((THUMB_WIDTH - resized_img.width()) / 2) as i32
                    };
                    let y_of: i32 = if resized_img.height() >= THUMB_HEIGHT {
                        0
                    } else {
                        ((THUMB_HEIGHT - resized_img.height()) / 2) as i32
                    };
                    imageops::overlay(
                        baseimg,
                        &resized_img,
                        (THUMB_X + x_of) as u32,
                        (THUMB_Y + y_of) as u32,
                    );
                    draw_hollow_rect_mut(
                        baseimg,
                        Rect::at(THUMB_X, THUMB_Y).of_size(THUMB_WIDTH, THUMB_HEIGHT),
                        COLOR_WHITE,
                    );
                }
                // SampleRate/BitDepth/Channels
                if let Some(sr) = info.samplerate.split_whitespace().next() {
                    let sr0 = format!("{:.0}", f64::from_str(sr)? * 1000.0);
                    if let Some(bd) = info.bitdepth.split_whitespace().next() {
                        let s = format!("{}:{}:{}", sr0, bd, info.channels);
                        draw_filled_rect_mut(
                            baseimg,
                            Rect::at(AUDIO_X, AUDIO_Y).of_size(AUDIO_WIDTH, AUDIO_HEIGHT),
                            COLOR_BLACK,
                        );
                        draw_text_mut(
                            baseimg,
                            COLOR_WHITE,
                            AUDIO_X as u32,
                            AUDIO_Y as u32,
                            self.scale_s,
                            &self.font_n,
                            &s,
                        );
                    }
                }

                // Seek bar
                let seek_pos = if info.duration > 0 {
                    SEEK_WIDTH * info.seek / (info.duration * 1000)
                } else {
                    0
                };
                if (seek_pos != self.seek_pos) || self.mpd_status_change {
                    draw_filled_rect_mut(
                        baseimg,
                        Rect::at(SEEK_X, SEEK_Y).of_size(SEEK_WIDTH, SEEK_HEIGHT),
                        COLOR_GREY,
                    );
                    if seek_pos > 0 {
                        draw_filled_rect_mut(
                            baseimg,
                            Rect::at(SEEK_X, SEEK_Y).of_size(seek_pos, SEEK_HEIGHT),
                            COLOR_WHITE,
                        );
                    }
                    self.seek_pos = seek_pos;
                }

                // CPU temperature
                let temp = match fs::read_to_string(CPU_THM_FILE) {
                    Ok(temp) => {
                        let n: f32 = temp.trim().parse::<f32>().unwrap() / 1000.0f32;
                        format!("CPU {n:.1} C")
                    }
                    Err(_) => "CPU --.- C".to_string(),
                };
                draw_filled_rect_mut(
                    baseimg,
                    Rect::at(CPU_THM_X, CPU_THM_Y).of_size(CPU_THM_WIDTH, CPU_THM_HEIGHT),
                    COLOR_BLACK,
                );
                draw_text_mut(
                    baseimg,
                    COLOR_WHITE,
                    CPU_THM_X as u32,
                    CPU_THM_Y as u32,
                    self.scale_s,
                    &self.font_n,
                    &temp,
                );

                // backup info
                *pre_info = info;
                self.mpd_status_change = false;
            }
        }
        Ok(())
    }

    /// Update image in clock mode.
    pub fn draw_clock(&mut self) {
        let baseimg = &mut self.baseimg;
        let dt = Local::now();

        draw_filled_rect_mut(
            baseimg,
            Rect::at(DISP_AREA_MARGIN_X, DISP_AREA_MARGIN_Y)
                .of_size(DISP_AREA_WIDTH, DISP_AREA_HEIGHT),
            COLOR_BLACK,
        );
        draw_text_mut(
            baseimg,
            COLOR_WHITE,
            DATE_INFO_X as u32,
            DATE_INFO_Y as u32,
            self.scale_m,
            &self.font_n,
            &dt.format("%a %m-%d-%Y").to_string(),
        );
        draw_text_mut(
            baseimg,
            COLOR_WHITE,
            TIME_INFO_X as u32,
            TIME_INFO_Y as u32,
            self.scale_xl,
            &self.font_n,
            &dt.format("%H:%M").to_string(),
        );
    }

    /// Update image in playing mode.
    pub fn draw_music_info(&mut self, sp: &mut Option<&mut SpInfo>) {
        let mut restart_scroll = true;
        let baseimg = &mut self.baseimg;

        if let Some(ref mut title_txt_img) = self.title_txt_img {
            let title_x = self.title_x;
            if title_txt_img.width() > DISP_AREA_WIDTH {
                let h0 = title_txt_img.height();
                let img0 = imageops::crop(title_txt_img, title_x, 0, DISP_AREA_WIDTH, h0);
                imageops::overlay(baseimg, &img0, TITLE_INFO_X as u32, TITLE_INFO_Y as u32);

                if title_x < title_txt_img.width() - DISP_AREA_WIDTH {
                    self.title_x = title_x + 1;

                    restart_scroll = false;
                }
            } else {
                imageops::overlay(
                    baseimg,
                    title_txt_img,
                    TITLE_INFO_X as u32,
                    TITLE_INFO_Y as u32,
                );
            }
        }

        if let Some(ref mut album_txt_img) = self.album_txt_img {
            let album_x = self.album_x;
            if album_txt_img.width() > DISP_AREA_WIDTH {
                let h0 = album_txt_img.height();
                let img0 = imageops::crop(album_txt_img, album_x, 0, DISP_AREA_WIDTH, h0);
                imageops::overlay(baseimg, &img0, ALBUM_INFO_X as u32, ALBUM_INFO_Y as u32);

                if album_x < album_txt_img.width() - DISP_AREA_WIDTH {
                    self.album_x = album_x + 1;

                    restart_scroll = false;
                }
            } else {
                imageops::overlay(
                    baseimg,
                    album_txt_img,
                    ALBUM_INFO_X as u32,
                    ALBUM_INFO_Y as u32,
                );
            }
        }

        if let Some(ref mut artist_txt_img) = self.artist_txt_img {
            let artist_x = self.artist_x;
            if artist_txt_img.width() > DISP_AREA_WIDTH {
                let h0 = artist_txt_img.height();
                let img0 = imageops::crop(artist_txt_img, artist_x, 0, DISP_AREA_WIDTH, h0);
                imageops::overlay(baseimg, &img0, ARTIST_INFO_X as u32, ARTIST_INFO_Y as u32);

                if artist_x < artist_txt_img.width() - DISP_AREA_WIDTH {
                    self.artist_x = artist_x + 1;

                    restart_scroll = false;
                }
            } else {
                imageops::overlay(
                    baseimg,
                    artist_txt_img,
                    ARTIST_INFO_X as u32,
                    ARTIST_INFO_Y as u32,
                );
            }

            if restart_scroll {
                self.title_x = 0;
                self.album_x = 0;
                self.artist_x = 0;
            }
        }

        // draw_spectrum
        if let Some(ref mut sp_info) = sp {
            sp_info.fft(&mut self.bar_vals);

            draw_filled_rect_mut(
                baseimg,
                Rect::at(SP_X, SP_Y).of_size(SP_WIDTH, SP_HEIGHT),
                COLOR_BLACK,
            );
            let mut x = SP_X;

            for i in 0..NUM_BARS {
                // dB + DYNAMIC_RANGE: 90 + GAIN: 10 / DYNAMIC_RANGE
                let mut y: i32 =
                    (SP_HEIGHT as f64 * (self.bar_vals[i].log10() * 20.0 + 100.0) / 90.0) as i32;
                if y < 0 {
                    y = 0;
                } else if y > SP_HEIGHT as i32 {
                    y = SP_HEIGHT as i32;
                }
                if y > 0 {
                    draw_filled_rect_mut(
                        baseimg,
                        Rect::at(x, (SP_HEIGHT + SP_Y as u32 - y as u32) as i32)
                            .of_size(SP_BAR_WIDTH as u32, y as u32),
                        COLOR_SP_BAR,
                    );
                }

                x += SP_BAR_WIDTH + SP_BAR_MARGIN;
            }
        }
    }
}
/// Output Usage
fn usage() {
    println!("st7789volumio");
    println!();
    println!("Usage: st7789volumio [OPTIONS]");
    println!();
    println!("Options:");
    println!(" -s<spi_bus>      SPI bus (0, 1, 2): Default 0");
    println!(" -c<cs_pin>       Slave Select pin (0, 1, 2): Default 0");
    println!("                       spi = 0, cs = 0...GPIO 8, 1...GPIO 7");
    println!("                       spi = 1, cs = 0...GPIO 18, 1...GPIO 17, 2...GIPI16");
    println!("                       spi = 2, cs = 0...GPIO 43, 1...GPIO 44, 2...GIPI45");
    println!(" -d<pin>          GPIO pin number for DC: Default 25");
    println!(" -r<pin>          GPIO pin number for RST: Default 27");
    println!(" -b<pin>          GPIO pin number for BLK: Default 24");
    println!(" -x<sw>           Audio visualizer ON(1)/OFF(0): Default 0");
    println!(" -t<offset>       Vizualizer offset millisec(0-1000): Default 500");
    println!("                       Effective only as -x1 specified");
}

/// Get Command-line parameters.
fn get_param() -> (u8, u8, u8, u8, u8, u8, u32) {
    let mut spi = DEF_SPI_BUS;
    let mut cs = DEF_CS_PIN;
    let mut dc = DEF_GPIO_DC_PIN;
    let mut rst = DEF_GPIO_RST_PIN;
    let mut blk = DEF_GPIO_BLK_PIN;
    let mut vz = 0; // Default Off
    let mut vf = DEF_VZ_OFFSET;

    for arg in env::args() {
        if &arg[0..1] == "-" {
            let v = &arg[2..];
            match v.parse::<u32>() {
                Ok(val) => match &arg[0..2] {
                    "-s" => spi = val as u8,
                    "-c" => cs = val as u8,
                    "-d" => dc = val as u8,
                    "-r" => rst = val as u8,
                    "-b" => blk = val as u8,
                    "-x" => vz = val as u8,
                    "-t" => vf = val,
                    _ => {
                        usage();
                        panic!()
                    }
                },
                Err(_e) => {
                    usage();
                    panic!()
                }
            };
        }
    }
    (spi, cs, dc, rst, blk, vz, vf)
}

/// Main routine
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (spi_n, cs_n, dc_n, rst_n, blk_n, vz_on, vz_ofst) = get_param();

    let mut state = State::new();

    #[allow(unused_assignments)]
    let mut sp_info;
    let mut sp: Option<&mut SpInfo> = None;

    let gpio = Gpio::new().expect("Failed Gpio::new");
    let dc_pin = gpio.get(dc_n)?.into_output();
    let rst_pin = gpio.get(rst_n)?.into_output();
    let blk_pin = gpio.get(blk_n)?.into_output();
    let spi_bus = match spi_n {
        1 => Bus::Spi1,
        2 => Bus::Spi2,
        _ => Bus::Spi0,
    };
    let cs = match cs_n {
        1 => SlaveSelect::Ss1,
        2 => SlaveSelect::Ss2,
        _ => SlaveSelect::Ss0,
    };
    let spi = Spi::new(spi_bus, cs, SPI_MAXSPEED_HZ, spi::Mode::Mode3).expect("failed Spi::new");

    let di = SPIInterfaceAutoCS::new(spi, dc_pin);
    let mut st7789 = St7789::new(
        di,
        Some(rst_pin),
        Some(blk_pin),
        DISP_WIDTH,
        DISP_HEIGHT,
        ROTATION::Rot180,
    );
    let mut st7789img = St7789Img::new(DISP_WIDTH, DISP_HEIGHT);
    // Display
    st7789.init().unwrap();

    // for Spectrum Visualizer
    if vz_on > 0 {
        let fifo_fd: c_int;
        unsafe {
            let file_name = CString::new(MPD_FIFO_FILE).unwrap();
            fifo_fd = libc::open(file_name.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK);
            if fifo_fd == -1 {
                exit(1);
            }
        }
        sp_info = SpInfo::new(fifo_fd, vz_ofst);
        sp = Some(&mut sp_info);
    }

    let mut is_first = true;
    let mut now_t = Instant::now();
    let mut pre_t = now_t;

    loop {
        now_t = Instant::now();
        let dur = now_t.duration_since(pre_t);

        if dur.as_secs() > INFO_INTERVAL_SEC || is_first {
            pre_t = now_t;
            is_first = false;
            let _ = state.update_state();
        }
        let interval = if state.pre_info.status.eq("play") {
            state.draw_music_info(&mut sp);
            DISP_INTERVAL_MSEC
        } else {
            state.draw_clock();
            CLOCK_INTERVAL_MSEC
        };

        let baseimg = &mut state.baseimg;
        st7789img.set_image(baseimg);
        if let Err(_e) = st7789.display_img(&st7789img) {
            // Might be panic and exit is much better...
            eprintln!("Failed st7789 display_img");
        }

        thread::sleep(Duration::from_millis(interval));
    }
    #[allow(unreachable_code)]
    Ok(())
}
