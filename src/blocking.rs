use embedded_hal::blocking::delay::DelayUs;
use embedded_hal::digital::v2::OutputPin;
use crate::{Bus, LcdPinConfiguration};

#[allow(unused)]
pub struct Lcd<
    EN: OutputPin,
    RS: OutputPin,
    B: Bus,
    DELAY: DelayUs<u16>
> {
    pins: LcdPinConfiguration<EN, RS, B>,
    delay: DELAY
}