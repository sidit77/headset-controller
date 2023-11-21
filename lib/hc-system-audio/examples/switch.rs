use hc_system_audio::AudioManager;

fn main() {
    let manager = AudioManager::new().unwrap();
    for device in manager.devices() {
        println!("{}", device.name());
    }
}