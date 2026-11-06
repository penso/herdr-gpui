//! WinRT's GATT server: the nearby service from a `GattServiceProvider`,
//! advertised connectable and discoverable with only its service UUID, the
//! way the macOS helper advertises. Windows sends each notification to the
//! one subscribed phone it is for. WinRT runs the event handlers on its
//! thread pool; they share [`Core`](crate::peripheral::Core) with the
//! command loop.

use super::{INFO, PAIR, PRESENCE, SERVICE};
use crate::{
    Error, Result,
    peripheral::{self, Backend, Shared},
    protocol::{Channel, Event, Out, Radio},
};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, PoisonError},
};
use windows::{
    Devices::{
        Bluetooth::{
            BluetoothAdapter, BluetoothError,
            GenericAttributeProfile::{
                GattCharacteristicProperties, GattLocalCharacteristic,
                GattLocalCharacteristicParameters, GattLocalService, GattProtectionLevel,
                GattProtocolError, GattReadRequestedEventArgs, GattServiceProvider,
                GattServiceProviderAdvertisingParameters, GattSession, GattWriteOption,
                GattWriteRequestedEventArgs,
            },
        },
        Radios::{Radio as WinRadio, RadioState},
    },
    Foundation::TypedEventHandler,
    Storage::Streams::{DataReader, DataWriter, IBuffer},
    Win32::{
        System::SystemInformation::GetTickCount,
        UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
    },
    core::{GUID, IInspectable, Ref},
};

/// What a notification carries beyond the ATT header.
const ATT_HEADER: u16 = 3;

pub(super) struct Windows {
    provider: GattServiceProvider,
    pair: GattLocalCharacteristic,
    presence: GattLocalCharacteristic,
    advertising: Mutex<bool>,
    /// Kept so its state handler stays registered.
    _radio: WinRadio,
}

impl Windows {
    pub(super) fn start(core: Shared, out: Out) -> Result<Self> {
        let adapter = BluetoothAdapter::GetDefaultAsync()?.join()?;
        if !adapter.IsPeripheralRoleSupported()? {
            out.emit(&Event::State {
                state: Radio::Unsupported,
            });
            return Err(Error::Gatt {
                step: "be a Bluetooth peripheral",
                code: 0,
            });
        }
        let radio = adapter.GetRadioAsync()?.join()?;
        out.emit(&Event::State {
            state: state(radio.State()?),
        });
        let watcher = out.clone();
        radio.StateChanged(&TypedEventHandler::<WinRadio, IInspectable>::new(
            move |radio: Ref<WinRadio>, _| {
                watcher.emit(&Event::State {
                    state: state(radio.ok()?.State()?),
                });
                Ok(())
            },
        ))?;

        let created = GattServiceProvider::CreateAsync(guid(SERVICE))?.join()?;
        let error = created.Error()?;
        if error != BluetoothError::Success {
            return Err(Error::Gatt {
                step: "create the nearby service",
                code: error.0,
            });
        }
        let provider = created.ServiceProvider()?;
        let service = provider.Service()?;
        let info = characteristic(&service, INFO, GattCharacteristicProperties::Read)?;
        let notifying = GattCharacteristicProperties::Write | GattCharacteristicProperties::Notify;
        let pair = characteristic(&service, PAIR, notifying)?;
        let presence = characteristic(&service, PRESENCE, notifying)?;

        let reader = core.clone();
        info.ReadRequested(&TypedEventHandler::<
            GattLocalCharacteristic,
            GattReadRequestedEventArgs,
        >::new(move |_, args| {
            let args = args.ok()?;
            let deferral = args.GetDeferral()?;
            let request = args.GetRequestAsync()?.join()?;
            let info = peripheral::lock(&reader).info();
            let offset = usize::try_from(request.Offset()?).unwrap_or(usize::MAX);
            match info.as_deref().map(|info| info.get(offset..)) {
                Some(Some(rest)) => request.RespondWithValue(&buffer(rest)?)?,
                Some(None) => {
                    request.RespondWithProtocolError(GattProtocolError::InvalidOffset()?)?
                }
                None => request.RespondWithProtocolError(GattProtocolError::ReadNotPermitted()?)?,
            }
            deferral.Complete()
        }))?;
        for (characteristic, channel) in [(&pair, Channel::Pair), (&presence, Channel::Presence)] {
            listen(characteristic, channel, &core, &out)?;
        }
        Ok(Self {
            provider,
            pair,
            presence,
            advertising: Mutex::new(false),
            _radio: radio,
        })
    }

    fn characteristic(&self, channel: Channel) -> &GattLocalCharacteristic {
        match channel {
            Channel::Pair => &self.pair,
            Channel::Presence => &self.presence,
        }
    }
}

impl Backend for Windows {
    fn refresh(&self, advertise: bool) {
        let mut advertising = self
            .advertising
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if advertise == *advertising {
            return;
        }
        let result = if advertise {
            GattServiceProviderAdvertisingParameters::new().and_then(|parameters| {
                parameters.SetIsConnectable(true)?;
                parameters.SetIsDiscoverable(true)?;
                self.provider.StartAdvertisingWithParameters(&parameters)
            })
        } else {
            self.provider.StopAdvertising()
        };
        match result {
            Ok(()) => *advertising = advertise,
            Err(error) => Out.emit(&Event::error(format_args!("advertising: {error}"))),
        }
    }

    fn notify(&self, device: &str, channel: Channel, chunks: Vec<Vec<u8>>) {
        let characteristic = self.characteristic(channel);
        let sent = (|| -> windows::core::Result<()> {
            let Some(client) = characteristic
                .SubscribedClients()?
                .into_iter()
                .find(|client| {
                    client
                        .Session()
                        .and_then(|session| session_device(&session))
                        .is_ok_and(|id| id == device)
                })
            else {
                return Ok(());
            };
            for chunk in chunks {
                characteristic
                    .NotifyValueForSubscribedClientAsync(&buffer(&chunk)?, &client)?
                    .join()?;
            }
            Ok(())
        })();
        if let Err(error) = sent {
            Out.emit(&Event::error(format_args!("notify: {error}")));
        }
    }

    fn idle_seconds(&self) -> Option<u64> {
        idle_seconds()
    }
}

/// A characteristic of the nearby service, readable or writable in plain
/// text: the phone's messages carry their own protection.
fn characteristic(
    service: &GattLocalService,
    uuid: &str,
    properties: GattCharacteristicProperties,
) -> Result<GattLocalCharacteristic> {
    let parameters = GattLocalCharacteristicParameters::new()?;
    parameters.SetCharacteristicProperties(properties)?;
    parameters.SetReadProtectionLevel(GattProtectionLevel::Plain)?;
    parameters.SetWriteProtectionLevel(GattProtectionLevel::Plain)?;
    let created = service
        .CreateCharacteristicAsync(guid(uuid), &parameters)?
        .join()?;
    let error = created.Error()?;
    if error != BluetoothError::Success {
        return Err(Error::Gatt {
            step: "create a characteristic",
            code: error.0,
        });
    }
    Ok(created.Characteristic()?)
}

/// Writes on `characteristic` become `channel`'s frames, and phones that
/// unsubscribe leave it.
fn listen(
    characteristic: &GattLocalCharacteristic,
    channel: Channel,
    core: &Shared,
    out: &Out,
) -> Result<()> {
    let (writer, sender) = (core.clone(), out.clone());
    characteristic.WriteRequested(&TypedEventHandler::<
        GattLocalCharacteristic,
        GattWriteRequestedEventArgs,
    >::new(move |_, args| {
        let args = args.ok()?;
        let deferral = args.GetDeferral()?;
        let session = args.Session()?;
        let device = session_device(&session)?;
        let request = args.GetRequestAsync()?.join()?;
        let value = bytes(&request.Value()?)?;
        let mut core = peripheral::lock(&writer);
        if !core.write_allowed(channel) {
            drop(core);
            request.RespondWithProtocolError(GattProtocolError::WriteNotPermitted()?)?;
            return deferral.Complete();
        }
        core.set_chunk(
            &device,
            usize::from(session.MaxPduSize()?.saturating_sub(ATT_HEADER)),
        );
        let event = core.received(&device, channel, &value);
        drop(core);
        if let Some(event) = event {
            sender.emit(&event);
        }
        if request.Option()? == GattWriteOption::WriteWithResponse {
            request.Respond()?;
        }
        deferral.Complete()
    }))?;
    let subscribed = Arc::new(Mutex::new(HashSet::<String>::new()));
    let (leaver, reporter) = (core.clone(), out.clone());
    characteristic.SubscribedClientsChanged(&TypedEventHandler::<
        GattLocalCharacteristic,
        IInspectable,
    >::new(
        move |characteristic: Ref<GattLocalCharacteristic>, _| {
            let now: HashSet<String> = characteristic
                .ok()?
                .SubscribedClients()?
                .into_iter()
                .filter_map(|client| {
                    client
                        .Session()
                        .and_then(|session| session_device(&session))
                        .ok()
                })
                .collect();
            let mut known = subscribed.lock().unwrap_or_else(PoisonError::into_inner);
            for gone in known.difference(&now) {
                if let Some(event) = peripheral::lock(&leaver).left(gone, channel) {
                    reporter.emit(&event);
                }
            }
            *known = now;
            Ok(())
        },
    ))?;
    Ok(())
}

fn session_device(session: &GattSession) -> windows::core::Result<String> {
    Ok(session.DeviceId()?.Id()?.to_string())
}

fn state(state: RadioState) -> Radio {
    match state {
        RadioState::On => Radio::PoweredOn,
        RadioState::Off | RadioState::Disabled => Radio::PoweredOff,
        _ => Radio::Unknown,
    }
}

/// A UUID string as WinRT's GUID.
fn guid(uuid: &str) -> GUID {
    u128::from_str_radix(&uuid.replace('-', ""), 16).map_or(GUID::zeroed(), GUID::from_u128)
}

fn buffer(bytes: &[u8]) -> windows::core::Result<IBuffer> {
    let writer = DataWriter::new()?;
    writer.WriteBytes(bytes)?;
    writer.DetachBuffer()
}

fn bytes(buffer: &IBuffer) -> windows::core::Result<Vec<u8>> {
    let length = usize::try_from(buffer.Length()?).unwrap_or(0);
    let reader = DataReader::FromBuffer(buffer)?;
    let mut bytes = vec![0; length];
    reader.ReadBytes(&mut bytes)?;
    Ok(bytes)
}

/// Seconds since the session's last keyboard or mouse input.
///
/// Win32 offers no safe wrapper for the last input time, and the
/// `windows` crate marks both calls `unsafe`.
#[allow(unsafe_code)]
fn idle_seconds() -> Option<u64> {
    let mut info = LASTINPUTINFO {
        cbSize: u32::try_from(size_of::<LASTINPUTINFO>()).ok()?,
        dwTime: 0,
    };
    // SAFETY: `info` is a live, initialised LASTINPUTINFO whose `cbSize`
    // is its own size, as GetLastInputInfo requires; the call only writes
    // `dwTime` through the pointer, which outlives the call.
    if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        return None;
    }
    // SAFETY: GetTickCount takes no arguments and only reads the system's
    // tick count.
    let now = unsafe { GetTickCount() };
    // Both counts wrap after 49.7 days; the difference does not.
    Some(u64::from(now.wrapping_sub(info.dwTime)) / 1000)
}
