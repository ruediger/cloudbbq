use bluez_async::BluetoothSession;
use cloudbbq2_rs::{find_device, BBQDevice};
use std::time::Duration;
use tokio::time;

const WAIT_DURATION: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (_, bt_session) = BluetoothSession::new().await?;
    let device_info = find_device(&bt_session).await?;
    println!("FOUND: {:?}", device_info);
    bt_session.connect(&device_info.id).await?;
    time::sleep(WAIT_DURATION).await;

    let device = BBQDevice::new(bt_session, device_info.id).await?;
    device.authenticate().await?;
    Ok(())
}
