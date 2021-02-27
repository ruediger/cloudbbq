use bluez_async::{
    uuid_from_u16, BluetoothError, BluetoothEvent, BluetoothSession, CharacteristicEvent,
    CharacteristicId, DeviceId, DeviceInfo,
};
use futures::future;
use futures::stream::{Stream, StreamExt};
use log::info;
use std::convert::TryInto;
use std::ops::Range;
use thiserror::Error;
use uuid::Uuid;

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
const SET_TARGET_TEMP_COMMAND: u8 = 0x01;
const SET_UNIT_COMMAND: u8 = 0x02;
const REAL_TIME_DATA_COMMAND: u8 = 0x0B;
const REQUEST_PROPERTY_COMMAND: u8 = 0x08;

const UNITS_CELCIUS_ARGUMENT: u8 = 0x00;
const UNITS_FAHRENHEIT_ARGUMENT: u8 = 0x01;

// Possible values for the first byte of the 'setting result'.
const SILENCE_PRESSED: u8 = 0x04;
const BATTERY_LEVEL_PROPERTY_ID: u8 = 0x24;
const ACKNOWLEDGE_COMMAND: u8 = 0xFF;

// Special temperature values.
const ABSENT_PROBE_VALUE: f32 = -1.0;
const TARGET_TEMP_NONE: f32 = -300.0;
const TEMPERATURE_MAX: f32 = i16::MAX as f32 / 10.0;
const TEMPERATURE_MIN: f32 = i16::MIN as f32 / 10.0;

const DEVICE_NAMES: [&str; 2] = ["BBQ", "iBBQ"];

/// An error communicating with a BBQ thermometer device.
#[derive(Debug, Error)]
pub enum Error {
    /// The given temperature could not be encoded because it is too high or too low.
    #[error("Temperature {0} out of range")]
    TemperatureEncodingError(f32),
    /// There was an error communicating over Bluetooth.
    #[error(transparent)]
    Bluetooth(#[from] BluetoothError),
}

/// Return all compatible BBQ thermometer devices currently known by the system.
pub async fn find_devices(bt_session: &BluetoothSession) -> Result<Vec<DeviceInfo>, Error> {
    let devices = bt_session.get_devices().await?;
    Ok(devices
        .into_iter()
        .filter(BBQDevice::is_compatible)
        .collect())
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
    /// Return whether the given Bluetooth device is a compatible BBQ thermometer.
    pub fn is_compatible(device: &DeviceInfo) -> bool {
        matches!(&device.name, Some(name) if DEVICE_NAMES.contains(&name.as_str()))
    }

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

    /// Set the desired temperature range for the given temperature probe. If the temperature goes
    /// outside the given range then the device will sound an alarm.
    async fn set_target_range(&self, probe: u8, range: Range<f32>) -> Result<(), Error> {
        let bottom_bytes = encode_temperature(range.start)?;
        let top_bytes = encode_temperature(range.end)?;
        let value = [
            SET_TARGET_TEMP_COMMAND,
            probe,
            bottom_bytes[0],
            bottom_bytes[1],
            top_bytes[0],
            top_bytes[1],
        ];
        self.bt_session
            .write_characteristic_value(&self.setting_data_characteristic, value)
            .await?;
        Ok(())
    }

    /// Set the target temperature for the given temperature probe. Once the temperature goes above
    /// the given value the device will sound an alarm.
    pub async fn set_target_temp(&self, probe: u8, target: f32) -> Result<(), Error> {
        self.set_target_range(probe, TARGET_TEMP_NONE..target).await
    }

    /// Enable or disable the device from sending real-time temperature data from its probes.
    pub async fn enable_real_time_data(&self, enable: bool) -> Result<(), BluetoothError> {
        let argument = if enable { 0x01 } else { 0x00 };
        let command = [REAL_TIME_DATA_COMMAND, argument, 0, 0, 0, 0];
        self.bt_session
            .write_characteristic_value(&self.setting_data_characteristic, command)
            .await
    }

    /// Request that the device report its current battery level. The result will come as a
    /// `SettingResult` event.
    pub async fn request_battery_level(&self) -> Result<(), BluetoothError> {
        let command = [
            REQUEST_PROPERTY_COMMAND,
            BATTERY_LEVEL_PROPERTY_ID,
            0,
            0,
            0,
            0,
        ];
        self.bt_session
            .write_characteristic_value(&self.setting_data_characteristic, command)
            .await
    }

    /// Get a stream of real time data from the device.
    ///
    /// You must also call `enable_real_time_data(true)` to actually get some data.
    pub async fn real_time(&self) -> Result<impl Stream<Item = RealTimeData>, BluetoothError> {
        let real_time_data_characteristic = self.real_time_data_characteristic.clone();
        self.bt_session
            .start_notify(&real_time_data_characteristic)
            .await?;
        let events = self
            .bt_session
            .characteristic_event_stream(&real_time_data_characteristic)
            .await?;
        Ok(StreamExt::filter_map(events, move |event| {
            future::ready(match event {
                BluetoothEvent::Characteristic {
                    id,
                    event: CharacteristicEvent::Value { value },
                } if id == real_time_data_characteristic => RealTimeData::try_parse(&value),
                _ => {
                    info!("Unexpected Bluetooth event {:?}", event);
                    None
                }
            })
        }))
    }

    /// Get a stream of setting results from the device. This includes responses to commands,
    /// battery level notifications, and notifications that the alarm has been silenced.
    pub async fn setting_results(
        &self,
    ) -> Result<impl Stream<Item = SettingResult>, BluetoothError> {
        let setting_result_characteristic = self.setting_result_characteristic.clone();
        self.bt_session
            .start_notify(&setting_result_characteristic)
            .await?;
        let events = self
            .bt_session
            .characteristic_event_stream(&setting_result_characteristic)
            .await?;
        Ok(StreamExt::filter_map(events, move |event| {
            future::ready(match event {
                BluetoothEvent::Characteristic {
                    id,
                    event: CharacteristicEvent::Value { value },
                } if id == setting_result_characteristic => SettingResult::try_parse(&value),
                _ => {
                    info!("Unexpected Bluetooth event {:?}", event);
                    None
                }
            })
        }))
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

/// A data point from a BBQ device, giving the temperature of all connected probes.
#[derive(Clone, Debug, PartialEq)]
pub struct RealTimeData {
    /// The current temperature of each probe in degrees Celcius, or None if the probe is
    /// disconnected.
    pub probe_temperatures: Vec<Option<f32>>,
}

impl RealTimeData {
    fn try_parse(value: &[u8]) -> Option<RealTimeData> {
        if value.len() % 2 != 0 {
            return None;
        }
        Some(RealTimeData {
            probe_temperatures: value
                .chunks_exact(2)
                .map(|bytes| {
                    let temperature = decode_temperature(bytes.try_into().unwrap());
                    if temperature == ABSENT_PROBE_VALUE {
                        None
                    } else {
                        Some(temperature)
                    }
                })
                .collect(),
        })
    }
}

/// A response to some command sent to the device, or a notification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettingResult {
    /// A confirmation that the given command has been received.
    AcknowledgeCommand { command_id: u8 },
    /// The current battery level of the device.
    BatteryLevel {
        current_voltage: u16,
        max_voltage: u16,
    },
    /// A notification that the button on the device has been pressed to stop the target temperature
    /// alarm sounding.
    SilencePressed,
}

impl SettingResult {
    fn try_parse(value: &[u8]) -> Option<SettingResult> {
        if value.len() != 6 {
            return None;
        }
        match value[0] {
            ACKNOWLEDGE_COMMAND => {
                assert!(value[2..] == [0, 0, 0, 0]);
                Some(SettingResult::AcknowledgeCommand {
                    command_id: value[1],
                })
            }
            BATTERY_LEVEL_PROPERTY_ID => Some(SettingResult::BatteryLevel {
                current_voltage: u16::from_le_bytes(value[1..=2].try_into().unwrap()),
                max_voltage: u16::from_le_bytes(value[3..=4].try_into().unwrap()),
            }),
            SILENCE_PRESSED => {
                assert!(value[1..] == [0xFF, 0, 0, 0, 0]);
                Some(SettingResult::SilencePressed)
            }
            _ => {
                info!("Unrecognised setting result: {:?}", value);
                None
            }
        }
    }
}

fn encode_temperature(temperature: f32) -> Result<[u8; 2], Error> {
    if temperature < TEMPERATURE_MIN || temperature > TEMPERATURE_MAX {
        return Err(Error::TemperatureEncodingError(temperature));
    }
    let temperature_fixed = (temperature * 10.0) as i16;
    Ok(temperature_fixed.to_le_bytes())
}

fn decode_temperature(bytes: [u8; 2]) -> f32 {
    i16::from_le_bytes(bytes) as f32 / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_real_time_invalid() {
        assert_eq!(RealTimeData::try_parse(&[0]), None);
    }

    #[test]
    fn parse_real_time_no_probes() {
        assert_eq!(
            RealTimeData::try_parse(&[0xF6, 0xFF, 0xF6, 0xFF]),
            Some(RealTimeData {
                probe_temperatures: vec![None, None]
            })
        );
    }

    #[test]
    fn parse_real_time() {
        assert_eq!(
            RealTimeData::try_parse(&[1, 2, 3, 4]),
            Some(RealTimeData {
                probe_temperatures: vec![Some(51.3), Some(102.7)]
            })
        );
    }

    #[test]
    fn parse_setting_result_invalid() {
        assert_eq!(SettingResult::try_parse(&[]), None);
    }

    #[test]
    fn parse_setting_result_acknowledge() {
        assert_eq!(
            SettingResult::try_parse(&[0xFF, 0x02, 0x00, 0x00, 0x00, 0x00]),
            Some(SettingResult::AcknowledgeCommand { command_id: 0x02 })
        );
    }

    #[test]
    fn parse_setting_result_battery_level() {
        assert_eq!(
            SettingResult::try_parse(&[0x24, 0x5B, 0x17, 0x96, 0x19, 0x00]),
            Some(SettingResult::BatteryLevel {
                current_voltage: 5979,
                max_voltage: 6550
            })
        );
    }

    #[test]
    fn parse_setting_result_silence_pressed() {
        assert_eq!(
            SettingResult::try_parse(&[0x04, 0xFF, 0x00, 0x00, 0x00, 0x00]),
            Some(SettingResult::SilencePressed)
        );
    }
}
