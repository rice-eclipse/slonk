# RESFET Rewrite API Proposal

Since we're rewriting the engine controller, we might as well rewrite the API. This proposal outlines a new API for RESFET's replacement, moving away from the RESFET's opaque, implementation-dependent approach.

## Terms

* *Dashboard*: the device running RESFET Dashboard. Acts as a client to the controller.

* *Controller*: the device running the replacement for RESFET Engine Controller, or whatever we call it.

* *Sensor*: For the sake of this API, a "sensor" is anything that can be read from an ADC and has a calibration. If, in the future, we decide to interface with sensors by means other than an ADC, we may change this definition (and, accordingly, how the configuration setup works).

* *Sensor group*: A sensor group is a set of sensors which are sampled at the same rate and will be handled by the same thread to reduce overhead.

## At a glance

Communication is done between the dashboard and controller by two TCP streams (as opposed to UDP, as before). The key reason is that there's little performance justification for just randomly losing datapoints in the stream. Across these channels, both the controller and dashboard can send messages to each other.

Messages traveling in either direction will be formatted using JSON. The overarching structure of the messages will be the same across both directions, and at the top level should be a mapping containing keys with timestamps, message types, and potentially other debug information.

Configuration files will also be formatted with JSON. To avoid having users use mismatched configurations, the configuration will be specified exclusively on the controller. During the intialization of a connection, the entire configuration file will be given to the dashboard as a message. This configuration file would contain hardware indices for ADCs, calibration values, burn durations, and similar.

## Example timeline

1. Controller and dashboard both start, and begin listening for an incoming connection on their respective TCP servers.

1. User enters the IP address of the controller, and then presses "Connect to Controller" or similar button on dashboard.

1. Dashboard connects to the specified IP address for the controller

1. Controller connects to the dashboard, using the incoming connection as the source for which IP to use. Controller then transmits a configuration message immediately.

1. Controller sends a series of status messages containing sensor data, and each is plotted on the dashboard.

1. User begins an ignition sequence. Ignition start message is sent to controller.

1. Controller completes ignition process.

## Configuration

A configuration file contains all the information necessary to set up an entire test. The file will declare a family of sensors and drivers, and also outline the ignition procedure. The fields of the main configuration object are as follows:

* `drivers` - array: A list describing each driver, giving each a unique identifier (which will later be referred to during ignition).

* `driver_status_frequency` - number: The number of times (per second) to attempt to send driver status update messages.

* `sensor_groups` - array: A list describing each set of sensors and the threads that manage them. It will also include calibration information.

* `ignition_sequence` - array: A list of objects describing each sequential operation to be taken during the ignition sequence.

* `shutoff_sequence` - array: A list of objects describing each sequential operation to be taken during the shutoff sequence.

### Sensors

Each sensor group (each being an element of the `sensor_groups` field) is an object with the following fields:

* `name` - string: The name of the sensor group. May not be shared between two distinct sensor groups.

* `standby_frequency` - number: The number of times, per second, to sample all the sensors in the sensor group *outside* of ingition procedures.

* `ignition_frequency` - number: The number of times, per second, to sample all the sensors in the sensor group during the ignition procedure.

* `transmission_frequency` - number: An upper bound on the number of times per second a sensor value update will be sent to the dashboard. If the transmission frequency is greater than the active sampling frequency (either standby or ignition), messages will be sent on a time scale according to how often they were sampled.

* `sensors` - array: The set of sensors. Each sensor will be an object containing the following keys:

  * `id` - string: The unique identifier for the sensor. May not be shared across sensor groups.

  * `calibration_intercept` - number: The linear offset for calibrating the sensors. For a calibration scheme of type `y = mx + b`, `calibration_intercept` is `b`.

  * `calibration_slope` - number: The slope of the linear calibration for the sensors. For a calibration scheme of type `y = mx + b`, `calibration_slope` is `m`.

  * `units` - string: The units of the sensor's calibrated value.

  * `range` (optional) - array of numbers: The legal range which the calibrated sensor value can be during the ignition process. If the field `rolling_average_width` is given, the rolling average value will be compared against the range. If the value is not within the range during ignition, then the ignition will immediately halt and the shutoff will begin.

  * `rolling_average_width` (optional) - number: The number of samples to be combined in a rolling average to be used for validating a range. The rolling average will not be used anywhere other than range validation, and will not be stored in the logs. The reason for this is because the rolling average is only needed to eliminate high-frequency noise from a range detection.

*Up for discussion*: allow for different calibration modes than just a linear one?

### Ignition sequence

`ignition_sequence` maps to an array of objects which each identify one "step" in the ignition process. A step is an object, and has the following field:

* `operation` - string: A string describing the operation to take on. The operation may be either `actuate` or `sleep`.

A `sleep` operation has only one extra field, `duration`, which is the number in milliseconds of how long to delay.

An `actuate` operation has two extra fields:

* `driver_id` - string: The identifier for the driver to be actuated.

* `state` - boolean: The state the driver should be actuated to (`true` for electrically powered and `false` for unpowered).

During the ignition procedure, the controller will execute each step in the ignition sequence configuration in order.

### Emergency shutoff sequence

`shutoff_sequence` maps to an array of steps, just like `ignition_sequence`. The steps that can be performed in a shutoff sequence are identical to those that can be performed during ignition.

### Sample configuration

I wouldn't recommend using this configuration - the numbers are made up and possibly could cause serious issues. However, it makes the syntax and structure of a configuration apparent.

```json
{
    "drivers": [
        {
            "id": "OXI_FILL",
            "default_on": false,
            "pin": 33
        },
        {
            "id": "IGNITION",
            "default_on": false,
            "pin": 36
        }
    ],
    "driver_status_frequency": 10,
    "sensor_groups": [
        {
            "name": "FAST",
            "standby_frequency": 200,
            "ignition_frequency": 5000,
            "transmission_frequency": 100,
            "sensors": [
                {
                    "id": "LC_MAIN",
                    "calibration_intercept": -304.38,
                    "calibration_slope": 0.4321,
                    "units": "lb"
                },
                {
                    "id": "PT_COMB",
                    "calibration_intercept": 1158.6,
                    "calibration_slope": -0.3113,
                    "units": "psi",
                    "range": [0, 900],
                    "rolling_average_width": 15
                }
            ]
        },
        {
            "name": "SLOW",
            "standby_frequency": 10,
            "ignition_frequency": 100,
            "transmission_frequency": 5,
            "sensors": [
                {
                    "id": "TC_COMB",
                    "calibration_intercept": -304.38,
                    "calibration_slope": 0.4321,
                    "units": "lb"
                },
                {
                    "id": "PT_INJE",
                    "calibration_intercept": 1158.6,
                    "calibration_slope": -0.3113,
                    "units": "psi"
                }
            ]
        },
    ],
    "ignition_sequence": [
        {
            "operation": "actuate",
            "driver_id": "OXI_FILL",
            "state": true
        },
        {
            "operation": "actuate",
            "driver_id": "IGNITION",
            "state": true
        },
        {
            "operation": "sleep",
            "duration": 10000
        },
        {
            "operation": "actuate",
            "driver_id": "OXI_FILL",
            "state": false
        },
        {
            "operation": "actuate",
            "driver_id": "IGNITION",
            "state": false
        }
    ],
    "shutoff_sequence": [
        {
            "operation": "actuate",
            "driver_id": "OXI_FILL",
            "state": false
        },
        {
            "operation": "actuate",
            "driver_id": "IGNITION",
            "state": false
        }
    ]
}
```

## Message specification

In the following section, the keys of each message will be given as a bullet point list, followed by an example. Every message, in either direction, must have the following keys:

* `message_type` - string: The identifier of the message types. Message types may not be aliased across directions (so a message of type `foo` must have the same format when from the dashboard as to when it is sent from the controller.).

* `send_time` - number: The total number of milliseconds elapsed from the UNIX epoch at the time of sending. For instance, approximately at the time of writing the number of milliseconds was 1651355351791.

For example:

```json
{
    "message_type": "foo",
    "send_time": 1651355351791,
    // other keys...
}
```

Each message can be separated by an arbitrary amount of whitespace. For instance, the following would be a legal sequence of messages for the controller to receive from the dashboard:

```json
{
    "message_type": "actuate",
    "send_time": 1651355351791,
    "driver_id": "OXI_FILL",
    "state": true
}
{
    "message_type": "ignition",
    "send_time": 1651355351791
}
```

### Dashboard to controller

Messages from the dashboard to the controller may or may not be processed sequentially. For safety reasons (accepting an emergency stop during an active message), each message received will receive its own thread to process it.

#### Ready

A `ready` message is sent immediately after the controller has fully parsed a `configuration` message and is ready to accept new messages from the controller. The `ready` message has no extra fields.

```json
{
    "message_type": "ready",
    "send_time": 1651355351791
}
```

#### Driver actuation

All driver actuation messages will have the type `actuate`.

* `driver_id` - string: The unique string identifier of the driver. For example, it could be "OXI_FILL".

* `state` - boolean: If `true`, the driver should be actuated to its electrically-powered state. If `false`, the driver should be deactuated to its unpowered state. If the driver state was already in the desired state, sending this message would result in a silent no-op.

```json
{
    "message_type": "actuate",
    "send_time": 1651355351791,
    "driver_id": "OXI_FILL",
    "state": true
}
```

#### Ignition start

Inform the controller to begin an ignition immediately. The controller will then actuate all valves according to the ignition procedure outlined in the configuration setup.

```json
{
    "message_type": "ignition",
    "send_time": 1651355351791
}
```

#### Emergency stop

Inform the controller to emergency stop. To execute an emergency stop, the controller will halt any ongoing ignition processes and then immediately start the shutoff procedure outlined in the configuration. If an ignition is not currently active, the controller will still execute the shutdown procedure.

```json
{
    "message_type": "emergency_stop",
    "send_time": 1651355351791
}
```

### Controller to dashboard

#### Configuration setup

A `configuration` message is given at the start of the conversation, as soon as the dashboard connects to the controller. This transmits the entire contents of the configuration file as a field of the message.

* `config` - object. This object should be exactly equal to the configuration object which was used at startup. Please see the configuration section for more detailed examples on what this should look like.

#### Sensor value

A `sensor_value` message will be sent when the controller has a new set of sensor values to be displayed and/or logged on the dashboard. The data values are not guaranteed to be contemporaneous, in order, or from the same sensor group.

* `data` - object: Each key in `data` corresponds to one sensor channel. The value of each key will individually be an array of objects, with the following keys:

  * `time` - number: The number of milliseconds since the UNIX epoch when the datum was collected.

  * `adc` - number: The raw ADC value of the sensor channel. To convert to natural units, the dashboard must use the calibration it gave in the configuration. The reason for this is because it seems unwise to transmit floats over a channel.

```json
{
    "message_type": "sensor_value",
    "send_time": 1651355351791,
    "data": {
        "PT_FEED": {
            "time": 1651355351534,
            "adc": 3456
        },
        "LC_MAIN": {
            "time": 1651355351462,
            "adc": 125
        }
    }
}
```

#### Driver value

A `driver_value` message will periodically sent to the dashboard at approximately the frequency specified in the `driver_status_frequency` field of the configuration. It describes the current state of all the drivers.

* `state` - object. A mapping from each driver's identifier to their current state, with `false` being unpowered and `true` being powered.

```json
{
    "message_type": "driver_value",
    "send_time": 1651355351791,
    "state": {
        "OXI_FILL": false,
        "ENGI_VENT": true,
        "IGNITION": false
    }
}
```

#### Display

A `display` message will be sent whenever the controller wishes to display a message to the user on the dashboard.

* `message` - string: The message which will be displayed to the user.

```json
{
    "message_type": "display",
    "send_time": 3133675200,
    "message": "The weather today is expected to be mostly sunny, with a high of 73 degrees Fahrenheit."
}
```

#### Error

An `error` message is sent to describe an error occurring on the controller. These errors may or may not be recoverable.

* `cause` - string: The type of cause of the error. The subsections of this group display the possible causes, and the according keys that will be added.

* `diagnostic` - string: A human-readable error message that can be displayed to the user.

```json
{
    "message_type": "error",
    "send_time": 1651355351791,
    "cause": "your error here",
    "diagnostic": "this is a placeholder",
    // other keys related to this error below...
}
```

Error message specifications below will list the other keys added to `body` dependent on context.

##### Malformed

A `malformed` message will add the following keys:

* `original_message` - string: The original message sent which caused the malformed error.

```json
{
    "message_type": "error",
    "send_time": 1651355351791,
    "cause": "malformed",
    "diagnostic": "expected key `driver_id` not found",
    "original_message": "{\"message_type\": \"actuate\",\"send_time\": 165135535000}"
}
```

##### Failed sensor read

A `sensor_fail` error is a failed sensor read, likely due to a hardware mismatch, such as if an ADC is not physically connected to the controller. We may assume that these messages are only ever sent from the controller to the dashboard.

* `sensor_id` - string: the identifier for the sensor which failed to read.

```json
{
    "message_type": "error",
    "send_time": 1651355351791,
    "cause": "sensor_fail",
    "diagnostic": "SPI transfer for LC_MAIN failed",
    "sensor_id": "LC_MAIN"
}
```

##### Permission

A `permission` error is caused by a failure to acquire permission to take on some action (such as reading from a file or interacting with hardware). Currently, the `permission` error has no other keys (but might if we run into things worth logging during implementation).

```json
{
    "message_type": "error",
    "send_time": 1651355351791,
    "cause": "permission",
    "diagnostic": "could not write to log file `log_LC_MAIN.txt`"
}
```
