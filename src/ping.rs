use crate::PingResult;
use pnet::packet::Packet;
use pnet::packet::{icmp, icmpv6};
use pnet::transport::TransportSender;
use pnet::util;
use rand::random;
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

pub struct Ping {
    addr: IpAddr,
    identifier: u16,
    sequence_number: u16,
    pub seen: bool,
}

pub struct ReceivedPing {
    pub addr: IpAddr,
    pub identifier: u16,
    pub sequence_number: u16,
    pub rtt: Duration,
}

impl Ping {
    pub fn new(addr: IpAddr) -> Ping {
        Ping {
            addr,
            identifier: random::<u16>(),
            sequence_number: 0,
            seen: false,
        }
    }

    pub fn get_addr(&self) -> IpAddr {
        return self.addr;
    }

    pub fn get_identifier(&self) -> u16 {
        return self.identifier;
    }

    pub fn get_sequence_number(&self) -> u16 {
        return self.sequence_number;
    }

    pub fn increment_sequence_number(&mut self) -> u16 {
        self.sequence_number += 1;
        return self.sequence_number;
    }
}

fn send_echo(
    tx: &mut TransportSender,
    ping: &mut Ping,
    size: usize,
) -> Result<usize, std::io::Error> {
    // Allocate enough space for a new packet
    let mut vec: Vec<u8> = vec![0; size];

    let mut echo_packet = icmp::echo_request::MutableEchoRequestPacket::new(&mut vec[..]).unwrap();
    echo_packet.set_sequence_number(ping.increment_sequence_number());
    echo_packet.set_identifier(ping.get_identifier());
    echo_packet.set_icmp_type(icmp::IcmpTypes::EchoRequest);

    let csum = util::checksum(echo_packet.packet(), 1);
    echo_packet.set_checksum(csum);

    tx.send_to(echo_packet, ping.get_addr())
}

fn send_echov6(
    tx: &mut TransportSender,
    ping: &mut Ping,
    size: usize,
) -> Result<usize, std::io::Error> {
    // Allocate enough space for a new packet
    let mut vec: Vec<u8> = vec![0; size];

    let mut echo_packet =
        icmpv6::echo_request::MutableEchoRequestPacket::new(&mut vec[..]).unwrap();
    echo_packet.set_sequence_number(ping.increment_sequence_number());
    echo_packet.set_identifier(ping.get_identifier());
    echo_packet.set_icmpv6_type(icmpv6::Icmpv6Types::EchoRequest);

    // Note: ICMPv6 checksum always calculated by the kernel, see RFC 3542

    tx.send_to(echo_packet, ping.get_addr())
}

pub fn send_pings(
    size: usize,
    timer: Arc<RwLock<Instant>>,
    stop: Arc<Mutex<bool>>,
    results_sender: Sender<PingResult>,
    thread_rx: Arc<Mutex<Receiver<ReceivedPing>>>,
    tx: Arc<Mutex<TransportSender>>,
    txv6: Arc<Mutex<TransportSender>>,
    targets: Arc<Mutex<BTreeMap<IpAddr, Ping>>>,
    max_rtt: Arc<Duration>,
) {
    loop {
        for (addr, ping) in targets.lock().unwrap().iter_mut() {
            match if addr.is_ipv4() {
                send_echo(&mut tx.lock().unwrap(), ping, size)
            } else if addr.is_ipv6() {
                send_echov6(&mut txv6.lock().unwrap(), ping, size)
            } else {
                Ok(0)
            } {
                Err(e) => error!("Failed to send ping to {:?}: {}", *addr, e),
                _ => {}
            }
            ping.seen = false;
        }
        let start_time = Instant::now();
        {
            // start the timer
            let mut timer = timer.write().unwrap();
            *timer = start_time;
        }
        loop {
            // use recv_timeout so we don't cause a CPU to needlessly spin
            match thread_rx
                .lock()
                .unwrap()
                .recv_timeout(max_rtt.saturating_sub(start_time.elapsed()))
            {
                Ok(ping_result) => {
                    // match ping_result {
                    let ReceivedPing {
                        addr,
                        identifier,
                        sequence_number,
                        rtt: _,
                    } = ping_result;
                    // Update the address to the ping response being received
                    if let Some(ping) = targets.lock().unwrap().get_mut(&addr) {
                        if ping.get_identifier() == identifier
                            && ping.get_sequence_number() == sequence_number
                        {
                            ping.seen = true;
                            // Send the ping result over the client channel
                            match results_sender.send(PingResult::Receive {
                                addr: ping_result.addr,
                                rtt: ping_result.rtt,
                            }) {
                                Ok(_) => {}
                                Err(e) => {
                                    if !*stop.lock().unwrap() {
                                        error!("Error sending ping result on channel: {}", e)
                                    }
                                }
                            }
                        } else {
                            debug!("Received echo reply from target {}, but sequence_number (expected {} but got {}) and identifier (expected {} but got {}) don't match", addr, ping.get_sequence_number(), sequence_number, ping.get_identifier(), identifier);
                        }
                    }
                }
                Err(_) => {
                    // Check we haven't exceeded the max rtt
                    if start_time.elapsed() >= *max_rtt {
                        break;
                    }
                }
            }
        }
        // check for addresses which haven't replied
        for (addr, ping) in targets.lock().unwrap().iter() {
            if ping.seen == false {
                // Send the ping Idle over the client channel
                match results_sender.send(PingResult::Idle { addr: *addr }) {
                    Ok(_) => {}
                    Err(e) => {
                        if !*stop.lock().unwrap() {
                            error!("Error sending ping Idle result on channel: {}", e)
                        }
                    }
                }
            }
        }
        // check if we've received the stop signal
        if *stop.lock().unwrap() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ping() {
        let mut p = Ping::new("127.0.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(p.get_sequence_number(), 0);
        assert!(p.get_identifier() > 0);

        p.increment_sequence_number();
        assert_eq!(p.get_sequence_number(), 1);
    }
}
