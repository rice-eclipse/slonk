#![warn(clippy::pedantic)]

#[allow(dead_code)]
/// The set of all states the engine controller can be in.
pub enum ControllerState {
    /// The engine is in standby - passively logging and awating commands.
    /// This state can only be reached from the `PostIgnite` and `EStopping`
    /// states.
    Standby,
    /// The engine is preparing to ignite in the upcoming seconds.
    /// This state can only be reached from the `Standby` state.
    /// During this phase, data logging threads should increase their logging
    /// speed.
    PreIgnite,
    /// The engine is currently ignited.
    /// This state can only be reached from the `Standby` state.
    /// Data logging should be fast here.
    Ignite,
    /// The engine is not currently ignited, but was igniting recently.
    /// This state can only be reached from the `Ignite` state.
    /// Since interesting things might still happen in the post-ignition phase,
    /// data logging should still be fast here.
    PostIgnite,
    /// An emergency stop command has just been called.
    /// This state is reachable from any other state.
    /// Data logging should be fast, since anything that is worth e-stopping
    /// over is probably very interesting.
    EStopping,
}
