extern crate fastping_rs;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use std::error::Error;

use fastping_rs::PingResult::{Idle, Receive};
use fastping_rs::Pinger;

fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();
    let (pinger, results) = match Pinger::new(None, Some(64)) {
        Ok((pinger, results)) => (pinger, results),
        Err(e) => panic!("Error creating pinger: {}", e),
    };

    pinger.add_ipaddr("8.8.8.8".parse()?);
    pinger.add_ipaddr("1.1.1.1".parse()?);
    pinger.add_ipaddr("7.7.7.7".parse()?);
    pinger.add_ipaddr("2001:4860:4860::8888".parse()?);
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
