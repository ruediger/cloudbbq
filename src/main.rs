#![allow(dead_code)]  // TODO: remove

mod bluetooth;
use bluetooth::{BluetoothSession, DeviceInfo};
use dbus::arg::RefArg;
use dbus::nonblock::stdintf::org_freedesktop_dbus::ObjectManager;
use dbus::nonblock::Proxy;
use thiserror::Error;
use tokio::time;
use std::collections::HashMap;
use std::time::Duration;
mod uuid;
use uuid::Uuid128;

const SCAN_DURATION: Duration = Duration::from_secs(5);

// https://gist.github.com/uucidl/b9c60b6d36d8080d085a8e3310621d64
const BBQ_SERVICE_UUID : Uuid128 = uuid::uuid16_to_uuid128(0xFFF0);
const ACCOUNT_AND_VERIFY_UUID : Uuid128 = uuid::uuid16_to_uuid128(0xFFF2);

const CREDENTIAL_MSG : [u8; 15] = [0x21, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0xb8, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00];


#[derive(Debug, Error)]
pub enum Error {
    #[error("No device was found")]
    NoDeviceFound,
}

pub async fn find_device(bt_session: &BluetoothSession, name: String)
                         -> Result<DeviceInfo, Box<dyn std::error::Error>> {
    bt_session.start_discovery().await?;
    time::delay_for(SCAN_DURATION).await;

    let devices = bt_session.get_devices().await?;
    for device in devices.into_iter() {
        if device.name.as_ref() == Some(&name) {
            return Ok(device);
        }
    }
    Err(Box::new(Error::NoDeviceFound))
}

#[derive(Debug)]
pub struct Characteristic<'a> {
    uuid: Uuid128,
    service_path: dbus::Path<'a>,
    path: dbus::Path<'a>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (_, bt_session) = BluetoothSession::new().await?;
    let device_info = find_device(&bt_session, "BBQ".to_string()).await?;
    println!("FOUND: {:?}", device_info);
    bt_session.connect(&device_info.id).await?;
    time::delay_for(SCAN_DURATION).await;

    let bluez_root = Proxy::new(
        "org.bluez",
        "/",
        std::time::Duration::from_secs(30),
        bt_session.connection.clone(),
    );
    let tree = bluez_root.get_managed_objects().await?;
    let mut services : HashMap<Uuid128, dbus::Path> = HashMap::new();
    let mut services_to_uuid : HashMap<dbus::Path, Uuid128> = HashMap::new();
    let mut characteristics : HashMap<Uuid128, Characteristic> = HashMap::new();
    for (path, interface) in tree.into_iter() {
        if path.starts_with(&device_info.id.object_path) {
            if let Some(service) = interface.get("org.bluez.GattService1") {
                if let Some(uuid) = service.get("UUID").and_then(|v| v.as_str()).and_then(|s| uuid::string_to_uuid128(s.to_string()).ok()) {
                    services.insert(uuid, dbus::Path::new(path.to_string()).unwrap());
                    services_to_uuid.insert(path, uuid);
                }
            } else if let Some(charactersitic) = interface.get("org.bluez.GattCharacteristic1") {
                if let Some(uuid) = charactersitic.get("UUID").and_then(|v| v.as_str()).and_then(|s| uuid::string_to_uuid128(s.to_string()).ok()) {
                    if let Some(service_path) = charactersitic.get("Service").and_then(|v| v.as_str()).and_then(|s| dbus::Path::from_slice(s).ok()) {
                        characteristics.insert(
                            uuid,
                            Characteristic {
                                uuid: uuid,
                                service_path: dbus::Path::new(service_path.to_string()).unwrap(),
                                path: path,
                            });
                    }
                }
            }
        }
    }

    // println!("SERVICES {:?}", services);
    // println!("CHARACTERISTICS {:?}", characteristics);

    // time::delay_for(SCAN_DURATION).await;

    if let Some(_service_path) = services.get(&BBQ_SERVICE_UUID) {
        if let Some(characteristic) = characteristics.get(&ACCOUNT_AND_VERIFY_UUID) {
            println!("PATH: {}", characteristic.path);
            bt_session.write_characteristic_value(
                &device_info.id,
                &characteristic.path,
                CREDENTIAL_MSG).await?;
        } else {
            println!("No Char!");
        }
    } else {
        println!("No Service!");
    }
    Ok(())
}
