# headset-controller
An app to configure wireless headphones.

This is meant to be a more lightweight replacement for software like the *SteelSeries Engine*. 

It's also mostly cross-platform.

## Screenshots
![image](https://github.com/sidit77/headset-controller/assets/5053369/e93e96ce-fa3d-4ca0-8a2a-fac36a17c602)

## Why?
My problems with ✨*Gamer*✨ software are perfectly expressed in [this rant by Adam from ShortCircuit](https://www.youtube.com/watch?v=0jxeNPHhalc&t=578s).

> I finally have it downloaded, but every single short circuit we do on gaming headsets that have some sort of (censored) proprietary dumb (censored) software takes like an hour and a half longer than they need to, because we're always fumbling around with this extra bull (censored) that provides the consumer nothing in return, other than what?
> 
> You can make your pretty lights good?
> 
> How hard is that to do?
> 
> How hard is it to make good software
> 
> that makes your lights go good?
> 
> I really don't know.
> 
> I'm not a software developer.
> 
> Maybe it's really, really, really, really, really hard.
> 
> So, maybe I just sound like an (censored) right now.
> 
> But it's just so frustrating that, as a consumer, I have to have this heavyweight thing that wants to know about my games.
> 
> It wants me to launch my games from it.
> 
> I'm not gonna do that.
> 
> I already have Epic Game Store.
> 
> I already have Steam,
> 
> I already have all these other dumb utilities.
> 
> My GPU drivers want to launch my games too.
> 
> Leave me alone.

Well, I'm a CS student and I agree. It shouldn't be that hard to make software that does exactly what it's supposed to do and nothing more.

The Windows build currently produces a single 5 MB executable that runs without installation.

![FiHYR5DXwAUffEt](https://github.com/sidit77/headset-controller/assets/5053369/fe792bd9-cfc7-4b9c-bea2-41248bd1714b)

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

## Supported Devices
* SteelSeries Arctis Nova 7 (X/P)

*It shouldn't be too hard to add support for more devices, but I only own this one headset.*

## Installation

### Prebuilt Binaries

Prebuilt binaries can be found in the [**GitHub Release Section**](https://github.com/sidit77/headset-controller/releases).

Simply download the binary, copy it to your preferred directory, and run it.

### Building Yourself

This app is built using 🦀*Rust*🦀, so you have to install it to build this project on your own.

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
sudo apt install libgtk-3-dev libayatana-appindicator3-dev
```

#### macOS

I don't own a Mac, so I can't test this. It might work or not.

## Todo

- [ ] Panic popup
- [ ] Normal error handling (show notification)
- [x] Device selection
- [ ] handling device disconnects
- [x] Implement the remaining functions for arctis
- [x] log file?
- [x] better system tray
- [ ] improve look of the equalizer
- [ ] more tooltips (language file)
- [x] Linux support

## License
MIT License
