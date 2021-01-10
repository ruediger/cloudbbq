use bluez_async::{uuid_from_u16, BluetoothError, BluetoothSession, DeviceId, DeviceInfo};
use std::time::Duration;
use thiserror::Error;
use tokio::time;
use uuid::Uuid;

const SCAN_DURATION: Duration = Duration::from_secs(5);

// https://gist.github.com/uucidl/b9c60b6d36d8080d085a8e3310621d64
const BBQ_SERVICE_UUID: Uuid = uuid_from_u16(0xFFF0);
const ACCOUNT_AND_VERIFY_UUID: Uuid = uuid_from_u16(0xFFF2);

const CREDENTIAL_MSG: [u8; 15] = [
    0x21, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0xb8, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const DEVICE_NAMES: [&str; 2] = ["BBQ", "iBBQ"];

#[derive(Debug, Error)]
pub enum Error {
    #[error("No device was found")]
    NoDeviceFound,
    /// There was an error communicating over Bluetooth.
    #[error(transparent)]
    Bluetooth(#[from] BluetoothError),
}

pub async fn find_device(bt_session: &BluetoothSession) -> Result<DeviceInfo, Error> {
    bt_session.start_discovery().await?;
    time::sleep(SCAN_DURATION).await;

    let devices = bt_session.get_devices().await?;
    for device in devices.into_iter() {
        if matches!(&device.name, Some(name) if DEVICE_NAMES.contains(&name.as_str())) {
            return Ok(device);
        }
    }
    Err(Error::NoDeviceFound)
}

pub async fn authenticate(
    bt_session: &BluetoothSession,
    device: &DeviceId,
) -> Result<(), BluetoothError> {
    let characteristic = bt_session
        .get_service_characteristic_by_uuid(device, BBQ_SERVICE_UUID, ACCOUNT_AND_VERIFY_UUID)
        .await?;
    println!("Characteristic: {:?}", characteristic);
    bt_session
        .write_characteristic_value(&characteristic.id, CREDENTIAL_MSG)
        .await
}
