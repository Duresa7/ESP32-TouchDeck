use super::*;
use ::windows::{
    Win32::{
        Foundation::PROPERTYKEY,
        Media::Audio::{
            AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, EDataFlow, ERole,
            Endpoints::{
                IAudioEndpointVolume, IAudioEndpointVolumeCallback,
                IAudioEndpointVolumeCallback_Impl,
            },
            IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
            MMDeviceEnumerator, eMultimedia, eRender,
        },
        System::Com::{
            CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
        },
    },
    core::{GUID, PCWSTR, Result as WinResult, implement},
};

const CONTEXT: GUID = GUID::from_u128(0x0c9cdfaa_64b2_4795_86e3_9b776eeed094);

struct Apartment;
impl Apartment {
    fn new() -> Result<Self, Failure> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() }
            .map_err(|_| Failure::Unavailable)?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback(Notifier);
impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, _: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> WinResult<()> {
        self.0.changed();
        Ok(())
    }
}
#[implement(IMMNotificationClient)]
struct DeviceCallback(Notifier);
impl IMMNotificationClient_Impl for DeviceCallback_Impl {
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _: &PCWSTR) -> WinResult<()> {
        if flow == eRender && role == eMultimedia {
            self.0.endpoint_changed();
        }
        Ok(())
    }
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> WinResult<()> {
        self.0.endpoint_changed();
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> WinResult<()> {
        self.0.endpoint_changed();
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> WinResult<()> {
        self.0.endpoint_changed();
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> WinResult<()> {
        Ok(())
    }
}

struct Endpoint {
    volume: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        let _ = unsafe { self.volume.UnregisterControlChangeNotify(&self.callback) };
    }
}
struct CoreAudio {
    endpoint: Option<Endpoint>,
    enumerator: IMMDeviceEnumerator,
    callback: IMMNotificationClient,
    notifier: Notifier,
    _apartment: Apartment,
}
pub(super) fn open(notifier: Notifier) -> Result<Box<dyn VolumeProvider>, Failure> {
    let apartment = Apartment::new()?;
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .map_err(|_| Failure::Unavailable)?;
    let callback: IMMNotificationClient = DeviceCallback(notifier.clone()).into();
    unsafe { enumerator.RegisterEndpointNotificationCallback(&callback) }
        .map_err(|_| Failure::Unavailable)?;
    Ok(Box::new(CoreAudio {
        endpoint: None,
        enumerator,
        callback,
        notifier,
        _apartment: apartment,
    }))
}
impl CoreAudio {
    fn bind(&mut self) -> Result<(), Failure> {
        self.endpoint.take();
        let device = unsafe {
            self.enumerator
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
        }
        .map_err(|_| Failure::NoDevice)?;
        let volume: IAudioEndpointVolume =
            unsafe { device.Activate(CLSCTX_ALL, None) }.map_err(|_| Failure::Unavailable)?;
        let callback: IAudioEndpointVolumeCallback = VolumeCallback(self.notifier.clone()).into();
        unsafe { volume.RegisterControlChangeNotify(&callback) }
            .map_err(|_| Failure::Unavailable)?;
        self.endpoint = Some(Endpoint { volume, callback });
        Ok(())
    }
    fn endpoint(&mut self) -> Result<&IAudioEndpointVolume, Failure> {
        if self.endpoint.is_none() {
            self.bind()?;
        }
        self.endpoint
            .as_ref()
            .map(|endpoint| &endpoint.volume)
            .ok_or(Failure::NoDevice)
    }
    fn recover<T>(&mut self, result: WinResult<T>) -> Result<T, Failure> {
        result.map_err(|_| {
            self.endpoint.take();
            self.notifier.endpoint_changed();
            Failure::Unavailable
        })
    }
}
impl VolumeProvider for CoreAudio {
    fn read(&mut self, rebind: bool) -> Result<Snapshot, Failure> {
        if rebind {
            self.bind()?;
        }
        let endpoint = self.endpoint()?;
        let result = unsafe {
            endpoint.GetMasterVolumeLevelScalar().and_then(|level| {
                endpoint.GetMute().map(|muted| Snapshot {
                    level: f64::from(level),
                    muted: muted.as_bool(),
                })
            })
        };
        self.recover(result)
    }
    fn set_level(&mut self, level: f64) -> Result<(), Failure> {
        let result = unsafe {
            self.endpoint()?
                .SetMasterVolumeLevelScalar(level as f32, &CONTEXT)
        };
        self.recover(result)
    }
    fn set_mute(&mut self, muted: bool) -> Result<(), Failure> {
        let result = unsafe { self.endpoint()?.SetMute(muted, &CONTEXT) };
        self.recover(result)
    }
}
impl Drop for CoreAudio {
    fn drop(&mut self) {
        self.endpoint.take();
        let _ = unsafe {
            self.enumerator
                .UnregisterEndpointNotificationCallback(&self.callback)
        };
    }
}
