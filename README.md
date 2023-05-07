# headset-controller
An app to configure wireless headphones.
This is meant to be a more lightweight replacement for software like the *SteelSeries Engine*. The Windows build currently produces a single ~4mb executable that runs without installation.

It's also mostly cross-platform.

## Features
* Read Battery Status
* Read Chat Mix (currently purely visual)
* Modify Equalizer
* Modify Side Tone
* Modify Microphone Volume
* Toggle Volume Limiter
* Modify Inactive Time
* Modify Mute Light
* Toggle Auto Enable Bluetooth
* Change Call Action
* Automatically switch audio when the headset connects (windows only)

## Screenshots
![image](https://user-images.githubusercontent.com/5053369/222571854-e99f5230-6417-4330-a4b5-110464803aed.png)

## Supported Devices
* SteelSeries Arctis Nova 7 (X/P)

*It shouldn't be too hard to add support for more devices, but I only own this one headset.*

## Installation

### Prebuilt Binaries

Prebuilt binaries can be found in the [**GitHub Release Section**](https://github.com/sidit77/headset-controller/releases).

Simply download the binary, copy it to your preferred directory, and run it.

### Building Yourself

This app is build using 🦀*Rust*🦀, so you have to install it to build this project on your own.

After that, simply clone the source and build it:
```bash
git clone https://github.com/sidit77/headset-controller.git
cd headset-controller
cargo build --release
```

#### Windows

On Windows, this program can be configured to use DirectX 11 for rendering instead of OpenGL. To build the DirectX version, run this command instead.
```bash
cargo build --release --no-default-features --features directx
```

#### Linux

On Linux, some additional packages are required.

```bash
sudo apt install libgtk-3-dev libayatana-appindicator3-dev libudev-dev
```

#### macOS

I don't own a Mac, so I can't test this. It might work or not.

## Todo

- [ ] Panic popup
- [ ] Normal error handling (show notification)
- [x] Devices selection
- [ ] handling device disconnects
- [ ] Implement the remaining functions for arctis
- [x] log file?
- [x] better system tray
- [ ] improve look of the equalizer
- [ ] more tooltips (language file)
- [x] linux support

## License
MIT License