# Introduction

A silly webhook for Twitch events. 🦀


### Building

robochick is written in Rust, so you'll need the latest installation of Rust (1.90.0 on my machine).

```
git clone https://github.com/srujan964/robochick-rs
cd robochick-rs
cargo build --release
```

The release profile builds robochick specifically to run on AWS Lambda. The dev build uses axum to bind to `127.0.0.1:3000` in order to allow for easier dev testing.

The following tools are optional:

- cargo lambda (to cross compile to arm64 a bit more easily)
- mountebank (local mock server and stub responses)

Running the mountebank mocks on port 3696:

```
mb start --configfile mocks/imposters.ejs --allowInjection
```
