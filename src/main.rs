#![allow(dead_code)]  // TODO: remove

use bluez_async::{uuid_from_u16, BluetoothSession, DeviceInfo};
use std::time::Duration;
use thiserror::Error;
use tokio::time;
use uuid::Uuid;

const SCAN_DURATION: Duration = Duration::from_secs(5);

// https://gist.github.com/uucidl/b9c60b6d36d8080d085a8e3310621d64
const BBQ_SERVICE_UUID: Uuid = uuid_from_u16(0xFFF0);
const ACCOUNT_AND_VERIFY_UUID: Uuid = uuid_from_u16(0xFFF2);

const CREDENTIAL_MSG : [u8; 15] = [0x21, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0xb8, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00];


#[derive(Debug, Error)]
pub enum Error {
    #[error("No device was found")]
    NoDeviceFound,
}

pub async fn find_device(bt_session: &BluetoothSession, name: String)
                         -> Result<DeviceInfo, Box<dyn std::error::Error>> {
    bt_session.start_discovery().await?;
    time::sleep(SCAN_DURATION).await;

    let devices = bt_session.get_devices().await?;
    for device in devices.into_iter() {
        if device.name.as_ref() == Some(&name) {
            return Ok(device);
        }
    }
    Err(Box::new(Error::NoDeviceFound))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (_, bt_session) = BluetoothSession::new().await?;
    let device_info = find_device(&bt_session, "BBQ".to_string()).await?;
    println!("FOUND: {:?}", device_info);
    bt_session.connect(&device_info.id).await?;
    time::sleep(SCAN_DURATION).await;

    // println!("SERVICES {:?}", services);
    // println!("CHARACTERISTICS {:?}", characteristics);

    // time::delay_for(SCAN_DURATION).await;

    let characteristic = bt_session
        .get_service_characteristic_by_uuid(
            &device_info.id,
            BBQ_SERVICE_UUID,
            ACCOUNT_AND_VERIFY_UUID,
        )
        .await?;
    println!("Characteristic: {:?}", characteristic);
    bt_session
        .write_characteristic_value(&characteristic.id, CREDENTIAL_MSG)
        .await?;
    Ok(())
}
