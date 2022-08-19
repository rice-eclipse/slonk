//! The core sensor data collection threads.

use std::{
    io::Write,
    sync::{Mutex, RwLock},
    thread::sleep,
    time::{Duration, SystemTime},
};

use crate::{
    config::Configuration,
    hardware::Mcp3208,
    outgoing::{Message, SensorReading},
    ControllerError, ControllerState,
};

#[allow(dead_code)]
/// A function which will continuously listen for new data from sensors.
/// It will loop indefinitely.
///
/// # Inputs
///
/// * `group_id`: The ID of the sensor group that this thread is responsible
///     for.
///     This is equal to the index of the sensor group in the configuration
///     object.
/// * `bus`: The SPI bus which the ADCs for this sensor group are on.
/// * `chip`: The GPIO chip to be used for actuating chip select pins for each
///     ADC.
/// * `configuration`: The primary configuration of the controller.
/// * `state`: The state of the whole system.
///     If a sensor enters an invalid value during ignition, this thread will
///     automatically update the state as needed.
/// * `dashboard_stream`:
///
/// # Panics
///
/// This function will panic in any of the following cases:
///
/// * If the value of `group_id` does not correspond to an existing sensor
///     group.
fn sensor_listen<'a>(
    group_id: u8,
    configuration: &Configuration,
    adcs: &[Mcp3208],
    state: &'a RwLock<ControllerState>,
    dashboard_stream: &'a Mutex<Option<impl Write>>,
) -> Result<(), ControllerError> {
    assert!(usize::from(group_id) < configuration.sensor_groups.len());

    // more convenient access to our sensor group data
    let group = &configuration.sensor_groups[usize::from(group_id)];
    // the last time that we sent a sensor status update
    let last_transmission_time = SystemTime::now();
    // most recent values read, to be sent to the dashboard
    let mut most_recent_readings: Vec<Option<(SystemTime, u16)>> = vec![None; group.sensors.len()];

    let standby_period = Duration::from_secs(1) / group.frequency_standby;
    let ignition_period = Duration::from_secs(1) / group.frequency_ignition;
    let transmission_period = Duration::from_secs(1) / group.frequency_transmission;

    let consumer_name = format!("sensor_listener_{}", group_id);

    loop {
        // read from each device
        for (idx, sensor) in group.sensors.iter().enumerate() {
            let reading = adcs[usize::from(sensor.adc)].read(&consumer_name, sensor.channel)?;
            let read_time = SystemTime::now();
            most_recent_readings[idx] = Some((read_time, reading));
        }

        // transmit data to the dashboard if it's been long enough since our
        // last transmission
        if SystemTime::now() > last_transmission_time + transmission_period {
            if let Some(stream) = dashboard_stream.lock()?.as_mut() {
                serde_json::to_writer(
                    stream,
                    // some iterator hackery to get things in the right shape
                    &Message::SensorValue {
                        group_id,
                        readings: &most_recent_readings
                            .into_iter()
                            .enumerate()
                            .filter_map(|(sensor_id, opt)| {
                                #[allow(clippy::cast_possible_truncation)]
                                opt.map(|(time, reading)| SensorReading {
                                    sensor_id: sensor_id as u8,
                                    reading,
                                    time,
                                })
                            })
                            .collect::<Vec<_>>(),
                    },
                )?;
            }

            most_recent_readings = vec![None; group.sensors.len()];
        }

        // use the system state to determine how long to sleep until the next
        // loop.
        // standby means we are sampling slowly, and anything else means we
        // sample quickly.
        let state_guard = state.read()?;
        let sleep_time = match &*state_guard {
            ControllerState::Standby => standby_period,
            _ => ignition_period,
        };

        // now take a nap until we next need to get data
        sleep(sleep_time);
    }
}
