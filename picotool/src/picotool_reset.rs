use async_io::block_on;
use nusb::{
    transfer::{ControlOut, ControlType, Recipient},
    DeviceInfo,
};
const RP_VID: u16 = 0x2E8A;

const RESET_REQUEST_BOOTSEL: u8 = 0x01;
// const RESET_REQUEST_FLASH: u8 = 0x02;

pub fn reset_usb_device() {
    let devices: Vec<DeviceInfo> = nusb::list_devices()
        .unwrap()
        .filter(|d| d.vendor_id() == RP_VID)
        .collect();

    if devices.is_empty() {
        println!("No USB devices found with a Raspberry Pi VendorID");
        return;
    }

    if devices.len() > 1 {
        println!("Found more than one device. Using the first one found");
    }

    let device_handle = devices.first().unwrap().open().unwrap();
    let reset_devices: Vec<u8> = device_handle.configurations().flat_map(|cfg| {
        cfg.interface_alt_settings()
            .filter(|alt| alt.class() == 0xff && alt.subclass() == 0 && alt.protocol() == 1)
            .map(|i| i.interface_number())
    }).collect();

    if reset_devices.is_empty() {
        println!("A device was found with a Raspberry Pi VendorID, but it did not have the USB Reset interface");
        return;
    }

    println!("Resetting pico...");
    for iface in reset_devices {
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
    }
}
