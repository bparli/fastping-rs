# fastping-rs
 ICMP ping library in Rust inspired by go-fastping and AnyEvent::FastPing Perl module

fastping-rs is a Rust ICMP ping library, inspired by [go-fastping](https://github.com/tatsushid/go-fastping)  and the [AnyEvent::FastPing Perl module](http://search.cpan.org/~mlehmann/AnyEvent-FastPing-2.01/), for quickly sending and measuring batches of ICMP ECHO REQUEST packets.

## Usage
`Pinger::new` returns a tuple containing the actual pinger, and the channel to listen for ping results on.  The ping results will either be a `PingResult::Receive` (if the ping response was received prior to the maximum allowed roud trip time) or a `PingResult::Idle` (if the response was not in time).

### run with example
```shell
git clone https://github.com/bparli/fastping-rs
cd fastping-rs
sudo RUST_LOG=info cargo run --example ping
```


# Example
Add some crate to your dependencies:
```toml
log = "0.4"
pretty_env_logger = "0.4"
fastping-rs = "0.2"
```

And then get started in your `main.rs`:
```rust
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use fastping_rs::PingResult::{Idle, Receive};
use fastping_rs::Pinger;

fn main() {
    pretty_env_logger::init();
    let (pinger, results) = match Pinger::new(None, Some(56)) {
        Ok((pinger, results)) => (pinger, results),
        Err(e) => panic!("Error creating pinger: {}", e),
    };

    pinger.add_ipaddr("8.8.8.8");
    pinger.add_ipaddr("1.1.1.1");
    pinger.add_ipaddr("7.7.7.7");
    pinger.add_ipaddr("2001:4860:4860::8888");
    pinger.run_pinger();

    loop {
        match results.recv() {
            Ok(result) => match result {
                Idle { addr } => {
                    error!("Idle Address {}.", addr);
                }
                Receive { addr, rtt } => {
                    info!("Receive from Address {} in {:?}.", addr, rtt);
                }
            },
            Err(_) => panic!("Worker threads disconnected before the solution was found!"),
        }
    }
}

```

Note a Pinger is initialized with two arguments: the maximum round trip time before an address is considered "idle" (2 seconds by default) and the size of the ping data packet (16 bytes by default).
To explicitly set these values Pinger would be initialized like so:
```rust
Pinger::new(Some(Duration::from_millis(3000)), Some(24 as usize))
```

The public functions `stop_pinger()` to stop the continuous pinger and `ping_once()` to only run one round of pinging are also available.

## Additional Notes
This library requires the ability to create raw sockets.  Either explicitly set for your program (`sudo setcap cap_net_raw=eip /usr/bin/testping` for example) or run as root.

Only supported on linux and osx for now (Windows will likely not work).  
