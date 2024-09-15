// This implementation is derived from the reference example provided at
// https://github.com/NotQuiteApex/usb-picoboot-rs
// with additions from https://github.com/9names/usb-picoboot-rs/tree/nusb

// see https://github.com/raspberrypi/picotool/blob/master/main.cpp#L4173
// for loading firmware over a connection

// see https://datasheets.raspberrypi.com/rp2040/rp2040-datasheet.pdf
// section 2.8.5 for details on PICOBOOT interface

use crate::picoboot::cmd::*;
use crate::TargetID;
use async_io::{block_on, Timer};
use bincode;
use futures_lite::FutureExt;
use nusb::{
    transfer::{
        ControlIn, ControlOut, ControlType, Direction, EndpointType, Recipient, RequestBuffer,
    },
    Device, DeviceInfo,
};
use std::{io, time::Duration};

const PICOBOOT_VID: u16 = 0x2E8A;
const PICOBOOT_PID_RP2040: u16 = 0x0003;
const PICOBOOT_PID_RP2350: u16 = 0x000f;

const USB_TIMEOUT: Duration = Duration::from_millis(5000);

struct ConnectionContext {
    target_id: TargetID,
    device: Device,
    interface: nusb::Interface,
    endpoint_out_addr: u8,
    endpoint_in_addr: u8,
}

fn is_picoboot_device(device: &nusb::DeviceInfo) -> bool {
    matches!(
        (device.vendor_id(), device.product_id()),
        (PICOBOOT_VID, PICOBOOT_PID_RP2040) | (PICOBOOT_VID, PICOBOOT_PID_RP2350)
    )
}

fn picoboot_device_type(device: &nusb::DeviceInfo) -> Option<TargetID> {
    match (device.vendor_id(), device.product_id()) {
        (PICOBOOT_VID, PICOBOOT_PID_RP2040) => Some(TargetID::Rp2040),
        (PICOBOOT_VID, PICOBOOT_PID_RP2350) => Some(TargetID::Rp2350),
        _ => None,
    }
}

fn open_device() -> Option<ConnectionContext> {
    let devices: Vec<DeviceInfo> = nusb::list_devices()
        .unwrap()
        .filter(is_picoboot_device)
        .collect();
    for device in &devices {
        println!(
            "Found an {:?} in bootsel mode",
            picoboot_device_type(device).unwrap()
        );
    }
    if devices.is_empty() {
        println!("No devices in bootsel mode found. Exiting!");
        return None;
    }
    if devices.len() > 1 {
        println!("Found more than one device. Using the first one found");
    }

    let device = devices.first().unwrap();
    let targetid = picoboot_device_type(device).unwrap();
    let mut endpoint_out_addr = None;
    let mut endpoint_in_addr = None;
    let mut endpoint_interfacenum = None;
    let device_handle = device.open().unwrap();
    let mut configs = device_handle.configurations();
    if let Some(config) = configs.next() {
        for interface in config.interfaces() {
            let interface_number = interface.interface_number();
            for altsetting in interface.alt_settings() {
                // from ref manual 5.6.2: PICOBOOT interface is recognised by
                // the vendor-specific Interface Class (0xff)
                // the zero interface subclass
                // the zero interface protocol
                if altsetting.class() == 0xff
                    && altsetting.subclass() == 0
                    && altsetting.protocol() == 0
                {
                    for endpoint in altsetting.endpoints() {
                        if endpoint.transfer_type() == EndpointType::Bulk {
                            if endpoint.direction() == Direction::Out {
                                endpoint_out_addr = Some(endpoint.address());
                                endpoint_interfacenum = Some(interface_number);
                            } else if endpoint.direction() == Direction::In {
                                endpoint_in_addr = Some(endpoint.address())
                            }
                        }
                    }
                }
            }
        }
    }

    let interface = if let Ok(interface) =
        device_handle.claim_interface(endpoint_interfacenum.expect("No interface found"))
    {
        interface
    } else {
        // maybe device is attached to OS driver? try to detach too
        device_handle
            .detach_and_claim_interface(endpoint_interfacenum.unwrap())
            .expect("could not detach and claim interface")
    };

    if endpoint_in_addr.is_some() && endpoint_out_addr.is_some() {
        let endpoint_out_addr = endpoint_out_addr.unwrap();
        let endpoint_in_addr = endpoint_in_addr.unwrap();
        Some(ConnectionContext {
            target_id: targetid,
            device: device_handle.clone(),
            interface,
            endpoint_out_addr,
            endpoint_in_addr,
        })
    } else {
        None
    }
}

// #[derive(Debug)]
#[allow(dead_code)]
pub struct PicobootConnection {
    ctx: ConnectionContext,
    cmd_token: u32,
    target_id: Option<TargetID>,
}

impl PicobootConnection {
    pub fn new() -> Option<Self> {
        let d = open_device();
        if d.is_none() {
            None
        } else {
            let d = d.unwrap();
            let target_id = d.target_id;

            Some(PicobootConnection {
                ctx: d,
                cmd_token: 1,
                target_id: Some(target_id),
            })
        }
    }

    fn bulk_read(&mut self, buf_size: usize, check: bool) -> io::Result<Vec<u8>> {
        let mut queue = self.ctx.interface.bulk_in_queue(self.ctx.endpoint_in_addr);

        queue.submit(RequestBuffer::new(buf_size));
        let result = block_on(queue.next_complete());
        let buf = result.into_result().unwrap();
        let len = buf.len();

        if check && len != buf_size {
            panic!("read mismatch {} != {}", len, buf_size)
        }

        Ok(buf)
    }

    fn bulk_write(&mut self, buf: Vec<u8>, check: bool) -> io::Result<()> {
        let fut = async {
            let comp = self
                .ctx
                .interface
                .bulk_out(self.ctx.endpoint_out_addr, buf.to_vec())
                .await;
            comp.status.map_err(io::Error::other)?;

            let len = comp.data.actual_length();
            if check && len != buf.len() {
                panic!("write mismatch {} != {}", len, buf.len())
            }
            Ok(())
        };

        block_on(fut.or(async {
            Timer::after(USB_TIMEOUT).await;
            Err(std::io::ErrorKind::TimedOut.into())
        }))
    }

    fn cmd(&mut self, mut cmd: PicobootCmd, buf: Vec<u8>) -> io::Result<Vec<u8>> {
        cmd.token = self.cmd_token;
        self.cmd_token += 1;
        let cmd = cmd;

        // write command
        let cmdu8 = bincode::serialize(&cmd).expect("failed to serialize cmd");
        self.bulk_write(cmdu8, true).expect("failed to write cmd");
        let _stat = self.get_command_status();

        // if we're reading or writing a buffer
        let l = cmd.transfer_len.try_into().unwrap();
        let mut res: Option<Vec<_>> = Some(vec![]);
        if l != 0 {
            if (cmd.cmd_id & 0x80) != 0 {
                res = Some(self.bulk_read(l, true).unwrap());
            } else {
                self.bulk_write(buf, true).unwrap()
            }
            let _stat = self.get_command_status();
        }

        // do ack
        if (cmd.cmd_id & 0x80) != 0 {
            self.bulk_write(vec![0], false).unwrap();
        } else {
            self.bulk_read(1, false).unwrap();
        }

        Ok(res.unwrap())
    }

    #[allow(dead_code)]
    pub fn access_not_exclusive(&mut self) -> io::Result<()> {
        self.set_exclusive_access(0)
    }

    #[allow(dead_code)]
    pub fn access_exclusive(&mut self) -> io::Result<()> {
        self.set_exclusive_access(1)
    }

    #[allow(dead_code)]
    pub fn access_exclusive_eject(&mut self) -> io::Result<()> {
        self.set_exclusive_access(2)
    }

    fn set_exclusive_access(&mut self, exclusive: u8) -> io::Result<()> {
        let mut args = [0; 16];
        args[0] = exclusive;
        let cmd = PicobootCmd::new(PicobootCmdId::ExclusiveAccess, 1, 0, args);
        self.cmd(cmd, vec![]).map(|_| ())
    }

    pub fn reboot(&mut self, pc: u32, sp: u32, delay: u32) -> io::Result<()> {
        let args = PicobootRebootCmd::ser(pc, sp, delay);
        let cmd = PicobootCmd::new(PicobootCmdId::Reboot, 12, 0, args);
        self.cmd(cmd, vec![]).map(|_| ())
    }

    pub fn reboot2_normal(&mut self, delay: u32) -> io::Result<()> {
        let flags: u32 = 0x0; // Normal boot
        let args = PicobootReboot2Cmd::ser(flags, delay, 0, 0);
        let cmd = PicobootCmd::new(PicobootCmdId::Reboot2, 0x10, 0, args);
        self.cmd(cmd, vec![]).map(|_| ())
    }

    pub fn flash_erase(&mut self, addr: u32, size: u32) -> io::Result<()> {
        let args = PicobootRangeCmd::ser(addr, size);
        let cmd = PicobootCmd::new(PicobootCmdId::FlashErase, 8, 0, args);
        self.cmd(cmd, vec![]).map(|_| ())
    }

    pub fn flash_write(&mut self, addr: u32, buf: Vec<u8>) -> io::Result<()> {
        let args = PicobootRangeCmd::ser(addr, buf.len() as u32);
        let cmd = PicobootCmd::new(PicobootCmdId::Write, 8, buf.len() as u32, args);
        self.cmd(cmd, buf).map(|_| ())
    }

    pub fn flash_read(&mut self, addr: u32, size: u32) -> io::Result<Vec<u8>> {
        let args = PicobootRangeCmd::ser(addr, size);
        let cmd = PicobootCmd::new(PicobootCmdId::Read, 8, size, args);
        self.cmd(cmd, vec![])
    }

    #[allow(dead_code)]
    pub fn enter_xip(&mut self) -> io::Result<()> {
        let args = [0; 16];
        let cmd = PicobootCmd::new(PicobootCmdId::EnterCmdXip, 0, 0, args);
        self.cmd(cmd, vec![]).map(|_| ())
    }

    pub fn exit_xip(&mut self) -> io::Result<()> {
        let args = [0; 16];
        let cmd = PicobootCmd::new(PicobootCmdId::ExitXip, 0, 0, args);
        self.cmd(cmd, vec![]).map(|_| ())
    }

    pub fn reset_interface(&mut self) {
        let result = block_on(self.ctx.device.control_out(ControlOut {
            control_type: ControlType::Vendor,
            recipient: Recipient::Interface,
            request: 0b01000001,
            value: 0,
            index: self.ctx.interface.interface_number() as u16,
            data: &[],
        }));
        let _ = result.into_result().expect("failed to reset");
    }

    fn get_command_status(&mut self) -> PicobootStatusCmd {
        let result = block_on(self.ctx.interface.control_in(ControlIn {
            control_type: ControlType::Vendor,
            recipient: Recipient::Interface,
            request: 0b01000010,
            value: 0,
            index: self.ctx.interface.interface_number() as u16,
            length: 16,
        }));

        let buf = result.into_result().expect("failed to get command status");

        let buf: PicobootStatusCmd =
            bincode::deserialize(&buf).expect("failed to parse command status buffer");

        buf
    }

    pub fn get_device_type(&self) -> Option<TargetID> {
        self.target_id
    }
}
