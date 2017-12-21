# cargo-contribute

[![crates.io](https://img.shields.io/crates/v/cargo-contribute.svg)](https://crates.io/crates/cargo-contribute)
[![Build Status](https://travis-ci.org/Xion/cargo-contribute.svg?branch=master)](https://travis-ci.org/Xion/cargo-contribute)

A cargo subcommand for contributing to development of your dependencies

## About

Want to give back to authors of the useful crates you are depending on in your projects?
With `cargo-contribute`, you will find some easy ways to do just that!

When run against a Rust project, `cargo-contribute` will find its immediate dependencies,
check their GitHub repositories, and look for unassigned issues that their maintainers are looking for help with.

Here's a sample:

    $ cargo contribute
    [kbknapp/clap-rs] #1094: -h, --help generate trailing spaces -- https://github.com/kbknapp/clap-rs/issues/1094
    [bluss/rust-itertools] #236: Forward `fn collect()` everywhere it is possible and where it makes a difference -- https://github.com/bluss/rust-itertools/issues/236
    [kbknapp/clap-rs] #1078: Dedupe Tests -- https://github.com/kbknapp/clap-rs/issues/1078
    [bluss/rust-itertools] #92: Group by that merges same key elements -- https://github.com/bluss/rust-itertools/issues/92
    [kbknapp/clap-rs] #1073: suboptimal flag suggestion -- https://github.com/kbknapp/clap-issues/rs/1073
    [bluss/rust-itertools] #32: Add Debug implementations where possible -- https://github.com/bluss/rust-itertools/issues/32
    [kbknapp/clap-rs] #850: zsh completion is too strict on command line args -- https://github.com/kbknapp/clap-rs/issues/850
    [dtolnay/isatty] #1: Implement stdin_isatty() for Windows -- https://github.com/dtolnay/isatty/issues/1

## Installation

`cargo-contribute` can be installed with `cargo install`:

    $ cargo install cargo-contribute

This shall put the `cargo-contribute` executable in your Cargo binary directory
(e.g. `~/.cargo/bin`) -- which hopefully is in your `$PATH` -- and make it accessible as Cargo subcommand.

## Usage

By default, `cargo-contribute` will suggest _all_ suitable issues filed against the direct dependencies
of your project. You can limit their number using the `-n`/`--count` flag:

    $ cargo contribute -n 3
    [bluss/rust-itertools] #236: Forward `fn collect()` everywhere it is possible and where it makes a difference -- https://github.com/bluss/rust-itertools/issues/236
    [bluss/rust-itertools] #92: Group by that merges same key elements -- https://github.com/bluss/rust-itertools/issues/92
    [bluss/rust-itertools] #32: Add Debug implementations where possible -- https://github.com/bluss/rust-itertools/issues/32

Additionally, you can tweak how each issue is printed out by using the `--format`/`-T` flag.
It accepts a standard Rust `format!()` string:

    $ cargo contribute --format '{url}'
    https://github.com/kbknapp/clap-rs/issues/1094
    https://github.com/bluss/rust-itertools/issues/236
    https://github.com/kbknapp/clap-rs/issues/1078
    https://github.com/bluss/rust-itertools/issues/92
    https://github.com/kbknapp/clap-issues/rs/1073
    https://github.com/bluss/rust-itertools/issues/32
    https://github.com/kbknapp/clap-rs/issues/850
    https://github.com/dtolnay/isatty/issues/1

It is also possible to provide your own [personal access token](https://github.com/settings/tokens)
to use when making calls to GitHub API.
This helps to avoid the (pretty strict) rate limits that are imposed on anonymous calls:

    $ cargo contribute --github-token XXXXXXXXXXXXXX

For more detailed usage instructions, check `cargo contribute --help`.

## License

`cargo-contribute` is licensed under the terms of the GPLv3 license.
