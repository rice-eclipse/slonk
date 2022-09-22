//! The core sensor data collection threads.

use std::{
    collections::VecDeque,
    io::Write,
    sync::Mutex,
    thread::{sleep, Scope},
    time::{Duration, SystemTime},
};

use crate::{
    config::Configuration,
    execution::emergency_stop,
    hardware::Adc,
    outgoing::{Message, SensorReading},
    ControllerError, ControllerState, StateGuard,
};

#[allow(dead_code)]
/// A function which will continuously listen for new data from sensors.
/// It will loop indefinitely.
///
/// # Inputs
///
/// * `thread_scope`: A reference to a scope that this function can use to spawn
///     other threads.
///     This is required so that the sensor listener thread can emergency-stop,
///     if needed.
/// * `group_id`: The ID of the sensor group that this thread is responsible
///     for.
///     This is equal to the index of the sensor group in the configuration
///     object.
/// * `adcs`: The set of ADCs which can be read from by the sensors.
/// * `configuration`: The primary configuration of the controller.
/// * `state`: The state of the whole system.
///     If a sensor enters an invalid value during ignition, this thread will
///     automatically update the state as needed.
/// * `dashboard_stream`: A stream where messages can be sent to the dashboard.
///
/// # Panics
///
/// This function will panic in any of the following cases:
///
/// * If the value of `group_id` does not correspond to an existing sensor
///     group.
fn sensor_listen<'a>(
    thread_scope: &'a Scope<'a, '_>,
    group_id: u8,
    configuration: &'a Configuration,
    log_files: &mut [impl Write],
    adcs: &[impl Adc],
    state: &'a StateGuard,
    dashboard_stream: &'a Mutex<Option<impl Write>>,
) -> Result<(), ControllerError> {
    assert!(usize::from(group_id) < configuration.sensor_groups.len());

    // more convenient access to our sensor group data
    let group = &configuration.sensor_groups[usize::from(group_id)];
    // the last time that we sent a sensor status update
    let mut last_transmission_time = SystemTime::now();
    // the last time that we wrote to the log files
    let mut last_log_time = SystemTime::now();

    // most recent values read, to be sent to the dashboard and logged.
    // the external bool is whether the most recent reading in the queue has
    // already been sent to the dashboard.
    // in each queue, the "back" contains the most recent readings and the
    // "front" contains the oldest ones.
    let mut most_recent_readings: Vec<(bool, VecDeque<(SystemTime, u16)>)> =
        vec![(false, VecDeque::new()); group.sensors.len()];
    // Rolling average values for sensor readings.
    let mut rolling_averages: Vec<f64> = group
        .sensors
        .iter()
        .map(|sensor| {
            // extract the middle value of the range to seed our rolling
            // average so that the sensor doesn't immediately start in a "bad"
            // spot
            if let Some((min, max)) = sensor.range {
                (min + max) / 2.0
            } else {
                0.0
            }
        })
        .collect();

    let standby_period = Duration::from_secs(1) / group.frequency_standby;
    let ignition_period = Duration::from_secs(1) / group.frequency_ignition;
    let transmission_period = Duration::from_secs(1) / group.frequency_transmission;
    let log_period = Duration::from_secs(1) / configuration.frequency_logging;

    let consumer_name = format!("sensor_listener_{}", group_id);

    while state.status()? != ControllerState::Quit {
        // read from each device
        for (idx, sensor) in group.sensors.iter().enumerate() {
            let reading = adcs[usize::from(sensor.adc)].read(&consumer_name, sensor.channel)?;
            let read_time = SystemTime::now();
            most_recent_readings[idx].0 = false;
            most_recent_readings[idx].1.push_back((read_time, reading));

            // update rolling averages
            let width = sensor.rolling_average_width.unwrap_or(1);
            let calibrated_value =
                f64::from(reading) * sensor.calibration_slope + sensor.calibration_intercept;
            let rolling_avg = (rolling_averages[idx] * (f64::from(width - 1)) + calibrated_value)
                / f64::from(width);
            rolling_averages[idx] = rolling_avg;

            // if rolling average went out of bounds, immediately start
            // emergency stopping
            if let Some((min, max)) = sensor.range {
                if rolling_avg < min || max < rolling_avg {
                    // oh no! a sensor is now in an illegal range!
                    // spin up another thread to emergency stop
                    thread_scope.spawn(|| emergency_stop(configuration, state));
                }
            }
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
                            .iter()
                            .enumerate()
                            .filter_map(|(sensor_id, (was_sent, deque))| {
                                if *was_sent {
                                    None
                                } else {
                                    #[allow(clippy::cast_possible_truncation)]
                                    deque.back().map(|&(time, reading)| SensorReading {
                                        sensor_id: sensor_id as u8,
                                        reading,
                                        time,
                                    })
                                }
                            })
                            .collect::<Vec<_>>(),
                    },
                )?;
            }

            last_transmission_time = SystemTime::now();
            for (ref mut sent, _) in &mut most_recent_readings {
                // mark all most recent readings as beeing created
                *sent = true;
            }
        }

        if SystemTime::now() > last_log_time + log_period {
            for (sensor_id, (_, reading_queue)) in most_recent_readings.iter().enumerate() {
                write_sensor_log(&mut log_files[sensor_id], reading_queue)?;
            }

            last_log_time = SystemTime::now();
        }

        // use the system state to determine how long to sleep until the next
        // loop.
        // standby means we are sampling slowly, and anything else means we
        // sample quickly.
        let sleep_time = match state.status()? {
            ControllerState::Standby => standby_period,
            _ => ignition_period,
        };

        // now take a nap until we next need to get data
        sleep(sleep_time);
    }

    // we are now quitting
    Ok(())
}

/// Write a new log datum to the sensor log file.
///
/// # Inputs
///
/// * `log_file`: The file to which the log will be written.
///     There must be exactly one log file per sensor.
/// * `adc_readings`: All the most recent sensor readings to be written to the
///     file.
///
/// # Results
///
/// Will write the data from the ADC readings in a CSV format to the file.
/// There will be two "columns" to this CSV data:
/// 1. The time since the UNIX epoch, in nanoseconds.
/// 1. The raw ADC value of the sensor at this time.
/// Will also include a trailing newline after the last row.
/// At the end of writing all of these lines, the file will be "flushed,"
/// meaning that all data will be immediately saved.
///
/// For instance, if a sensor had a reading of 42 at a time of 1 second, 500
/// nanoseconds after the UNIX epoch began, the following text would be
/// written:
///
/// ```text
/// 1000000500,42
///
/// ```
///
/// If the entire process succeeds, this function will return `Ok(())`.
///
/// # Errors
///
/// This function will return an `Err` if writing to the log file fails.
///
/// # Panics
///
/// This function will panic if a time contained in the ADC readings was before
/// the UNIX epoch.
fn write_sensor_log<'a>(
    log_file: &mut impl Write,
    adc_readings: impl IntoIterator<Item = &'a (SystemTime, u16)>,
) -> std::io::Result<()> {
    for (sys_time, reading) in adc_readings {
        let since_epoch_time = sys_time.duration_since(SystemTime::UNIX_EPOCH).unwrap();

        writeln!(log_file, "{},{}", since_epoch_time.as_nanos(), reading)?;
    }

    log_file.flush()
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, thread::scope};

    use serde_json::Value;

    use super::*;

    /// Dummy ADC structure for testing.
    struct ReturnsNumber(u16);

    impl Adc for ReturnsNumber {
        fn read(&self, _: &str, _: u8) -> Result<u16, crate::ControllerError> {
            Ok(self.0)
        }
    }

    #[test]
    fn data_written() {
        // create some dummy configuration for the sensor listener thread to
        // read
        let config = r#"{
            "frequency_status": 10,
            "frequency_logging": 100,
            "sensor_groups": [
                {
                    "label": "dummy",
                    "frequency_standby": 10,
                    "frequency_ignition": 10,
                    "frequency_transmission": 10,
                    "sensors": [
                        {
                            "label": "dummy_sensor0",
                            "units": "mops",
                            "calibration_intercept": 0,
                            "calibration_slope": 0,
                            "adc": 0,
                            "channel": 0
                        },
                        {
                            "label": "dummy_sensor1",
                            "units": "mops",
                            "calibration_intercept": 0,
                            "calibration_slope": 0,
                            "adc": 1,
                            "channel": 0
                        }
                    ]
                }
            ],
            "drivers": [],
            "ignition_sequence": [],
            "estop_sequence": [],
            "spi_mosi": 0,
            "spi_miso": 0,
            "spi_clk": 0,
            "spi_frequency_clk": 50000,
            "adc_cs": [0, 0]
        }"#;
        let adcs: Vec<ReturnsNumber> = (0..2).map(ReturnsNumber).collect();
        let mut cfg_cursor = Cursor::new(config);
        let config = Configuration::parse(&mut cfg_cursor).unwrap();
        let state = StateGuard::new(ControllerState::Standby);
        let mut logs = vec![Cursor::new(Vec::new()); 2];
        let output_stream = Mutex::new(Some(Vec::new()));

        // actual magic happens here
        scope(|s| {
            // spawn a sensor listener thread and let it do its thing
            let handle =
                s.spawn(|| sensor_listen(s, 0, &config, &mut logs, &adcs, &state, &output_stream));

            // give the thread enough time to read exactly one value
            sleep(Duration::from_millis(150));

            // notify the thread to die
            state.move_to(ControllerState::Quit).unwrap();

            // collect the thread's return value
            handle.join().unwrap().unwrap();
        });

        // validate the one sensor reading that was sent to our dummy dashboard

        let json_val: Value =
            serde_json::from_slice(output_stream.lock().unwrap().as_deref().unwrap()).unwrap();

        let json_obj = json_val.as_object().unwrap();

        assert_eq!(
            json_obj.get("type").unwrap().as_str().unwrap(),
            "SensorValue"
        );

        assert_eq!(json_obj.get("group_id").unwrap().as_u64().unwrap(), 0);

        // extract group ids and reading values
        let readings: Vec<(u64, u64)> = json_obj
            .get("readings")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|val| val.as_object().unwrap())
            .map(|obj| {
                (
                    obj.get("sensor_id").unwrap().as_u64().unwrap(),
                    obj.get("reading").unwrap().as_u64().unwrap(),
                )
            })
            .collect();

        let logged_strings: Vec<String> = logs
            .into_iter()
            .map(|cursor| String::from_utf8(cursor.into_inner()).unwrap())
            .collect();

        for (idx, logged_string) in logged_strings.iter().enumerate() {
            let tokens = logged_string
                .split(',')
                .flat_map(|s| s.split('\n'))
                .map(String::from)
                .collect::<Vec<_>>();

            // check that sensor readings got written in the right columns
            assert_eq!(tokens[1], format!("{idx}").as_str());
            assert_eq!(tokens[3], format!("{idx}").as_str());
            assert_eq!(tokens[4], "");
        }

        assert_eq!(readings, [(0, 0), (1, 1)]);
    }
}
