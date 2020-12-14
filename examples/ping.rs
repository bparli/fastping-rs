extern crate fastping_rs;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use fastping_rs::PingResult::{Idle, Receive};
use fastping_rs::Pinger;

fn main() {
    pretty_env_logger::init();
    let (pinger, results) = match Pinger::new(None, Some(64)) {
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
