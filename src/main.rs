use bluez_async::BluetoothSession;
use cloudbbq::{find_devices, BBQDevice, TemperatureUnit};
use futures::select;
use futures::stream::StreamExt;
use std::time::Duration;
use tokio::time;

const SCAN_DURATION: Duration = Duration::from_secs(5);
const WAIT_DURATION: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (_, bt_session) = BluetoothSession::new().await?;
    bt_session.start_discovery().await?;
    time::sleep(SCAN_DURATION).await;
    let devices = find_devices(&bt_session).await?;
    if devices.is_empty() {
        println!("No devices found");
        return Ok(());
    }
    println!("FOUND: {:?}", devices);
    let device_id = devices[0].id.clone();
    bt_session.connect(&device_id).await?;
    time::sleep(WAIT_DURATION).await;

    let device = BBQDevice::new(bt_session, device_id).await?;
    device.authenticate().await?;

    let mut setting_results = device.setting_results().await?.fuse();
    device.request_battery_level().await?;

    println!("Setting unit");
    device
        .set_temperature_unit(TemperatureUnit::Celcius)
        .await?;

    device.set_target_temp(0, 35.0).await?;

    let mut real_time_data = device.real_time().await?.fuse();
    device.enable_real_time_data(true).await?;

    println!("Events:");
    loop {
        select! {
            data = real_time_data.select_next_some() => println!("Realtime data: {:?}", data),
            result = setting_results.select_next_some() => println!("Setting result: {:?}", result),
            complete => break,
        };
    }

    Ok(())
}
