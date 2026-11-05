//! What a machine boots from: its files, and the configuration they make.
//!
//! `objc2-virtualization` marks every call `unsafe`, as it does not check the
//! framework's threading rules. These only build configuration objects.
#![allow(unsafe_code)]

use super::{
    Display, Error, Name, Result, Settings,
    store::{self, AUXILIARY, DISK, HARDWARE, IDENTITY},
    vm::description,
};
use objc2::{AnyThread as _, rc::Retained};
use objc2_foundation::{NSArray, NSData, NSString, NSURL};
use objc2_virtualization::{
    VZDiskImageStorageDeviceAttachment, VZMACAddress, VZMacAuxiliaryStorage,
    VZMacGraphicsDeviceConfiguration, VZMacGraphicsDisplayConfiguration, VZMacHardwareModel,
    VZMacMachineIdentifier, VZMacOSBootLoader, VZMacPlatformConfiguration,
    VZNATNetworkDeviceAttachment, VZUSBKeyboardConfiguration,
    VZUSBScreenCoordinatePointingDeviceConfiguration, VZVirtioBlockDeviceConfiguration,
    VZVirtioEntropyDeviceConfiguration, VZVirtioNetworkDeviceConfiguration,
    VZVirtualMachineConfiguration,
};
use sha2::{Digest as _, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// A display that follows its tab starts at this size.
const FIT_WIDTH: u16 = 2560;
const FIT_HEIGHT: u16 = 1600;
/// A 2x Retina mode, so text stays sharp.
const DISPLAY_PPI: isize = 220;

/// The machine's network address, the same at each launch: left to itself the
/// framework picks a new one, and a saved state then refuses to restore. It
/// comes from the machine's identity, so it needs no file of its own. Marked
/// locally administered and unicast, as an address no vendor owns must be.
fn mac_address(identity: &[u8]) -> String {
    let digest = Sha256::digest(identity);
    let first = (digest[0] | 0b10) & !0b1;
    format!(
        "{first:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        digest[1], digest[2], digest[3], digest[4], digest[5]
    ) // i18n: data
}

/// A machine's files: the small ones read, the large ones found.
pub(super) struct Bundle {
    hardware: Vec<u8>,
    identity: Vec<u8>,
    auxiliary: PathBuf,
    disk: PathBuf,
    pub(super) settings: Settings,
    /// The saved state to resume from, if it is suspended.
    pub(super) saved: Option<PathBuf>,
}

impl Bundle {
    /// Reads machine `name`'s folder. It reaches the disk, so it runs off the
    /// main thread.
    pub(super) fn read(name: &Name) -> Result<Self> {
        let folder = store::folder(name)?;
        let auxiliary = folder.join(AUXILIARY);
        let disk = folder.join(DISK);
        if !auxiliary.is_file() || !disk.is_file() {
            return Err(Error::NotInstalled(folder));
        }
        let saved = Some(store::state_file(name)?).filter(|state| state.is_file());
        Ok(Self {
            hardware: read(&folder.join(HARDWARE))?,
            identity: read(&folder.join(IDENTITY))?,
            auxiliary,
            disk,
            settings: store::boot_settings(&folder)?,
            saved,
        })
    }

    pub(super) fn configuration(&self) -> Result<Retained<VZVirtualMachineConfiguration>> {
        let hardware = NSData::with_bytes(&self.hardware);
        let identity = NSData::with_bytes(&self.identity);
        let auxiliary = file_url(&self.auxiliary);
        let disk = file_url(&self.disk);
        let (width, height) = match self.settings.display {
            Display::Fit => (FIT_WIDTH, FIT_HEIGHT),
            Display::Fixed { width, height } => (width, height),
        };
        // SAFETY: plain configuration objects, built and used on this thread.
        // Each init is given the object its class requires.
        unsafe {
            let hardware = VZMacHardwareModel::initWithDataRepresentation(
                VZMacHardwareModel::alloc(),
                &hardware,
            )
            .filter(|model| model.isSupported())
            .ok_or(Error::Hardware)?;
            let identity = VZMacMachineIdentifier::initWithDataRepresentation(
                VZMacMachineIdentifier::alloc(),
                &identity,
            )
            .ok_or(Error::Hardware)?;
            let platform = VZMacPlatformConfiguration::new();
            platform.setHardwareModel(&hardware);
            platform.setMachineIdentifier(&identity);
            let auxiliary =
                VZMacAuxiliaryStorage::initWithURL(VZMacAuxiliaryStorage::alloc(), &auxiliary);
            platform.setAuxiliaryStorage(Some(&auxiliary));
            let attachment = VZDiskImageStorageDeviceAttachment::initWithURL_readOnly_error(
                VZDiskImageStorageDeviceAttachment::alloc(),
                &disk,
                false,
            )
            .map_err(|error| Error::Framework(description(&error)))?;
            let block = VZVirtioBlockDeviceConfiguration::initWithAttachment(
                VZVirtioBlockDeviceConfiguration::alloc(),
                &attachment,
            );
            let display = VZMacGraphicsDisplayConfiguration::initWithWidthInPixels_heightInPixels_pixelsPerInch(
                VZMacGraphicsDisplayConfiguration::alloc(),
                isize::try_from(width).unwrap_or(isize::MAX),
                isize::try_from(height).unwrap_or(isize::MAX),
                DISPLAY_PPI,
            );
            let graphics = VZMacGraphicsDeviceConfiguration::new();
            graphics.setDisplays(&NSArray::from_retained_slice(&[display]));
            let network = VZVirtioNetworkDeviceConfiguration::new();
            let address = NSString::from_str(&mac_address(&self.identity));
            let address = VZMACAddress::initWithString(VZMACAddress::alloc(), &address)
                .ok_or(Error::Hardware)?;
            network.setMACAddress(&address);
            let nat = VZNATNetworkDeviceAttachment::new();
            network.setAttachment(Some(&nat));

            let configuration = VZVirtualMachineConfiguration::new();
            let boot = VZMacOSBootLoader::new();
            configuration.setBootLoader(Some(&boot));
            configuration.setCPUCount(self.settings.cpus.into());
            configuration.setMemorySize(u64::from(self.settings.memory_gib) << 30);
            configuration.setPlatform(&platform);
            configuration.setStorageDevices(&NSArray::from_retained_slice(&[
                Retained::into_super(block),
            ]));
            configuration.setGraphicsDevices(&NSArray::from_retained_slice(&[
                Retained::into_super(graphics),
            ]));
            configuration.setKeyboards(&NSArray::from_retained_slice(&[Retained::into_super(
                VZUSBKeyboardConfiguration::new(),
            )]));
            configuration.setPointingDevices(&NSArray::from_retained_slice(&[
                Retained::into_super(VZUSBScreenCoordinatePointingDeviceConfiguration::new()),
            ]));
            configuration.setEntropyDevices(&NSArray::from_retained_slice(&[
                Retained::into_super(VZVirtioEntropyDeviceConfiguration::new()),
            ]));
            configuration.setNetworkDevices(&NSArray::from_retained_slice(&[
                Retained::into_super(network),
            ]));
            configuration
                .validateWithError()
                .map_err(|error| Error::Framework(description(&error)))?;
            configuration
                .validateSaveRestoreSupportWithError()
                .map_err(|error| Error::Framework(description(&error)))?;
            Ok(configuration)
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => {
            Error::NotInstalled(path.parent().unwrap_or(path).to_owned())
        }
        _ => store::read_error(path, source),
    })
}

pub(super) fn file_url(path: &Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

#[cfg(test)]
mod tests;
