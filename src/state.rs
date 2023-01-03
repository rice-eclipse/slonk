use std::sync::RwLock;

#[derive(Debug)]
/// The set of errors that can be caused from working with a `Guard`.
pub enum Error {
    /// The guard's lock was poisoned. This implies a panicked thread owned a lock.
    Poison,
    /// An illegal transition was attempted.
    IllegalTransition {
        /// The state that the transition was attempted from.
        from: State,
        /// The state that the transistion was attempted into.
        to: State,
    },
}

/// A guard for controller state which can be used to notify other threads of changes to controller
/// state.
pub struct Guard {
    /// The current state.
    state: RwLock<State>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// The set of all states the engine controller can be in.
pub enum State {
    /// The engine is in standby - passively logging and awating commands.
    /// This state can only be reached from the `PostIgnite` and `EStopping` states.
    Standby,
    /// The engine is preparing to ignite in the upcoming seconds.
    /// This state can only be reached from the `Standby` state.
    /// During this phase, data logging threads should increase their logging speed.
    PreIgnite,
    /// The engine is currently ignited.
    /// This state can only be reached from the `Standby` state.
    /// Data logging should be fast here.
    Ignite,
    /// The engine is not currently ignited, but was igniting recently.
    /// This state can only be reached from the `Ignite` state.
    /// Since interesting things might still happen in the post-ignition phase, data logging should
    /// still be fast here.
    PostIgnite,
    /// An emergency stop command has just been called.
    /// This state is reachable from any other state except the `Quit` state.
    /// Data logging should be fast, since anything that is worth e-stopping over is probably very
    /// interesting.
    EStopping,
    /// The engine controller is shutting down.
    /// This state can only be reached from the `Standby` state.
    /// During this state, each thread will "wrap up" its work and then exit as soon as possible.
    Quit,
}

impl Guard {
    #[must_use]
    /// Construct a new `Guard`.
    /// Initializes its state to the value of `state`.
    pub fn new(state: State) -> Guard {
        Guard {
            state: RwLock::new(state),
        }
    }

    /// Get the status of this guard.
    /// This operation is blocking.
    ///
    /// # Errors
    ///
    /// Will return an error in the case that the internal lock of this guard is poisoned.
    pub fn status(&self) -> Result<State, Error> {
        match self.state.read() {
            Ok(s) => Ok(*s),
            Err(_) => Err(Error::Poison),
        }
    }

    /// Move this guard into a new state.
    ///
    /// # Errors
    ///
    /// This function will return an `Err(ControllerError::Poison)` in the case that an internal
    /// lock is poisoned.
    /// If `new_state` is not reachable from the current state, an
    /// `Err(ControllerError::IllegalTransition)` will be returned.
    pub fn move_to(&self, new_state: State) -> Result<(), Error> {
        let mut write_guard = self.state.write().map_err(|_| Error::Poison)?;
        let old_state = *write_guard;

        // determine whether the transition is valid
        let valid_transition = match new_state {
            State::Standby => old_state == State::EStopping || old_state == State::PostIgnite,
            State::PreIgnite | State::Quit => old_state == State::Standby,
            State::Ignite => old_state == State::PreIgnite,
            State::PostIgnite => old_state == State::Ignite,
            State::EStopping => old_state != State::Quit,
        };

        if !valid_transition {
            return Err(Error::IllegalTransition {
                from: old_state,
                to: new_state,
            });
        }

        *write_guard = new_state;
        Ok(())
    }
}
