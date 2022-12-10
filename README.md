# `slonk`: a new engine controller software

## Motivation

The
[previous RESFET controller software](https://github.com/rice-eclipse/resfet)
kind of sucks.
It's a whole load of spaghetti code.
The goal for this project is to rewrite the controller to make it more
configurable, easier to extend to support multiple engines, and generally just
less of a pain to work with.

## API

`slonk` also includes a redesign for the API for communcations between the dashboard and controller.
For further details, refer to
[api.md](https://github.com/rice-eclipse/slonk/blob/master/api.md).

This documnent also explains the structure of configuration files.

## Installation and dependencies

`slonk` is written in Rust, and uses Cargo, the main Rust build system, to build and test.
We recommend using [rustup](https://rustup.rs) to set up the Rust build environment.

## Building

To build the release version of the code, navigate to the root directory of this repository and 
enter `cargo build --release`.
To run the controller, run `./target/release/slonk` after building.

`slonk` must be run as root (via `sudo`) in order to take ownership of GPIO. 

The controller executable takes two arguments:

1. A path to the configuration JSON file.
1. A path to the directory where logs will be stored.

For example, the following command would run the engine controller for the Titan motor configuration 
and store logs in `../slogs/my_test_logs`.

```sh
cargo build --release
sudo ./target/release/slonk config/titan.json ../slogs/my_test_logs
```

To run all tests, run `cargo test`.

## Standard Git Procedures

To reduce chances of version control blunders, we've created standard git procedures.
Refer to [git_procedures.md](https://github.com/rice-eclipse/slonk/blob/master/git_procedures.md)
for more details.
