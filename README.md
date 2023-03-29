st7789volumio
====
ST7789(240x240) viewer for Volumio on Raspberry Pi

![demo](LCDdemo.gif)

## Description
私の環境でそれまで使っていたblue-7さん作成のmpd_guiがVolumio3にアップデートした後でコンパイルできなくなったのでRustで書き直しました。  
いくつかネット上のrustで書かれたディスプレイドライバを試すもそのままコンパイルできなかったので、ラズパイで動くようにソースの部分借用をさせていただいています<(_ _)>。  
表示レイアウトはmpd_guiを丸々パクりました。

ちなみに動作はAliExpressで数百円で購入した1.3インチ240x240のTFTディスプレイでのみ行っています。

+ 2023/03/29 Visualizerの音ズレをプログラム側で解消.mpd.confのalsaのbuffer_time変更不要に.
+ 2023/03.07 Audio Visualizerを追加

## Requirement
* volumio (3で確認)
* フォント: (自分の好きなフォントでソースを書き換えてください)  
    * Takao Fonts (https://launchpad.net/takao-fonts) TakaoPGothic.ttf   
    * LED Digital 7 (http://www.styleseven.com/) led_digital_7.ttf

## Compile
私は、Linux環境でクロスコンパイルしました。
[参照](#acknowledgments)

## Install
* SPIの有効化を行うため、/boot/config.txtに下記を追記。  
追記後、再起動。
```
dtparam=spi=on
```

* Audio VisualizerをONにする場合(起動パラメータ -x1)、/volumio/app/plugins/music_service/mpd/mpd.conf.tmplに下記を変更。  
変更後、再起動。

```
audio_output {
    type         "fifo"
    <途中略>
    enabled      "yes"
    path         "/tmp/snapfifo"
    format       "44100:16:1"
}
```

* [Requirement](#requirement)に載せた2つのフォントをダウンロードし、~volumio/.local/share/fontsの下にコピー。  
※フォント、配置場所を変えたい場合はソースで書き換えてください。

* 適当にディレクトリを作り、コンパイルしたモジュール(st7789volumio)を配置。

* そのディレクトリに移動し、下記を実行。  
[Usage](#usage)を参考にdisplayとラズパイ接続に合わせて起動パラメータを指定ください。(もしくはソース変更)
```
$ ./st7789volumio
```

※ 起動時の自動実行等は適当にやってください。。。

## Usage
```
Usage: st7789volumio [OPTIONS]  

Options:  
 -s<spi_bus>    SPI bus (0, 1, 2): Default 0  
 -c<cs_pin>     Slave Select pin (0, 1, 2): Default 0  
                    spi = 0, cs = 0...GPIO 8, 1...GPIO 7  
                    spi = 1, cs = 0...GPIO 18, 1...GPIO 17, 2...GIPI16  
                    spi = 2, cs = 0...GPIO 43, 1...GPIO 44, 2...GIPI45  
 -d<pin>        GPIO pin number for DC: Default 25  
 -r<pin>        GPIO pin number for RST: Default 27  
 -b<pin>        GPIO pin number for BLK: Default 24  
 -x<sw>         Audio visualizer ON(1)/OFF(0): Default 0  
 -t<offset>     Vizualizer offset millisec(0-1000): Default 500
                    Effective only as -x1 specified
```

## Acknowledgments
* [NonoPi-NEO](https://github.com/blue777/NanoPi-NEO)
* [st7789 - Rust library for displays using the ST7735 driver](https://github.com/almindor/st7789)
* [ラズパイで動くバイナリプログラムをRustでクロスコンパイルするための基本手順](https://geek.tacoskingdom.com/blog/64)
* [回転すると表示位置がずれる件の解決](https://github.com/zephyrproject-rtos/zephyr/issues/32286#issuecomment-990594099)
* [CAVA - Cross-platform Audio Visualizer](https://github.com/karlstav/cava)
* [Rust library for frequency spectrum analysis using FFT](https://crates.io/crates/spectrum-analyzer)
* [ncmpcpp - NCurses Music Player Client](https://github.com/ncmpcpp/ncmpcpp)

## Author

[Trunkene](https://github.com/Trunkene)
