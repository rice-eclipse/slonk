# `slonk` Rewrite API Proposal

Since we're rewriting the engine controller, we might as well rewrite the API.
This proposal outlines a new API for `slonk`, moving away from RESFET's opaque,
implementation-dependent approach.

## Terms

- _Dashboard_: the device running `slonkboard`.
  Acts as a client to the controller.

- _Controller_: the device running the replacement for `slonk`.

- _Sensor_: For the sake of this API, a "sensor" is anything that can be read from an ADC and has a
  calibration.
  If, in the future, we decide to interface with sensors by means other than an ADC, we may change
  this definition (and, accordingly, how the configuration setup works).

- _Sensor group_: A sensor group is a set of sensors which are sampled at the same rate and will be
  handled by the same thread to reduce overhead.

## At a glance

Communication is done between the dashboard and controller by pure TCP (as opposed to UDP, as
before).
The key reason is that there's little performance justification for just randomly losing datapoints
in the stream, and there's not enough data being sent across to be worth caring about the
transmission overhead.
Across these channels, both the controller and dashboard can send messages to each other.

Messages traveling in either direction will be formatted using JSON.
The overarching structure of the messages will be the same across both directions, and at the top
level should be a mapping containing keys with timestamps, message types, and potentially other
debug information.

Configuration files will also be formatted with JSON.
To avoid having users use mismatched configurations, the configuration will be specified exclusively
on the controller.
During the intialization of a connection, the entire configuration file will be given to the
dashboard as a message.
This configuration file contains hardware indices for ADCs, calibration values, burn durations,
and similar.

## Example timeline

1. Controller and dashboard both start.
  The controller begins listening for an incoming connection on its TCP server.

1. User enters the IP address of the controller, and then presses "Connect to Controller" or similar
  button on dashboard.

1. Dashboard connects to the specified IP address for the controller.

1. Controller transmits a configuration message immediately.

1. Controller sends a series of status messages containing sensor data, and each is plotted on the
  dashboard.

1. User begins an ignition sequence.
  Ignition start message is sent to controller.

1. Controller completes ignition process.

## Configuration

A configuration file contains all the information necessary to set up an entire test.
The file will declare a family of sensors and drivers, and also outline the ignition procedure.
The fields of the main configuration object are as follows:

- `frequency_status` - number: The number of times (per second) to attempt to send driver status
  update messages.

- `log_buffer_size` - number: The size of each log buffer.
  When a log buffer is full, its data will be flushed into a log file.

- `sensor_groups` - array: A list describing each set of sensors and the threads that manage them.
It also includes calibration information.

- `pre_ignite_time` - number. The duration of the pre-ignition period in milliseconds.
  During pre-ignition, sensors log data at a high frequency, but the ignition procedure has not yet
  started.

- `post_ignite_time` - number. The duration of the post-ignition period in milliseconds.
  During post-ignition, sensors log data at a high frequency, but the ignition procedure has
  already ended.

- `drivers` - array: A list describing each driver, giving each a unique identifier (which will
  later be referred to during ignition).

- `ignition_sequence` - array: A list of objects describing each sequential operation to be taken
  during the ignition sequence.

- `estop_sequence` - array: A list of objects describing each sequential operation to be taken
  during the shutoff sequence.

### Drivers

Each driver is represented by an object in the `drivers` list.
It will have the following keys:

- `label` - string: A human-readable name for the driver.

- `pin` - int: The GPIO pin that the driver controls.
  Note that the GPIO pin is by software standards, and it is _not_ the phyiscal pinout on the
  Raspberry Pi.

- `protected` - bool: Whether the user of the dashboard can directly actuate this pin.
  If `false`, the user can only read the state of this driver, and the only way the driver can be
  actuated is via an ignition or emergency stop sequence.
  The ignition driver should always be protected.

### Sensors

Each sensor group (each being an element of the `sensor_groups` field) is an object with the
following fields:

- `label` - string: The name of the sensor group.
  May not be shared between two distinct sensor groups.

- `frequency_standby` - number: The number of times, per second, to sample all the sensors in the
  sensor group _outside_ of ingition procedures.

- `frequency_ignition` - number: The number of times, per second, to sample all the sensors in the
  sensor group during the ignition procedure.

- `frequency_transmission` - number: An upper bound on the number of times per second a sensor value
  update will be sent to the dashboard.
  If the transmission frequency is greater than the active sampling frequency (either standby or
  ignition), messages will be sent on a time scale according to how often they were sampled.

- `sensors` - array: The set of sensors. Each sensor will be an object containing the following
    keys:

  - `label` - string: The unique identifier for the sensor.
    May not be shared across sensor groups.

  - `color` - string: A color which can be used for displaying the sensor's value.

  - `units` - string: The units of the sensor's calibrated value.

  - `range` (optional) - array of numbers: The legal range which the calibrated sensor value can be
    during the ignition process.
    The rolling average value will be compared against the range.
    If the value is not within the range during ignition, then the ignition will immediately halt
    and emergency shutoff will begin.

  - `calibration_intercept` - number: The linear offset for calibrating the sensors.
    For a calibration scheme of type `y = mx + b`, `calibration_intercept` is `b`.

  - `calibration_slope` - number: The slope of the linear calibration for the sensors.
    For a calibration scheme of type `y = mx + b`, `calibration_slope` is `m`.

  - `rolling_average_width` number: The number of samples to be combined in a rolling average to be
    used for validating a range.
    The rolling average will not be used anywhere other than range validation, and will not be
    stored in the logs.
    The reason for this is because the rolling average is only needed to eliminate high-frequency
    noise from a range detection.

  - `adc` - number: the ID of the ADC (as specified in `adc_cs` of the root configuration object) to
    be used for measuring this sensor.

  - `channel` - number: the ADC channel which this sensor measures.

In the future, we may change the specification for calibrations to include non-affine calibrations.

### Ignition sequence

`ignition_sequence` maps to an array of objects which each identify one "step" in the ignition
process.
A step is an object, and has the following field:

- `type` - string: A string describing the operation to take on.
  The operation may be either `Actuate` or `Sleep`.

A `Sleep` operation has only one extra field, `duration`, which is an object with fields `secs` and
`nanos` describing the length of the duration in seconds and nanoseconds.

An `actuate` operation has two extra fields:

- `driver_id` - string: The identifier for the driver to be actuated.

- `value` - boolean: The logic level the driver should be actuated to (`true` for electrically
  powered and `false` for unpowered).

During the ignition procedure, the controller will execute each step in the ignition sequence
configuration in order.

### Emergency shutoff sequence

`estop_sequence` maps to an array of steps, just like `ignition_sequence`.
The steps that can be performed in a shutoff sequence are identical to those that can be performed
during ignition.

### Sample configuration

I wouldn't recommend using this configuration - the numbers are made up and possibly could cause
serious issues.
However, it makes the syntax and structure of a configuration apparent.

```json
{
  "frequency_status": 10,
  "log_buffer_size": 256,
  "sensor_groups": [
    {
      "label": "FAST",
      "frequency_standby": 10,
      "frequency_ignition": 1000,
      "frequency_transmission": 10,
      "sensors": [
        {
          "label": "LC_MAIN",
          "units": "lb",
          "calibration_intercept": 0.34,
          "calibration_slope": 33.2,
          "rolling_average_width": 5,
          "adc": 0,
          "channel": 0
        },
        {
          "label": "PT_FEED",
          "units": "psi",
          "range": [-500, 3000],
          "calibration_intercept": 92.3,
          "calibration_slope": -302.4,
          "rolling_average_width": 4,
          "adc": 0,
          "channel": 1
        }
      ]
    }
  ],
  "drivers": [
    {
      "label": "OXI_FILL",
      "pin": 33
    }
  ],
  "pre_ignite_time": 500,
  "post_ignite_time": 5000,
  "ignition_sequence": [
    {
      "type": "Actuate",
      "driver_id": 0,
      "value": true
    },
    {
      "type": "Sleep",
      "duration": {
        "secs": 10,
        "nanos": 0
      }
    },
    {
      "type": "Actuate",
      "driver_id": 0,
      "value": false
    }
  ],
  "estop_sequence": [
    {
      "type": "Actuate",
      "driver_id": 0,
      "value": false
    }
  ],
  "spi_mosi": 26,
  "spi_miso": 27,
  "spi_clk": 28,
  "spi_frequency_clk": 50000,
  "adc_cs": [37]
}
```

## Message specification

In the following section, the keys of each message will be given as a bullet point list, followed by
an example.
Every message, in either direction, must have the following keys:

- `type` - string: The identifier of the message types.

For example:

```json
{
  "type": "Foo"
  // other keys...
}
```

Each message can be separated by an arbitrary amount of whitespace.
For instance, the following would be a legal sequence of messages for the controller to receive from
the dashboard:

```json
{
    "type": "Actuate",
    "driver_id": 0,
    "value": true
}
{
    "type": "Ignition",
}
```

### Dashboard to controller

#### Driver actuation

All driver actuation messages will have the type `Actuate`.

- `driver_id` - number: The ID of the driver to be actuated.
  This ID is equal to the index of the driver in the original configuration object.

- `value` - boolean: The logic level that the driver should be actuated to.
  If `true`, the driver should be actuated to its electrically-powered level.
  If `false`, the driver should be deactuated to its unpowered level.
  If the driver level was already in the desired level, sending this message would result in a
  silent no-op.

```json
{
  "type": "Actuate",
  "driver_id": 0,
  "value": true
}
```

#### Ignition start

Inform the controller to begin an ignition immediately.
The controller will then actuate all valves according to the ignition procedure outlined in the
configuration setup.

```json
{
  "type": "Ignition"
}
```

#### Emergency stop

Inform the controller to emergency stop.
To execute an emergency stop, the controller will halt any ongoing ignition processes and then
immediately start the shutoff procedure outlined in the configuration.
If an ignition is not currently active, the controller will still execute the shutdown procedure.

```json
{
  "type": "EmergencyStop"
}
```

### Controller to dashboard

#### Configuration setup

A `Config` message is given at the start of the conversation, as soon as the dashboard connects to
the controller.
This transmits the entire contents of the configuration file as a field of the message.

- `config` - object. This object should be exactly equal to the configuration object which was used
  at startup.
  Please see the configuration section for more detailed examples on what this should look like.

```json
{
  "type": "Config",
  "config": // ...
}
```

#### Sensor value

A `SensorValue` message will be sent when the controller has a new set of sensor values to be
displayed and/or logged on the dashboard.
The data values are not guaranteed to be contemporaneous, or in order, but they will be all from the
same sensor group.

- `group_id` - number: The ID of the sensor group containing all sensors read for this message.
  This ID is the index of the sensor group in the original configuration object.

- `readings` - array: A sequence of objects describing readings for sensors.
  Each sensor has the following properties:

  - `sensor_id` - number: The ID of the sensor which created the reading.
    This ID is the index of the sensor in the sensor array in the original configuration object.

  - `reading` - number: The raw ADC reading of the sensor.

  - `time` - object: The time at which the reading was created.
    The time object will have the following properties:

    - `secs_since_epoch` - number. The number of seconds since the UNIX epoch when the reading was
      created.

    - `nanos_since_epoch` - number. The number of nanoseconds since the last second since the UNIX
      epoch.

```json
{
  "type": "SensorValue",
  "group_id": 0,
  "readings": [
    {
      "sensor_id": 0,
      "reading": 3456,
      "time": {
        "secs_since_epoch": 1651355351,
        "nanos_since_epoch": 534000000
      }
    }
  ]
}
```

#### Driver value

A `DriverValue` message will periodically sent to the dashboard at approximately the frequency
specified in the `frequency_status` field of the configuration.
It describes the current values of all the drivers.

- `values` - array. An array of booleans, each describing the logic level of one driver.
  Each index in the `values` array corresponds to the ID of each driver, which is also its index in
  the original configuration object's list of drivers.

```json
{
  "type": "DriverValue",
  "values": [false, true, false]
}
```
