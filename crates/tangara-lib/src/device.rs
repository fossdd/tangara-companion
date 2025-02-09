pub mod connection;
pub mod info;

use std::sync::Arc;

use serialport::{SerialPortInfo, UsbPortInfo, SerialPortType};
use thiserror::Error;

use crate::{flash::{Flash, FlashTask, self}, firmware};

pub use connection::Connection;

const USB_VID: u16 = 4617; // cool tech zone
const USB_PID: u16 = 8212; // Tangara

#[derive(Clone, Debug)]
pub struct Tangara {
    connection: Connection,
    params: Arc<ConnectionParams>,
}

#[derive(Clone, Debug)]
pub struct ConnectionParams {
    pub serial: SerialPortInfo,
    pub usb: UsbPortInfo,
}

#[derive(Debug, Error)]
pub enum FindTangaraError {
    #[error("Error enumerating serial ports: {0}")]
    Port(#[from] serialport::Error),
    #[error("Can't find Tangara, make sure it's plugged in and turned on")]
    NoTangara,
}

impl Tangara {
    pub async fn open(params: &ConnectionParams)
        -> Result<Tangara, connection::OpenError>
    {
        let connection = Connection::open(&params.serial).await?;

        Ok(Tangara {
            connection,
            params: Arc::new(params.clone()),
        })
    }

    pub fn serial_port_name(&self) -> &str {
        &self.params.serial.port_name
    }

    pub fn serial_port(&self) -> &SerialPortInfo {
        &self.params.serial
    }

    pub fn usb_port(&self) -> &UsbPortInfo {
        &self.params.usb
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn find() -> Result<ConnectionParams, FindTangaraError> {
        match find_serialport() {
            Ok(Some(params)) => { return Ok(params); }
            Ok(None) => {}
            Err(error) => {
                log::error!("error enumerating serial ports: {error}");
            }
        }

        #[cfg(target_os = "linux")]
        match find_devtmpfs() {
            Ok(Some(params)) => { return Ok(params); }
            Ok(None) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                log::error!("error enumerating /dev/serial/by-id: {error}");
            }
        }

        Err(FindTangaraError::NoTangara)
    }

    pub async fn setup_flash(self, firmware: Arc<firmware::Firmware>) -> (Flash, FlashTask) {
        let params = self.params.clone();

        // disconnect before trying to  reopen the port for flash
        self.connection.disconnect().await;

        flash::setup(params, firmware)
    }
}


/// Finds a Tangara using the serialport crate. Cross platform, but
/// doesn't work under Flatpak as it relies on udev and Flatpak does
/// not have great udev support.
fn find_serialport() -> Result<Option<ConnectionParams>, serialport::Error> {
    for port in serialport::available_ports()? {
        if let SerialPortType::UsbPort(usb) = &port.port_type {
            if usb.vid == USB_VID && usb.pid == USB_PID {
                return Ok(Some(ConnectionParams {
                    serial: port.clone(),
                    usb: usb.clone(),
                }));
            }
        }
    }

    Ok(None)
}

/// Fallback for when we're running under Flatpak
#[cfg(target_os = "linux")]
fn find_devtmpfs() -> Result<Option<ConnectionParams>, std::io::Error> {
    for entry in std::fs::read_dir("/dev/serial/by-id")? {
        let entry = entry?;
        let name = entry.file_name();

        let Some(name) = name.to_str() else {
            continue;
        };

        if !name.starts_with("usb-cool_tech_zone_Tangara_") {
            continue;
        }

        let path = entry.path().canonicalize()?;

        let Some(path) = path.to_str().map(str::to_owned) else {
            continue;
        };

        let usb_info = UsbPortInfo {
            vid: USB_VID,
            pid: USB_PID,
            serial_number: None,
            manufacturer: None,
            product: None,
        };

        return Ok(Some(ConnectionParams {
            serial: SerialPortInfo {
                port_name: path,
                port_type: SerialPortType::UsbPort(usb_info.clone()),
            },
            usb: usb_info,
        }));
    }

    Ok(None)
}
