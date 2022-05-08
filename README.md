# RESFET Controller v2

## Motivation

The [previous RESFET controller software](https://github.com/rice-eclipse/resfet) kind of sucks. It's a whole load of spaghetti code. The goal for this project is to rewrite the controller to make it more configurable, easier to extend to support multiple engines, and generally just less of a pain to work with.

## API

The RESFET rewrite also includes a redesign for the API for communcations between the dashboard and controller. For further details, refer to [api.md](https://github.com/rice-eclipse/resfet-controller-2/blob/master/api.md).

## Installation and dependencies

RESFET Controller v2 is written in Rust, and uses Cargo, the main Rust build system, to build and test. We recommend using [rustup](https://rustup.rs) to set up the Rust build environment. We will list any dependencies we add here (such as for C interoperation).

## Building

To build the release version of the code, navigate to the root directory of this repository and enter `cargo build --release`. To run the controller, either run `cargo run --release` or `./target/release/resfet_controller_2`.

To run all tests, run `cargo test`.
