use crate::{
    hardware::GpioPin,
    state::{Guard, State},
    ControllerError,
};

use std::{thread::sleep, time::Duration};

/// Perform the heartbeat thread for the controller.
/// This will alternate the output value on `pin` for as long as the server is running.
pub fn heartbeat(pin: &mut impl GpioPin, state: &Guard) -> Result<(), ControllerError> {
    while state.status()? != State::Quit {
        pin.write(true)?;
        sleep(Duration::from_millis(50));
        pin.write(false)?;
        sleep(Duration::from_millis(50));
        pin.write(true)?;
        sleep(Duration::from_millis(50));
        pin.write(false)?;
        sleep(Duration::from_millis(850));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::thread::scope;

    use crate::hardware::ListenerPin;

    use super::*;

    #[test]
    fn heartbeat_write() {
        let mut pin = ListenerPin::new(false);
        let guard = Guard::new(State::Standby);

        scope(|s| {
            s.spawn(|| heartbeat(&mut pin, &guard));

            sleep(Duration::from_millis(200));
            guard.move_to(State::Quit).unwrap();
        });

        assert_eq!(pin.history(), &vec![false, true, false, true, false]);
    }
}
