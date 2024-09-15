# Picotool-rs

A (work in progress) alternative implementation of picotool, as a library.

At the moment it can only flash simple UF2 files but the plan is for it to do more of the basic things that picotool does,
as well as extend it to perform some things that embedded rust users might want:

Example things I want to do, that would be tough for picotool:
- dump panic strings (that might be defmt encoded)
- flash then attach a defmt usb or serial connection


## Acknowledgments

picoboot implementation derived from [this reference implementation](https://github.com/NotQuiteApex/usb-picoboot-rs) and the [rp2350 datasheet](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf)