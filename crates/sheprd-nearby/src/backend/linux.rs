//! BlueZ over D-Bus: the nearby service as a GATT application object tree
//! registered with `GattManager1`, advertised with an `LEAdvertisement1`
//! that carries only the service UUID, the way the macOS helper
//! advertises. zbus runs the objects on its own executor, never Tokio.
//!
//! BlueZ sends a characteristic's notifications to every phone subscribed
//! to it, with no way to pick one. With two phones exchanging at once, one
//! may read the other's frames; the phone then retries, as it does when a
//! frame goes astray.

use super::{INFO, PAIR, PRESENCE, SERVICE};
use crate::{
    Result,
    peripheral::{self, Backend, Shared},
    protocol::{Channel, Event, Out, Radio},
};
use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
    thread,
};
use zbus::{
    DBusError, MatchRule, blocking,
    message::Type,
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value},
};

const BLUEZ: &str = "org.bluez";
const APP: &str = "/org/herdr/nearby";
const SERVICE_PATH: &str = "/org/herdr/nearby/service0";
const INFO_PATH: &str = "/org/herdr/nearby/service0/info";
const PAIR_PATH: &str = "/org/herdr/nearby/service0/pair";
const PRESENCE_PATH: &str = "/org/herdr/nearby/service0/presence";
const ADVERT_PATH: &str = "/org/herdr/nearby/advert0";
const CHARACTERISTIC: &str = "org.bluez.GattCharacteristic1";
/// What a notification carries beyond the ATT header.
const ATT_HEADER: usize = 3;

type Properties = HashMap<String, OwnedValue>;
type Managed = HashMap<OwnedObjectPath, HashMap<String, Properties>>;

pub(super) struct Linux {
    conn: blocking::Connection,
    adapter: OwnedObjectPath,
    /// Whether the advertisement is registered with BlueZ.
    advertising: Mutex<bool>,
}

impl Linux {
    /// Serves the GATT objects and registers them with the first adapter
    /// that can be a peripheral, then watches it and the phones on it.
    pub(super) fn start(core: Shared, out: Out) -> Result<Self> {
        let conn = blocking::Connection::system()?;
        let managed: Managed = conn
            .call_method(
                Some(BLUEZ),
                "/",
                Some("org.freedesktop.DBus.ObjectManager"),
                "GetManagedObjects",
                &(),
            )?
            .body()
            .deserialize()?;
        let Some((adapter, interfaces)) = managed.into_iter().find(|(_, interfaces)| {
            interfaces.contains_key("org.bluez.GattManager1")
                && interfaces.contains_key("org.bluez.LEAdvertisingManager1")
        }) else {
            out.emit(&Event::State {
                state: Radio::Unsupported,
            });
            return Err(crate::Error::Gatt {
                step: "find an adapter that can be a peripheral",
                code: 0,
            });
        };
        let powered = interfaces
            .get("org.bluez.Adapter1")
            .and_then(|adapter| adapter.get("Powered"))
            .and_then(|value| bool::try_from(value).ok());
        out.emit(&Event::State {
            state: match powered {
                Some(true) => Radio::PoweredOn,
                Some(false) => Radio::PoweredOff,
                None => Radio::Unknown,
            },
        });

        let server = conn.object_server();
        server.at(APP, zbus::fdo::ObjectManager)?;
        server.at(SERVICE_PATH, Service)?;
        let characteristic = |uuid, flags: &[&str], channel| Characteristic {
            uuid,
            flags: flags.iter().map(|flag| (*flag).to_owned()).collect(),
            channel,
            core: core.clone(),
            out: out.clone(),
            notifying: false,
        };
        server.at(INFO_PATH, characteristic(INFO, &["read"], None))?;
        server.at(
            PAIR_PATH,
            characteristic(PAIR, &["write", "notify"], Some(Channel::Pair)),
        )?;
        server.at(
            PRESENCE_PATH,
            characteristic(PRESENCE, &["write", "notify"], Some(Channel::Presence)),
        )?;
        server.at(ADVERT_PATH, Advertisement)?;
        drop(server);

        conn.call_method(
            Some(BLUEZ),
            adapter.as_str(),
            Some("org.bluez.GattManager1"),
            "RegisterApplication",
            &(
                ObjectPath::try_from(APP).map_err(zbus::Error::from)?,
                HashMap::<&str, Value<'_>>::new(),
            ),
        )?;
        watch(&conn, &adapter, core, out)?;
        Ok(Self {
            conn,
            adapter,
            advertising: Mutex::new(false),
        })
    }

    /// Registers or unregisters the advertisement with the adapter.
    fn call(&self, method: &str, register: bool) -> zbus::Result<()> {
        let advert = ObjectPath::try_from(ADVERT_PATH)?;
        let manager = Some("org.bluez.LEAdvertisingManager1");
        if register {
            self.conn.call_method(
                Some(BLUEZ),
                self.adapter.as_str(),
                manager,
                method,
                &(advert, HashMap::<&str, Value<'_>>::new()),
            )?;
        } else {
            self.conn.call_method(
                Some(BLUEZ),
                self.adapter.as_str(),
                manager,
                method,
                &(advert,),
            )?;
        }
        Ok(())
    }
}

impl Backend for Linux {
    fn refresh(&self, advertise: bool) {
        let mut advertising = self
            .advertising
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if advertise == *advertising {
            return;
        }
        let result = if advertise {
            self.call("RegisterAdvertisement", true)
        } else {
            self.call("UnregisterAdvertisement", false)
        };
        match result {
            Ok(()) => *advertising = advertise,
            Err(error) => Out.emit(&Event::error(format_args!("advertising: {error}"))),
        }
    }

    fn notify(&self, _device: &str, channel: Channel, chunks: Vec<Vec<u8>>) {
        let path = match channel {
            Channel::Pair => PAIR_PATH,
            Channel::Presence => PRESENCE_PATH,
        };
        for chunk in chunks {
            let changed = HashMap::from([("Value", Value::from(chunk))]);
            let sent = self.conn.emit_signal(
                None::<&str>,
                path,
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &(CHARACTERISTIC, changed, Vec::<&str>::new()),
            );
            if let Err(error) = sent {
                Out.emit(&Event::error(format_args!("notify: {error}")));
                return;
            }
        }
    }

    fn idle_seconds(&self) -> Option<u64> {
        idle_seconds()
    }
}

/// Follows the adapter's power and phones leaving, from BlueZ's
/// `PropertiesChanged` signals, on a thread of its own.
fn watch(
    conn: &blocking::Connection,
    adapter: &OwnedObjectPath,
    core: Shared,
    out: Out,
) -> Result<()> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(BLUEZ)?
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .build();
    let signals = blocking::MessageIterator::for_match_rule(rule, conn, Some(64))?;
    let adapter = adapter.clone();
    thread::spawn(move || {
        for message in signals {
            let Ok(message) = message else { continue };
            let header = message.header();
            let Some(path) = header.path() else { continue };
            let Ok((interface, changed, _)) = message
                .body()
                .deserialize::<(String, Properties, Vec<String>)>()
            else {
                continue;
            };
            let flag = |name: &str| {
                changed
                    .get(name)
                    .and_then(|value| bool::try_from(value).ok())
            };
            match interface.as_str() {
                "org.bluez.Adapter1" if path.as_str() == adapter.as_str() => {
                    if let Some(powered) = flag("Powered") {
                        out.emit(&Event::State {
                            state: if powered {
                                Radio::PoweredOn
                            } else {
                                Radio::PoweredOff
                            },
                        });
                    }
                }
                "org.bluez.Device1" if flag("Connected") == Some(false) => {
                    let mut core = peripheral::lock(&core);
                    let _ = core.left(path.as_str(), Channel::Pair);
                    if let Some(event) = core.left(path.as_str(), Channel::Presence) {
                        drop(core);
                        out.emit(&event);
                    }
                }
                _ => {}
            }
        }
    });
    Ok(())
}

/// Seconds since the session's last input, from GNOME's idle monitor or
/// the freedesktop screen saver, whichever answers on the session bus.
fn idle_seconds() -> Option<u64> {
    let session = blocking::Connection::session().ok()?;
    let mutter = session
        .call_method(
            Some("org.gnome.Mutter.IdleMonitor"),
            "/org/gnome/Mutter/IdleMonitor/Core",
            Some("org.gnome.Mutter.IdleMonitor"),
            "GetIdletime",
            &(),
        )
        .ok()
        .and_then(|reply| reply.body().deserialize::<u64>().ok());
    let milliseconds = mutter.or_else(|| {
        session
            .call_method(
                Some("org.freedesktop.ScreenSaver"),
                "/org/freedesktop/ScreenSaver",
                Some("org.freedesktop.ScreenSaver"),
                "GetSessionIdleTime",
                &(),
            )
            .ok()
            .and_then(|reply| reply.body().deserialize::<u32>().ok())
            .map(u64::from)
    })?;
    Some(milliseconds / 1000)
}

/// What BlueZ answers a phone with when a read or write is refused.
#[derive(Debug, DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum Refusal {
    #[zbus(error)]
    ZBus(zbus::Error),
    NotPermitted(String),
    InvalidOffset(String),
}

struct Service;

#[zbus::interface(name = "org.bluez.GattService1")]
impl Service {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> String {
        SERVICE.to_owned()
    }

    #[zbus(property)]
    fn primary(&self) -> bool {
        true
    }
}

struct Characteristic {
    uuid: &'static str,
    flags: Vec<String>,
    /// `None` for `info`, which is read only.
    channel: Option<Channel>,
    core: Shared,
    out: Out,
    notifying: bool,
}

#[zbus::interface(name = "org.bluez.GattCharacteristic1")]
impl Characteristic {
    fn read_value(&self, options: Properties) -> std::result::Result<Vec<u8>, Refusal> {
        let info = peripheral::lock(&self.core).info();
        let (Some(info), None) = (info, self.channel) else {
            return Err(Refusal::NotPermitted("not readable now".to_owned()));
        };
        let offset = options
            .get("offset")
            .and_then(|value| u16::try_from(value).ok())
            .map_or(0, usize::from);
        info.get(offset..)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| Refusal::InvalidOffset("past the end".to_owned()))
    }

    fn write_value(&self, value: Vec<u8>, options: Properties) -> std::result::Result<(), Refusal> {
        let Some(channel) = self.channel else {
            return Err(Refusal::NotPermitted("read only".to_owned()));
        };
        let Some(device) = options
            .get("device")
            .and_then(|value| OwnedObjectPath::try_from(value.clone()).ok())
        else {
            return Err(Refusal::NotPermitted("no device".to_owned()));
        };
        let mut core = peripheral::lock(&self.core);
        if !core.write_allowed(channel) {
            return Err(Refusal::NotPermitted("not accepting writes now".to_owned()));
        }
        if let Some(mtu) = options
            .get("mtu")
            .and_then(|value| u16::try_from(value).ok())
        {
            core.set_chunk(device.as_str(), usize::from(mtu).saturating_sub(ATT_HEADER));
        }
        let event = core.received(device.as_str(), channel, &value);
        drop(core);
        if let Some(event) = event {
            self.out.emit(&event);
        }
        Ok(())
    }

    fn start_notify(&mut self) {
        self.notifying = true;
    }

    fn stop_notify(&mut self) {
        self.notifying = false;
    }

    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> String {
        self.uuid.to_owned()
    }

    #[zbus(property)]
    fn service(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(SERVICE_PATH).unwrap_or_default()
    }

    #[zbus(property)]
    fn flags(&self) -> Vec<String> {
        self.flags.clone()
    }

    #[zbus(property)]
    fn notifying(&self) -> bool {
        self.notifying
    }
}

/// The advertisement: only the service UUID, which iOS needs to find the
/// computer in the background; the name stays in `info`, and only while
/// pairing.
struct Advertisement;

#[zbus::interface(name = "org.bluez.LEAdvertisement1")]
impl Advertisement {
    fn release(&self) {}

    #[zbus(property, name = "Type")]
    fn kind(&self) -> String {
        "peripheral".to_owned()
    }

    #[zbus(property, name = "ServiceUUIDs")]
    fn service_uuids(&self) -> Vec<String> {
        vec![SERVICE.to_owned()]
    }
}
