/*
  slonk, a rocket engine controller.
  Copyright (C) 2022 Rice Eclipse.

  slonk is free software: you can redistribute it and/or modify
  it under the terms of the GNU General Public License as published by
  the Free Software Foundation, either version 3 of the License, or
  (at your option) any later version.

  slonk is distributed in the hope that it will be useful,
  but WITHOUT ANY WARRANTY; without even the implied warranty of
  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
  GNU General Public License for more details.

  You should have received a copy of the GNU General Public License
  along with this program.  If not, see <http://www.gnu.org/licenses/>.
*/

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
    console::UserLog,
    execution::emergency_stop,
    hardware::{Adc, GpioPin},
    outgoing::{DashChannel, Message, SensorReading},
    ControllerError, ControllerState, StateGuard,
};

#[allow(dead_code)]
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
/// A function which will continuously listen for new data from sensors.
/// It will loop indefinitely.
///
/// # Inputs
///
/// * `thread_scope`: A reference to a scope that this function can use to spawn other threads.
///     This is required so that the sensor listener thread can emergency-stop, if needed.
/// * `group_id`: The ID of the sensor group that this thread is responsible for.
///     This is equal to the index of the sensor group in the configuration object.
/// * `adcs`: The set of ADCs which can be read from by the sensors.
/// * `configuration`: The primary configuration of the controller.
/// * `driver_lines`: The GPIO lines for each driver.
/// * `log_files`: Handles for log files associated with the sensors in this sensor group.
///     Each index corresponds exactly to its associated index in the group.
/// * `state`: The state of the whole system.
///     If a sensor enters an invalid value during ignition, this thread will automatically update
///     the state as needed.
/// * `dashboard_stream`: A stream where messages can be sent to the dashboard.
///
/// # Errors
///
/// This function will only return an error if the controller state status lock is poisoned.
///
/// # Panics
///
/// This function will panic in any of the following cases:
///
/// * If the value of `group_id` does not correspond to an existing sensor group.
/// * If the current system time is before the UNIX epoch.
pub fn sensor_listen<'a>(
    thread_scope: &'a Scope<'a, '_>,
    group_id: u8,
    configuration: &'a Configuration,
    driver_lines: &'a Mutex<Vec<impl GpioPin + Send + Sync>>,
    log_files: &mut [impl Write],
    user_log: &UserLog<impl Write>,
    adcs: &[Mutex<impl Adc>],
    state: &'a StateGuard,
    dashboard_stream: &'a Mutex<DashChannel<impl Write, impl Write>>,
) -> Result<(), ControllerError> {
    assert!(usize::from(group_id) < configuration.sensor_groups.len());

    // more convenient access to our sensor group data
    let group = &configuration.sensor_groups[usize::from(group_id)];
    // the last time that we sent a sensor status update
    let mut last_transmission_time = SystemTime::now();

    // the most recent reading from each sensor which has *not* already been sent to the dashboard.
    // each element will be None if the most recent reading was sent to the dashboard.
    let mut transmission_readings: Vec<Option<(SystemTime, u16)>> = vec![None; group.sensors.len()];

    // most recent values read, to be logged.
    // in each queue, the "back" contains the most recent readings and the "front" contains the
    // oldest ones.
    let mut most_recent_readings: Vec<VecDeque<(SystemTime, u16)>> =
        vec![VecDeque::new(); group.sensors.len()];

    // Rolling average values for sensor readings.
    let mut rolling_averages: Vec<f64> = group
        .sensors
        .iter()
        .map(|sensor| {
            // extract the middle value of the range to seed our rolling average so that the sensor
            // doesn't immediately start in a "bad" spot
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

    while state.status()? != ControllerState::Quit {
        // read from each device
        for (idx, sensor) in group.sensors.iter().enumerate() {
            let Ok(mut adc_guard) = adcs[usize::from(sensor.adc)].lock() else {
                #[allow(unused_must_use)]{
                    user_log.critical(&format!("unable to acquire mutex on sensor ADC for {} due to poisoning", sensor.label));
                }
                continue;
            };
            let adc_read_result = adc_guard.read(sensor.channel);
            let Ok(reading) = adc_read_result else {
                #[allow(unused_must_use)] {
                    user_log.warn(&format!("unable to read {} due to error: {adc_read_result:?}", sensor.label));
                }
                continue;
            };
            let read_time = SystemTime::now();
            most_recent_readings[idx].push_back((read_time, reading));
            transmission_readings[idx] = Some((read_time, reading));
            // update rolling averages
            let width = sensor.rolling_average_width.unwrap_or(1);
            let calibrated_value =
                f64::from(reading) * sensor.calibration_slope + sensor.calibration_intercept;
            let rolling_avg = (rolling_averages[idx] * (f64::from(width - 1)) + calibrated_value)
                / f64::from(width);
            rolling_averages[idx] = rolling_avg;

            // if rolling average went out of bounds, immediately start emergency stopping
            if let Some((min, max)) = sensor.range {
                #[allow(unused_must_use)]
                if rolling_avg < min || max < rolling_avg {
                    user_log.warn(&format!(
                        "Sensor {} was out of bounds with value {rolling_avg}, attempting emergency stop", 
                        sensor.label
                    ));
                    // oh no! a sensor is now in an illegal range!
                    // spin up another thread to emergency stop.
                    // this may return an error due to illegal transistion, but that is not our
                    // problem.
                    thread_scope.spawn(|| {
                        if let Ok(mut drivers_guard) = driver_lines.lock() {
                            emergency_stop(configuration, drivers_guard.as_mut(), state);
                        }
                    });
                }
            }
        }

        // transmit data to the dashboard if it's been long enough since our last transmission
        if SystemTime::now() > last_transmission_time + transmission_period {
            let mut channel_guard = dashboard_stream.lock()?;
            if channel_guard.has_target() {
                // send message to dashboard
                let send_result = channel_guard.send(&Message::SensorValue {
                    group_id,
                    readings: &transmission_readings
                        .iter()
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
                });

                #[allow(unused_must_use)]
                if let Err(e) = send_result {
                    user_log.warn(&format!("unable to transmit data to dashboard: {e:?}"));
                }
            }

            last_transmission_time = SystemTime::now();
            transmission_readings = vec![None; group.sensors.len()];
        }

        for (sensor_id, reading_queue) in most_recent_readings.iter().enumerate() {
            if reading_queue.len() >= configuration.log_buffer_size {
                #[allow(unused_must_use)]
                if let Err(e) = write_sensor_log(&mut log_files[sensor_id], reading_queue) {
                    user_log.warn(&format!(
                        "unable to write data for sensor {}: {e:?}",
                        group.sensors[sensor_id].label
                    ));
                }
            }
        }

        // use the system state to determine how long to sleep until the next loop.
        // standby means we are sampling slowly, and anything else means we sample quickly.
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

#[allow(dead_code)]
/// Periodically check in on the status of the drivers, and log that status.
/// Will also transmit that driver status to the dashboard.
///
/// # Inputs
///
/// * `configuration`: The configuration for the current mode of the controller.
/// * `driver_lines`: The driver GPIO pins.
/// * `log_file`: The file to which logs should be written.
///     Information will be written to the log file in the following format:
///     ```text
///      {time},{driver0_status},{driver1_status},
///
///     ```
///     with one row for every sample.
///     `{time}` is the number of nanoseconds since the UNIX epoch.
/// * `state`: The overall system state.
///     This function will only return after `State` transitions to `ControllerState::Quit`.
/// * `dashboard_stream`: A channel by which messages can be sent to the dashboard.
///
/// # Errors
///
/// This function will return an error if we are unable to acquire a lock or if we are unable to
/// write to the message log.
///
/// # Panics
///
/// This function will panic if the current time is before the UNIX epoch.
pub fn driver_status_listen(
    configuration: &Configuration,
    driver_lines: &Mutex<Vec<impl GpioPin>>,
    log_file: &mut impl Write,
    user_log: &UserLog<impl Write>,
    state: &StateGuard,
    dashboard_stream: &Mutex<DashChannel<impl Write, impl Write>>,
) -> Result<(), ControllerError> {
    // the time required to sleep
    let sleep_time = Duration::from_secs(1) / configuration.frequency_status;
    let mut driver_states = vec![false; driver_lines.lock()?.len()];
    while state.status()? != ControllerState::Quit {
        // read off the states of the drivers
        let read_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap();

        let Ok(mut drivers_guard) = driver_lines.lock() else {
            #[allow(unused_must_use)] {
                user_log.critical("Unable to acquire lock over driver mutex");
            }
            continue;
        };

        for (driver_idx, (driver_line, state_ref)) in
            drivers_guard.iter_mut().zip(&mut driver_states).enumerate()
        {
            match driver_line.read() {
                Ok(read_value) => *state_ref = read_value,
                #[allow(unused_must_use)]
                Err(e) => {
                    user_log.warn(&format!(
                        "Unable to read state of driver {driver_idx}: {e:?}"
                    ));
                }
            }
        }

        // write driver status information
        #[allow(unused_must_use)]
        if let Err(e) = write_driver_log(log_file, read_time, &driver_states) {
            user_log.warn(&format!(
                "Unable to write driver status information to log file: {e}"
            ));
        }

        // optionally transmit to dashboard
        dashboard_stream.lock()?.send(&Message::DriverValue {
            values: &driver_states,
        })?;

        // take a nap until we are ready to send another message
        sleep(sleep_time);
    }

    Ok(())
}

/// Write the status of the drivers to a log file.
///
/// # Inputs
///
/// * `log_file`: The file to which log information should be written.
/// * `read_time`: The time at which the read of driver states occurred.
/// * `driver_states`: The state of each driver.
///
/// # Errors
///
/// This function will return an `Err` if writing to the log file fails.
fn write_driver_log(
    log_file: &mut impl Write,
    read_time: Duration,
    driver_states: &[bool],
) -> std::io::Result<()> {
    write!(log_file, "{},", read_time.as_nanos())?;
    for driver_state in driver_states {
        write!(log_file, "{driver_state},")?;
    }
    writeln!(log_file)?;

    Ok(())
}

/// Write a new log datum to the sensor log file.
///
/// # Inputs
///
/// * `log_file`: The file to which the log will be written.
///     There must be exactly one log file per sensor.
/// * `adc_readings`: All the most recent sensor readings to be written to the file.
///
/// # Results
///
/// Will write the data from the ADC readings in a CSV format to the file.
/// There will be two "columns" to this CSV data:
/// 1. The time since the UNIX epoch, in nanoseconds.
/// 1. The raw ADC value of the sensor at this time.
/// Will also include a trailing newline after the last row.
/// At the end of writing all of these lines, the file will be "flushed," meaning that all data will
/// be immediately saved.
///
/// For instance, if a sensor had a reading of 42 at a time of 1 second, 500 nanoseconds after the
/// UNIX epoch began, the following text would be written:
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
/// This function will panic if a time contained in the ADC readings was before the UNIX epoch.
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

    use crate::hardware::ListenerPin;

    use super::*;

    /// Dummy ADC structure for testing.
    struct ReturnsNumber(u16);

    impl Adc for ReturnsNumber {
        fn read(&mut self, _: u8) -> Result<u16, crate::ControllerError> {
            Ok(self.0)
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn data_written() {
        // create some dummy configuration for the sensor listener thread to read
        let config = r#"{
            "frequency_status": 10,
            "log_buffer_size": 1,
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
            "pre_ignite_time": 500,
            "post_ignite_time": 5000,
            "drivers": [],
            "ignition_sequence": [],
            "estop_sequence": [],
            "spi_mosi": 0,
            "spi_miso": 0,
            "spi_clk": 0,
            "spi_frequency_clk": 50000,
            "adc_cs": [0, 0]
        }"#;
        let adcs: Vec<Mutex<ReturnsNumber>> =
            (0..2).map(|n| Mutex::new(ReturnsNumber(n))).collect();
        let mut cfg_cursor = Cursor::new(config);
        let config = Configuration::parse(&mut cfg_cursor).unwrap();
        let state = StateGuard::new(ControllerState::Standby);
        let mut logs = vec![Cursor::new(Vec::new()); 2];
        // log file of outputs
        let mut output_log = Vec::new();
        // stream of outgoing messages
        let mut output_stream_buf = Vec::new();
        let output_stream = Mutex::new(DashChannel::<&mut Vec<u8>, &mut Vec<u8>>::new(
            &mut output_log,
        ));
        output_stream
            .lock()
            .unwrap()
            .set_channel(&mut output_stream_buf);
        let driver_lines = Mutex::new(Vec::<ListenerPin>::new());

        // actual magic happens here
        scope(|s| {
            // spawn a sensor listener thread and let it do its thing
            let handle = s.spawn(|| {
                sensor_listen(
                    s,
                    0,
                    &config,
                    &driver_lines,
                    &mut logs,
                    &UserLog::new(Vec::<u8>::new()),
                    &adcs,
                    &state,
                    &output_stream,
                )
            });

            // give the thread enough time to read exactly one value
            sleep(Duration::from_millis(150));

            // notify the thread to die
            // hackery to make a valid state transition sequence
            state.move_to(ControllerState::Quit).unwrap();
            println!("joining...");

            // collect the thread's return value
            handle.join().unwrap().unwrap();
        });

        // validate the one sensor reading that was sent to our dummy dashboard

        let json_val: Value = serde_json::from_slice(&output_stream_buf).unwrap();

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

        assert_eq!(readings, [(0, 0), (1, 1)]);

        // check that log information was generated correctly
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
            assert_eq!(tokens[5], format!("{idx}").as_str());
            assert_eq!(tokens[6], "");
        }
    }

    #[test]
    /// Test that an emergency stop is successfully called.
    fn estop_called() {
        // create some dummy configuration for the sensor listener thread to read
        let config = r#"{
            "frequency_status": 10,
            "log_buffer_size": 1,
            "sensor_groups": [
                {
                    "label": "dummy",
                    "frequency_standby": 1,
                    "frequency_ignition": 1,
                    "frequency_transmission": 1,
                    "sensors": [
                        {
                            "label": "dummy_sensor0",
                            "units": "mops",
                            "calibration_intercept": 0,
                            "calibration_slope": 1,
                            "adc": 0,
                            "channel": 0,
                            "rolling_average_width": 2,
                            "range": [-5, 5]
                        }
                    ]
                }
            ],
            "pre_ignite_time": 0,
            "post_ignite_time": 0,
            "drivers": [],
            "ignition_sequence": [],
            "estop_sequence": [ {
                "type": "Sleep",
                "duration": {
                    "secs": 1,
                    "nanos": 0
                }
            }],
            "spi_mosi": 0,
            "spi_miso": 0,
            "spi_clk": 0,
            "spi_frequency_clk": 50000,
            "adc_cs": [0]
        }"#;

        let adc = Mutex::new(ReturnsNumber(100));

        let mut cfg_cursor = Cursor::new(config);
        let config = Configuration::parse(&mut cfg_cursor).unwrap();

        let state = StateGuard::new(ControllerState::Standby);
        let mut logs = vec![Cursor::new(Vec::new()); 2];
        let output_stream = Mutex::new(DashChannel::<Vec<u8>, Vec<u8>>::new(Vec::new()));
        let driver_lines = Mutex::new(Vec::<ListenerPin>::new());

        // actual magic happens here
        scope(|s| {
            // spawn a sensor listener thread and let it do its thing
            s.spawn(|| {
                sensor_listen(
                    s,
                    0,
                    &config,
                    &driver_lines,
                    &mut logs,
                    &UserLog::new(Vec::<u8>::new()),
                    &[adc],
                    &state,
                    &output_stream,
                )
            });

            // give the thread enough time to read values
            sleep(Duration::from_millis(200));

            // check that we are currently e-stopping
            assert_eq!(state.status().unwrap(), ControllerState::EStopping);

            // continually attempt to kill the thread
            while state.move_to(ControllerState::Quit).is_err() {
                println!("failed: {:?}", state.status().unwrap());
            }
        });
    }
}
