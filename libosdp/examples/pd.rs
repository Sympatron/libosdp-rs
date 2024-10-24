//
// Copyright (c) 2023-2024 Siddharth Chandrasekaran <sidcha.dev@gmail.com>
//
// SPDX-License-Identifier: Apache-2.0

use libosdp::{
    Channel, ChannelError, OsdpError, OsdpFlag, PdCapEntity, PdCapability, PdInfoBuilder,
};
use std::{thread, time::Duration};

struct OsdpChannel;

impl OsdpChannel {
    pub fn new(_path: &str) -> Self {
        // setup device
        Self {}
    }
}

/// Read documentation for each member in [libosdp::Channel].
impl Channel for OsdpChannel {
    fn get_id(&self) -> i32 {
        0
    }

    fn read(&mut self, _buf: &mut [u8]) -> Result<usize, ChannelError> {
        // TODO: Read from device
        Ok(0)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, ChannelError> {
        // TODO: Write from device
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<(), ChannelError> {
        // TODO: flush device
        Ok(())
    }
}

fn main() -> Result<(), OsdpError> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .format_target(false)
        .format_timestamp(None)
        .init();
    let args: Vec<String> = std::env::args().collect();
    let channel = OsdpChannel::new(&args[1]);

    #[rustfmt::skip]
    let key = [
        0x94, 0x4b, 0x8e, 0xdd, 0xcb, 0xaa, 0x2b, 0x5f,
        0xe2, 0xb0, 0x14, 0x8d, 0x1b, 0x2f, 0x95, 0xc9
    ];

    let pd_info = PdInfoBuilder::new()
        .name("PD 101")?
        .address(101)?
        .baud_rate(115200)?
        .flag(OsdpFlag::EnforceSecure)
        .capability(PdCapability::CommunicationSecurity(PdCapEntity::new(1, 1)))
        .secure_channel_key(key);
    let mut pd = libosdp::PeripheralDevice::new(pd_info, Box::new(channel))?;
    pd.set_command_callback(|_| {
        println!("Received command!");
        0
    });
    loop {
        pd.refresh();
        thread::sleep(Duration::from_millis(50));
    }
}
