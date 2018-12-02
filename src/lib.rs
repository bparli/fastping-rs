extern crate pnet;
extern crate pnet_macros_support;
#[macro_use]
extern crate log;
extern crate rand;

mod ping;

use pnet::packet::ip::IpNextHeaderProtocols;
use pnet::transport::{icmp_packet_iter, icmpv6_packet_iter};
use pnet::transport::transport_channel;
use pnet::transport::{TransportSender, TransportReceiver};
use pnet::transport::TransportChannelType::Layer4;
use pnet::transport::TransportProtocol::{Ipv4, Ipv6};
use std::net::{IpAddr};
use ::ping::{send_echo, send_echov6};
use std::time::{Duration, Instant};
use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use std::sync::{Arc, Mutex, RwLock};

// ping result type.  Idle represents pings that have not received a repsonse within the max_rtt.
// Receive represents pings which have received a repsonse
pub enum PingResult {
    Idle{addr: IpAddr},
    Receive{addr: IpAddr, rtt: Duration},
}

// result type returned by fastping_rs::Pinger::new()
pub type NewPingerResult = Result<(Pinger, Receiver<PingResult>), String>;

pub struct Pinger {
    // Number of milliseconds of an idle timeout. Once it passed,
	// the library calls an idle callback function.  Default is 2000
    max_rtt: Duration,

    // map of addresses to ping on each run
    addrs: BTreeMap<IpAddr, bool>,

    // Size in bytes of the payload to send.  Default is 16 bytes
    size: i32,

    // sender end of the channel for piping results to client
    results_sender: Sender<PingResult>,

    // sender end of libpnet icmp v4 transport channel
    tx: TransportSender,

    // receiver end of libpnet icmp v4 transport channel
    rx: Arc<Mutex<TransportReceiver>>,

    // sender end of libpnet icmp v6 transport channel
    txv6: TransportSender,

    // receiver end of libpnet icmp v6 transport channel
    rxv6: Arc<Mutex<TransportReceiver>>,

    // sender for internal result passing beween threads
    thread_tx: Sender<PingResult>,

    // receiver for internal result passing beween threads
    thread_rx: Receiver<PingResult>,

    // timer for tracking round trip times
    timer: Arc<RwLock<Instant>>,

    // flag to stop pinging
    stop: bool,
}

impl Pinger {
    // initialize the pinger and start the icmp and icmpv6 listeners
    pub fn new(_max_rtt: Option<u64>, _size: Option<i32>) -> NewPingerResult {
        let addrs = BTreeMap::new();
        let (sender, receiver) = channel();

        let protocol = Layer4(Ipv4(IpNextHeaderProtocols::Icmp));
        let (tx, rx) = match transport_channel(4096, protocol) {
            Ok((tx, rx)) => (tx, rx),
            Err(e) => return Err(e.to_string()),
        };

        let protocolv6 = Layer4(Ipv6(IpNextHeaderProtocols::Icmpv6));
        let (txv6, rxv6) = match transport_channel(4096, protocolv6) {
            Ok((txv6, rxv6)) => (txv6, rxv6),
            Err(e) => return Err(e.to_string()),
        };

        let (thread_tx, thread_rx) = channel();

        let mut pinger = Pinger{
            max_rtt: Duration::from_millis(2000),
            addrs: addrs,
            size: 16,
            results_sender: sender,
            tx: tx,
            rx: Arc::new(Mutex::new(rx)),
            txv6: txv6,
            rxv6: Arc::new(Mutex::new(rxv6)),
            thread_rx: thread_rx,
            thread_tx: thread_tx,
            timer: Arc::new(RwLock::new(Instant::now())),
            stop: false,
        };
        if let Some(rtt_value) = _max_rtt {
            pinger.max_rtt = Duration::from_millis(rtt_value);
        }
        if let Some(size_value) = _size {
            pinger.size = size_value;
        }

        pinger.start_listener();
        Ok((pinger, receiver))
    }

    // add either an ipv4 or ipv6 target address for pinging
    pub fn add_ipaddr(&mut self, ipaddr: &str) {
        let addr = ipaddr.parse::<IpAddr>();
        match addr {
            Ok(valid_addr) => {
                debug!("Address added {}", valid_addr);
                self.addrs.insert(valid_addr, true);
            }
            Err(e) => {
                error!("Error adding ip address {}. Error: {}", ipaddr, e);
            },
        };
    }

    // remove a previously added ipv4 or ipv6 target address
    pub fn remove_ipaddr(&mut self, ipaddr: &str) {
        let addr = ipaddr.parse::<IpAddr>();
        match addr {
            Ok(valid_addr) => {
                debug!("Address removed {}", valid_addr);
                self.addrs.remove(&valid_addr);
            }
            Err(e) => {
                error!("Error removing ip address {}. Error: {}", ipaddr, e);
            },
        };
    }

    // stop running the continous pinger
    pub fn stop_pinger(&mut self) {
        self.stop = true;
    }

    // run one round of pinging and stop
    pub fn ping_once(self) {
        self.pings(true);
    }

    // run the continuous pinger
    pub fn run_pinger(self) {
        thread::spawn(move ||{
            self.pings(false);
        });
    }

    fn pings(mut self, run_once: bool) {
        loop {
            for (addr, seen) in self.addrs.iter_mut() {
                if addr.is_ipv4() {
                    send_echo(&mut self.tx, *addr);
                } else if addr.is_ipv6() {
                    send_echov6(&mut self.txv6, *addr);
                }
                *seen = false;
            }
            {
                // start the timer
                let mut timer = self.timer.write().unwrap();
                *timer = Instant::now();
            }

            loop {
                match self.thread_rx.try_recv() {
                    Ok(result) => {
                        match result {
                            PingResult::Receive{addr, rtt: _} => {
                                // Update the address to the ping response being received
                                if let Some(seen) = self.addrs.get_mut(&addr) {
                                    *seen = true;
                                }
                                // Send the ping result over the client channel
                                match self.results_sender.send(result) {
                                    Ok(_) => {},
                                    Err(e) => {
                                        error!("Error sending ping result on channel: {}", e)
                                    }
                                }
                            }
                            _ => {}
                        }
                    },
                    Err(_) => {
                        // Check we haven't exceeded the max rtt
                        let start_time = self.timer.read().unwrap();
                        if Instant::now().duration_since(*start_time) > self.max_rtt {
                            break
                        }
                    }
                }
            }
            // check for addresses which haven't replied
            for (addr, seen) in self.addrs.iter() {
                if *seen == false {
                    // Send the ping Idle over the client channel
                    match self.results_sender.send(PingResult::Idle{addr: *addr}) {
                        Ok(_) => {},
                        Err(e) => {
                            error!("Error sending ping Idle result on channel: {}", e)
                        }
                    }
                }
            }
            // check if we've received the stop signal
            if self.stop || run_once {
                return
            }
        }
    }

    fn start_listener(&mut self) {
        // start icmp listeners in the background and use internal channels for results

        // setup ipv4 listener
        let thread_tx = self.thread_tx.clone();
        let rx = self.rx.clone();
        let timer = self.timer.clone();

        thread::spawn(move || {
            let mut receiver = rx.lock().unwrap();
            let mut iter = icmp_packet_iter(&mut receiver);
            loop {
                match iter.next() {
                    Ok((_, addr)) => {
                        let start_time = timer.read().unwrap();
                        match thread_tx.send(PingResult::Receive{addr: addr, rtt: Instant::now().duration_since(*start_time)}) {
                            Ok(_) => {},
                            Err(e) => {
                                error!("Error sending ping result on channel: {}", e)
                            }
                        }
                    },
                    Err(e) => {
                        error!("An error occurred while reading: {}", e);
                    }
                }
            }
        });

        // setup ipv6 listener
        let thread_txv6 = self.thread_tx.clone();
        let rxv6 = self.rxv6.clone();
        let timerv6 = self.timer.clone();
        thread::spawn(move || {
            let mut receiver = rxv6.lock().unwrap();
            let mut iter = icmpv6_packet_iter(&mut receiver);
            loop {
                match iter.next() {
                    Ok((_, addr)) => {
                        let start_time = timerv6.read().unwrap();
                        match thread_txv6.send(PingResult::Receive{addr: addr, rtt: Instant::now().duration_since(*start_time)}) {
                            Ok(_) => {},
                            Err(e) => {
                                error!("Error sending ping result on channel: {}", e)
                            }
                        }
                    },
                    Err(e) => {
                        error!("An error occurred while reading: {}", e);
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_newpinger() {
        // test we can create a new pinger with optional arguments,
        // test it returns the new pinger and a client channel
        // test we can use the client channel
        match Pinger::new(Some(3000 as u64), Some(24 as i32)) {
            Ok((test_pinger, test_channel)) => {
                assert_eq!(test_pinger.max_rtt, Duration::new(3, 0));
                assert_eq!(test_pinger.size, 24 as i32);

                match test_pinger.results_sender.send(PingResult::Idle{addr: "127.0.0.1".parse::<IpAddr>().unwrap()}) {
                    Ok(_) => {
                        match test_channel.recv() {
                            Ok(result) => {
                                match result {
                                    PingResult::Idle{addr} => {
                                        assert_eq!(addr, "127.0.0.1".parse::<IpAddr>().unwrap());
                                    },
                                    _ => {}
                                }
                            },
                            Err(_) => assert!(false),
                        }
                    }
                    Err(_) => assert!(false)
                }
            },
            Err(e) => {
                println!("Test failed: {}", e);
                assert!(false)
            }
        };
    }

    #[test]
    fn test_add_remove_addrs() {
        match Pinger::new(None, None) {
            Ok((mut test_pinger, _)) => {
                test_pinger.add_ipaddr("127.0.0.1");
                assert_eq!(test_pinger.addrs.len(), 1);
                assert!(test_pinger.addrs.contains_key(&"127.0.0.1".parse::<IpAddr>().unwrap()));

                test_pinger.remove_ipaddr("127.0.0.1");
                assert_eq!(test_pinger.addrs.len(), 0);
                assert_eq!(test_pinger.addrs.contains_key(&"127.0.0.1".parse::<IpAddr>().unwrap()), false);
            }
            Err(e) => {
                println!("Test failed: {}", e);
                assert!(false)
            }
        }
    }

    #[test]
    fn test_stop() {
        match Pinger::new(None, None) {
            Ok((mut test_pinger, _)) => {
                assert_eq!(test_pinger.stop, false);
                test_pinger.stop_pinger();
                assert_eq!(test_pinger.stop, true);
            }
            Err(e) => {
                println!("Test failed: {}", e);
                assert!(false)
            }
        }
    }

    #[test]
    fn test_integration() {
        // more comprehensive integration test
        match Pinger::new(None, None) {
            Ok((mut test_pinger, test_channel)) => {
                let test_addrs = vec!["127.0.0.1", "7.7.7.7", "::1"];
                for addr in test_addrs.iter() {
                    test_pinger.add_ipaddr(addr);
                }
                test_pinger.ping_once();
                for _ in test_addrs.iter() {
                    match test_channel.recv() {
                        Ok(result) => {
                            match result {
                                PingResult::Idle{addr} => {
                                    assert_eq!("7.7.7.7".parse::<IpAddr>().unwrap(), addr);
                                },
                                PingResult::Receive{addr, rtt: _} => {
                                    if addr == "::1".parse::<IpAddr>().unwrap() {
                                        assert!(true)
                                    } else if addr == "127.0.0.1".parse::<IpAddr>().unwrap() {
                                        assert!(true)
                                    } else {
                                        assert!(false)
                                    }
                                }
                            }
                        },
                        Err(_) => assert!(false),
                    }
                }
            }
            Err(e) => {
                println!("Test failed: {}", e);
                assert!(false)
            }
        }
    }
}
