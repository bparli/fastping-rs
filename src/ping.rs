use std::net::{IpAddr};
use rand::random;
use pnet::packet::icmp::{IcmpTypes};
use pnet::transport::TransportSender;
use pnet::packet::icmp::{echo_request};
use pnet::packet::icmpv6::{Icmpv6Types, MutableIcmpv6Packet};
use pnet::util;
use pnet_macros_support::types::*;
use pnet::packet::Packet;

pub fn send_echo(tx: &mut TransportSender, addr: IpAddr) {
    // Allocate enough space for a new packet
    let mut vec: Vec<u8> = vec![0; 16];


    // Use echo_request so we can set the identifier and sequence number
    let mut echo_packet = echo_request::MutableEchoRequestPacket::new(&mut vec[..]).unwrap();
    echo_packet.set_sequence_number(random::<u16>());
    echo_packet.set_identifier(random::<u16>());
    echo_packet.set_icmp_type(IcmpTypes::EchoRequest);

    let csum = icmp_checksum(&echo_packet);
    echo_packet.set_checksum(csum);

    match tx.send_to(echo_packet, addr) {
        Ok(n) => {
            debug!("Using payload {}", &n);
        },
        Err(e) => panic!("failed to send packet: {}", e),
    }
}

pub fn send_echov6(tx: &mut TransportSender, addr: IpAddr) {
    // Allocate enough space for a new packet
    let mut vec: Vec<u8> = vec![0; 16];


    // Use echo_request so we can set the identifier and sequence number
    let mut echo_packet = MutableIcmpv6Packet::new(&mut vec[..]).unwrap();
    echo_packet.set_icmpv6_type(Icmpv6Types::EchoRequest);

    let csum = icmpv6_checksum(&echo_packet);
    echo_packet.set_checksum(csum);

    match tx.send_to(echo_packet, addr) {
        Ok(n) => {
            debug!("Using payload {}", &n);
        },
        Err(e) => panic!("failed to send packet: {}", e),
    }
}

fn icmp_checksum(packet: &echo_request::MutableEchoRequestPacket) -> u16be {
    util::checksum(packet.packet(), 1)
}

fn icmpv6_checksum(packet: &MutableIcmpv6Packet) -> u16be {
    util::checksum(packet.packet(), 1)
}
