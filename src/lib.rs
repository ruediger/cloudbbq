use bluez_async::{
    uuid_from_u16, BluetoothError, BluetoothSession, CharacteristicId, DeviceId, DeviceInfo,
};
use std::time::Duration;
use thiserror::Error;
use tokio::time;
use uuid::Uuid;

const SCAN_DURATION: Duration = Duration::from_secs(5);

// https://gist.github.com/uucidl/b9c60b6d36d8080d085a8e3310621d64
const BBQ_SERVICE_UUID: Uuid = uuid_from_u16(0xFFF0);
const SETTING_RESULT_UUID: Uuid = uuid_from_u16(0xFFF1);
const ACCOUNT_AND_VERIFY_UUID: Uuid = uuid_from_u16(0xFFF2);
const HISTORY_DATA_UUID: Uuid = uuid_from_u16(0xFFF3);
const REAL_TIME_DATA_UUID: Uuid = uuid_from_u16(0xFFF4);
const SETTING_DATA_UUID: Uuid = uuid_from_u16(0xFFF5);

const CREDENTIAL_MSG: [u8; 15] = [
    0x21, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0xb8, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00,
];

// Possible values for the first byte of 'setting data'.
const SET_UNIT_COMMAND: u8 = 0x02;

const UNITS_CELCIUS_ARGUMENT: u8 = 0x00;
const UNITS_FAHRENHEIT_ARGUMENT: u8 = 0x01;

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

/// A Bluetooth BBQ thermometer device which is connected.
#[derive(Clone, Debug)]
pub struct BBQDevice {
    bt_session: BluetoothSession,
    setting_result_characteristic: CharacteristicId,
    account_and_verify_characteristic: CharacteristicId,
    history_data_characteristic: CharacteristicId,
    real_time_data_characteristic: CharacteristicId,
    setting_data_characteristic: CharacteristicId,
}

impl BBQDevice {
    /// Construct a new BBQDevice wrapper around an appropriate Bluetooth device which is already
    /// connected.
    pub async fn new(
        bt_session: BluetoothSession,
        device: DeviceId,
    ) -> Result<BBQDevice, BluetoothError> {
        let service = bt_session
            .get_service_by_uuid(&device, BBQ_SERVICE_UUID)
            .await?
            .id;
        let setting_result_characteristic = bt_session
            .get_characteristic_by_uuid(&service, SETTING_RESULT_UUID)
            .await?
            .id;
        let account_and_verify_characteristic = bt_session
            .get_characteristic_by_uuid(&service, ACCOUNT_AND_VERIFY_UUID)
            .await?
            .id;
        let history_data_characteristic = bt_session
            .get_characteristic_by_uuid(&service, HISTORY_DATA_UUID)
            .await?
            .id;
        let real_time_data_characteristic = bt_session
            .get_characteristic_by_uuid(&service, REAL_TIME_DATA_UUID)
            .await?
            .id;
        let setting_data_characteristic = bt_session
            .get_characteristic_by_uuid(&service, SETTING_DATA_UUID)
            .await?
            .id;
        Ok(BBQDevice {
            bt_session,
            setting_result_characteristic,
            account_and_verify_characteristic,
            history_data_characteristic,
            real_time_data_characteristic,
            setting_data_characteristic,
        })
    }

    /// Authenticate with the device. This must be done before anything else, or it will disconnect
    /// after a short time.
    pub async fn authenticate(&self) -> Result<(), BluetoothError> {
        self.bt_session
            .write_characteristic_value(&self.account_and_verify_characteristic, CREDENTIAL_MSG)
            .await
    }

    /// Configure which temperature unit the device will use for its display. This does not affect
    /// the Bluetooth interface.
    pub async fn set_temperature_unit(&self, unit: TemperatureUnit) -> Result<(), BluetoothError> {
        let argument = match unit {
            TemperatureUnit::Celcius => UNITS_CELCIUS_ARGUMENT,
            TemperatureUnit::Fahrenheit => UNITS_FAHRENHEIT_ARGUMENT,
        };
        let command = [SET_UNIT_COMMAND, argument, 0, 0, 0, 0];
        self.bt_session
            .write_characteristic_value(&self.setting_data_characteristic, command)
            .await
    }
}

/// The temperature unit which the thermometer uses for its display.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TemperatureUnit {
    /// ºC
    Celcius,
    /// ºF
    Fahrenheit,
}
