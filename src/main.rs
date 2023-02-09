//! Volumio TFT st7789 viewer

use st7789volumio::control::{SPIInterfaceAutoCS, WriteOnlyDataCommand};
use st7789volumio::{St7789, St7789Img, ROTATION};

use chrono::Local;
use image::imageops;
use image::imageops::FilterType;
use image::{Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use rppal::{gpio, spi};
use rppal::{
    gpio::{Gpio, Level},
    spi::{Bus, SlaveSelect, Spi},
};
use rusttype::{point, Font, Scale};
use serde::Deserialize;
use serde_aux::prelude::*;
use serde_with::*;
use std::{
    env,
    env::Args,
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

///
/// Data-type definitions.
///

/// Volumio info
#[serde_as]
#[derive(Deserialize, Clone)]
pub struct Info {
    #[serde_as(as = "DefaultOnNull")]
    pub status: String,
    #[serde_as(as = "DefaultOnNull")]
    pub title: String,
    #[serde_as(as = "DefaultOnNull")]
    pub album: String,
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

/// Global status
pub struct State<'a> {
    pub pre_info: Info,
    pub mpd_status_change: bool,

    pub baseimg: RgbaImage,

    pub title_txt_img: Option<RgbaImage>,
    pub album_txt_img: Option<RgbaImage>,
    pub artist_txt_img: Option<RgbaImage>,

    pub title_x: u32,
    pub album_x: u32,
    pub artist_x: u32,

    pub seek_pos: u32,

    scale_xl: Scale,
    scale_l: Scale,
    scale_m: Scale,
    scale_s: Scale,

    font_i: Font<'a>,
    font_n: Font<'a>,

    pub color_black: image::Rgba<u8>,
    pub color_white: image::Rgba<u8>,
    pub color_grey: image::Rgba<u8>,
    pub color_lightblue: image::Rgba<u8>,
}

impl State<'_> {
    pub fn new() -> State<'static> {
        State {
            pre_info: Info::new(),
            mpd_status_change: true,
            baseimg: {
                let mut baseimg = RgbaImage::new(DISP_WIDTH, DISP_HEIGHT);
                draw_filled_rect_mut(
                    &mut baseimg,
                    Rect::at(0, 0).of_size(DISP_WIDTH, DISP_HEIGHT),
                    Rgba::from([0, 0, 0, 255]),
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

            color_black: Rgba::from([0, 0, 0, 255]),
            color_white: Rgba::from([255, 255, 255, 255]),
            color_grey: Rgba::from([120, 120, 120, 255]),
            color_lightblue: Rgba::from([176, 224, 255, 255]),
        }
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
fn get_text_img(font: &Font, text: &str, scale: Scale, col: image::Rgba<u8>) -> Option<RgbaImage> {
    if text.is_empty() {
        None
    } else {
        let t_w: u32;
        let t_h: u32;
        let black: image::Rgba<u8> = Rgba::from([0, 0, 0, 255]);

        // Title text image
        (t_w, t_h) = calc_text_size(&font, text, scale);

        let w = if t_w <= DISP_AREA_WIDTH {
            t_w
        } else {
            t_w + 20 + DISP_AREA_WIDTH
        };
        let mut img = RgbaImage::new(w, t_h);
        draw_filled_rect_mut(&mut img, Rect::at(0, 0).of_size(w, t_h), black);
        draw_text_mut(&mut img, col, 0, 0, scale, &font, text);
        if t_w > DISP_AREA_WIDTH {
            draw_text_mut(&mut img, col, t_w + 20, 0, scale, &font, text);
        }
        Some(img)
    }
}

/// Get Information from Volumio.
fn update_state(mut state: &mut State) -> Result<(), Box<dyn std::error::Error>> {
    // get MDP status
    if let Ok(res) = reqwest::blocking::get(format!("{}{}", MDP_BASE_URL, GET_STATE_API)) {
        if let Ok(info) = res.json::<Info>() {
            let baseimg = &mut state.baseimg;
            let pre_info = &mut state.pre_info;

            if !info.status.eq(&pre_info.status) {
                draw_filled_rect_mut(
                    baseimg,
                    Rect::at(DISP_AREA_MARGIN_X, DISP_AREA_MARGIN_Y)
                        .of_size(DISP_AREA_WIDTH, DISP_AREA_HEIGHT),
                    state.color_black,
                );

                state.mpd_status_change = true;
            }

            // Title changed
            if !info.title.eq(&pre_info.title) {
                state.title_x = 0;
                state.title_txt_img = get_text_img(
                    &state.font_i,
                    &info.title,
                    state.scale_l,
                    state.color_lightblue,
                );
                draw_filled_rect_mut(
                    baseimg,
                    Rect::at(TITLE_INFO_X, TITLE_INFO_Y)
                        .of_size(TITLE_INFO_WIDTH, TITLE_INFO_HEIGHT),
                    state.color_black,
                );
            }
            // Album changed
            if !info.album.eq(&pre_info.album) {
                state.album_x = 0;
                state.album_txt_img =
                    get_text_img(&state.font_i, &info.album, state.scale_m, state.color_white);
                draw_filled_rect_mut(
                    baseimg,
                    Rect::at(ALBUM_INFO_X, ALBUM_INFO_Y)
                        .of_size(ALBUM_INFO_WIDTH, ALBUM_INFO_HEIGHT),
                    state.color_black,
                );
            }
            // Artist changed
            if !info.artist.eq(&pre_info.artist) {
                state.artist_x = 0;
                state.artist_txt_img = get_text_img(
                    &state.font_i,
                    &info.artist,
                    state.scale_m,
                    state.color_white,
                );
                draw_filled_rect_mut(
                    baseimg,
                    Rect::at(ARTIST_INFO_X, ARTIST_INFO_Y)
                        .of_size(ARTIST_INFO_WIDTH, ARTIST_INFO_HEIGHT),
                    state.color_black,
                );
            }
            // Albumart changed
            if !info.albumart.eq(&pre_info.albumart) || state.mpd_status_change {
                // Thumbnail
                let img_bytes =
                    if info.albumart.starts_with("http:") {
                    reqwest::blocking::get(format!("{}", &info.albumart))?
                        .bytes()?
                    } else {
                    reqwest::blocking::get(format!("{}{}", MDP_BASE_URL, &info.albumart))?
                        .bytes()?
                    };
                let img = image::load_from_memory(&img_bytes).unwrap();

                let resized_img = img.resize(THUMB_WIDTH, THUMB_HEIGHT, FilterType::Triangle);

                imageops::overlay(baseimg, &resized_img, THUMB_X as u32, THUMB_Y as u32);
                draw_hollow_rect_mut(
                    baseimg,
                    Rect::at(THUMB_X, THUMB_Y).of_size(THUMB_WIDTH, THUMB_HEIGHT),
                    state.color_white,
                );
            }
            // SampleRate/BitDepth/Channels
            if let Some(sr) = info.samplerate.split_whitespace().next() {
                let sr0 = format!("{:.0}", f64::from_str(&sr)? * 1000.0);
                if let Some(bd) = info.bitdepth.split_whitespace().next() {
                    let s = format!("{}:{}:{}", sr0, bd, info.channels);
                    draw_filled_rect_mut(
                        baseimg,
                        Rect::at(AUDIO_X, AUDIO_Y).of_size(AUDIO_WIDTH, AUDIO_HEIGHT),
                        state.color_black,
                    );
                    draw_text_mut(
                        baseimg,
                        state.color_white,
                        AUDIO_X as u32,
                        AUDIO_Y as u32,
                        state.scale_s,
                        &state.font_n,
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
            if (seek_pos != state.seek_pos) || state.mpd_status_change {
                draw_filled_rect_mut(
                    baseimg,
                    Rect::at(SEEK_X, SEEK_Y).of_size(SEEK_WIDTH, SEEK_HEIGHT),
                    state.color_grey,
                );
                if seek_pos > 0 {
                    draw_filled_rect_mut(
                        baseimg,
                        Rect::at(SEEK_X, SEEK_Y).of_size(seek_pos, SEEK_HEIGHT),
                        state.color_white,
                    );
                }
                state.seek_pos = seek_pos;
            }

            // CPU temperature
            let temp = match fs::read_to_string(CPU_THM_FILE) {
                Ok(temp) => {
                    let n: f32 = temp.trim().parse::<f32>().unwrap() / 1000.0f32;
                    format!("CPU {:.1} C", n)
                }
                Err(_) => "CPU --.- C".to_string(),
            };
            draw_filled_rect_mut(
                baseimg,
                Rect::at(CPU_THM_X, CPU_THM_Y).of_size(CPU_THM_WIDTH, CPU_THM_HEIGHT),
                state.color_black,
            );
            draw_text_mut(
                baseimg,
                Rgba([255u8, 255u8, 255u8, 255u8]),
                CPU_THM_X as u32,
                CPU_THM_Y as u32,
                state.scale_s,
                &state.font_n,
                &temp,
            );

            // backup info
            *pre_info = info.clone();
            state.mpd_status_change = false;
        }
    }
    Ok(())
}

/// Update image in clock mode.
fn draw_clock(mut state: &mut State) {
    let baseimg = &mut state.baseimg;
    let dt = Local::now();

    draw_filled_rect_mut(
        baseimg,
        Rect::at(DISP_AREA_MARGIN_X, DISP_AREA_MARGIN_Y).of_size(DISP_AREA_WIDTH, DISP_AREA_HEIGHT),
        state.color_black,
    );
    draw_text_mut(
        baseimg,
        state.color_white,
        DATE_INFO_X as u32,
        DATE_INFO_Y as u32,
        state.scale_m,
        &state.font_n,
        &dt.format("%a %m-%d-%Y").to_string(),
    );
    draw_text_mut(
        baseimg,
        state.color_white,
        TIME_INFO_X as u32,
        TIME_INFO_Y as u32,
        state.scale_xl,
        &state.font_n,
        &dt.format("%H:%M").to_string(),
    );
}

/// Update image in playing mode.
fn draw_music_info(mut state: &mut State) {
    let mut restart_scroll = true;
    let baseimg = &mut state.baseimg;

    if let Some(ref mut title_txt_img) = state.title_txt_img {
        let title_x = state.title_x;
        if title_txt_img.width() > DISP_AREA_WIDTH {
            let h0 = title_txt_img.height();
            let img0 = imageops::crop(title_txt_img, title_x, 0, DISP_AREA_WIDTH, h0);
            imageops::overlay(baseimg, &img0, TITLE_INFO_X as u32, TITLE_INFO_Y as u32);

            if title_x < title_txt_img.width() - DISP_AREA_WIDTH {
                state.title_x = title_x + 1;

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

    if let Some(ref mut album_txt_img) = state.album_txt_img {
        let album_x = state.album_x;
        if album_txt_img.width() > DISP_AREA_WIDTH {
            let h0 = album_txt_img.height();
            let img0 = imageops::crop(album_txt_img, album_x, 0, DISP_AREA_WIDTH, h0);
            imageops::overlay(baseimg, &img0, ALBUM_INFO_X as u32, ALBUM_INFO_Y as u32);

            if album_x < album_txt_img.width() - DISP_AREA_WIDTH {
                state.album_x = album_x + 1;

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

    if let Some(ref mut artist_txt_img) = state.artist_txt_img {
        let artist_x = state.artist_x;
        if artist_txt_img.width() > DISP_AREA_WIDTH {
            let h0 = artist_txt_img.height();
            let img0 = imageops::crop(artist_txt_img, artist_x, 0, DISP_AREA_WIDTH, h0);
            imageops::overlay(baseimg, &img0, ARTIST_INFO_X as u32, ARTIST_INFO_Y as u32);

            if artist_x < artist_txt_img.width() - DISP_AREA_WIDTH {
                state.artist_x = artist_x + 1;

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
            state.title_x = 0;
            state.album_x = 0;
            state.artist_x = 0;
        }
    }
}

/// Output Usage
fn usage() {
    println!("st7789volumio");
    println!("");
    println!("Usage: st7789volumio [OPTIONS]");
    println!("");
    println!("Options:");
    println!(" -s<spi_bus>      SPI bus (0, 1, 2): Default 0");
    println!(" -c<cs_pin>        Slave Select pin (0, 1, 2): Default 0");
    println!("                       spi = 0, cs = 0...GPIO 8, 1...GPIO 7");
    println!("                       spi = 1, cs = 0...GPIO 18, 1...GPIO 17, 2...GIPI16");
    println!("                       spi = 2, cs = 0...GPIO 43, 1...GPIO 44, 2...GIPI45");
    println!(" -d<pin>           GPIO pin number for DC: Default 25");
    println!(" -r<pin>          GPIO pin number for RST: Default 27");
    println!(" -b<pin>          GPIO pin number for BLK: Default 24");
}

/// Get Command-line parameters.
fn get_param() -> (u8, u8, u8, u8, u8) {
    let mut spi = DEF_SPI_BUS;
    let mut cs = DEF_CS_PIN;
    let mut dc = DEF_GPIO_DC_PIN;
    let mut rst = DEF_GPIO_RST_PIN;
    let mut blk = DEF_GPIO_BLK_PIN;

    for arg in env::args() {
        if &arg[0..1] == "-" {
            let v = &arg[2..];
            let val = match v.parse::<u8>() {
                Ok(val) => match &arg[0..2] {
                    "-s" => spi = val,
                    "-c" => cs = val,
                    "-d" => dc = val,
                    "-r" => rst = val,
                    "-b" => blk = val,
                    _ => {
                        usage();
                        panic!()
                    }
                },
                Err(e) => {
                    usage();
                    panic!()
                }
            };
        }
    }
    (spi, cs, dc, rst, blk)
}

/// Main routine
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Args = env::args();
    let (spi_n, cs_n, dc_n, rst_n, blk_n) = get_param();

    let mut state = State::new();

    let mut gpio = Gpio::new().expect("Failed Gpio::new");
    let mut dc_pin = gpio.get(dc_n)?.into_output();
    let mut rst_pin = gpio.get(rst_n)?.into_output();
    let mut blk_pin = gpio.get(blk_n)?.into_output();
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
    let mut spi =
        Spi::new(spi_bus, cs, SPI_MAXSPEED_HZ, spi::Mode::Mode3).expect("failed Spi::new");

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
    st7789.init();

    let mut is_first = true;
    let mut now_t = Instant::now();
    let mut pre_t = now_t;

    loop {
        now_t = Instant::now();
        let dur = now_t.duration_since(pre_t);

        if dur.as_secs() > INFO_INTERVAL_SEC || is_first {
            pre_t = now_t;
            is_first = false;
            let _ = update_state(&mut state);
        }
        let interval = if state.pre_info.status.eq("play") {
            draw_music_info(&mut state);
            DISP_INTERVAL_MSEC
        } else {
            draw_clock(&mut state);
            CLOCK_INTERVAL_MSEC
        };

        let mut baseimg = &mut state.baseimg;
        st7789img.set_image(&mut baseimg);
        st7789
            .display_img(&st7789img)
            .expect("Failed st7789 display_img");

        thread::sleep(Duration::from_millis(interval));
    }

    Ok(())
}
