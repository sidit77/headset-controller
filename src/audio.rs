use anyhow::Result;
use windows::core::PCWSTR;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{DEVICE_STATE_ACTIVE, eConsole, eMultimedia, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize, STGM_READ};
use com_policy_config::{IPolicyConfig, PolicyConfigClient};

fn main() -> Result<()>{

    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)?;

        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

        println!("All Devices:");
        let device_collection = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        for i in 0..device_collection.GetCount()? {
            let device = device_collection.Item(i)?;
            let property_store = device.OpenPropertyStore(STGM_READ)?;
            let name = property_store.GetValue(&PKEY_Device_FriendlyName)?;
            println!("  {}", name.Anonymous.Anonymous.Anonymous.pwszVal.display());
        }

        {
            let selected_device = device_collection.Item(0)?;
            let property_store = selected_device.OpenPropertyStore(STGM_READ)?;
            let name = property_store.GetValue(&PKEY_Device_FriendlyName)?;
            println!("Default Device: {}", name.Anonymous.Anonymous.Anonymous.pwszVal.display());

            let device_id = PCWSTR(selected_device.GetId()?.0);
            println!("{}", device_id.display());

            //let policy_config: IPolicyConfig = CoCreateInstance(&PolicyConfigClient, None, CLSCTX_ALL)?;
            //policy_config.SetDefaultEndpoint(&device_id, eConsole)?;
        }

        {
            let default_device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
            let property_store = default_device.OpenPropertyStore(STGM_READ)?;
            let name = property_store.GetValue(&PKEY_Device_FriendlyName)?;
            println!("Default Device: {}", name.Anonymous.Anonymous.Anonymous.pwszVal.display());
        }

        CoUninitialize();
    }


    Ok(())
}