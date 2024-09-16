use async_io::block_on;
use nusb::{
    transfer::{ControlOut, ControlType, Recipient},
    DeviceInfo,
};
const RP_VID: u16 = 0x2E8A;

const RESET_REQUEST_BOOTSEL: u8 = 0x01;
//const RESET_REQUEST_FLASH: u8 = 0x02;

pub fn reset_usb_device() {
    let devices: Vec<DeviceInfo> = nusb::list_devices()
        .unwrap()
        .filter(|d| d.vendor_id() == RP_VID)
        .collect();
    if devices.len() > 1 {
        println!("Found more than one device. Using the first one found");
    }

    let device = devices.first().unwrap();
    let device_handle = device.open().unwrap();
    let mut interfacelist: Vec<u8> = vec![];
    for config in device_handle.configurations() {
        for interface in config.interfaces() {
            for altsetting in interface.alt_settings() {
                if altsetting.class() == 0xff
                    && altsetting.subclass() == 0
                    && altsetting.protocol() == 1
                {
                    println!("found a possible reset interface");
                    interfacelist.push(interface.interface_number());
                }
            }
        }
    }
    for iface in interfacelist {
        let d = device_handle
            .claim_interface(iface)
            .or(device_handle.detach_and_claim_interface(iface))
            .expect("could not claim interface");
        let _result = block_on(d.control_out(ControlOut {
            control_type: ControlType::Class,
            recipient: Recipient::Interface,
            request: RESET_REQUEST_BOOTSEL,
            value: 0,
            index: iface as u16,
            data: &[],
        }));
        // On successful reset, we get a USB stall.
        // The only way we can tell if we succeeded would be to connect via picoboot
        // so don't try to handle the result
    }
}
