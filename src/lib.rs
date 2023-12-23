#![no_std]
#![feature(try_blocks)]
#![allow(private_bounds)]

// remove when complete release
#![allow(unused)]

/// lcd is reset and initialized with display: off, it's off just because it's like that in reset procedure,
/// even if it seems to work with display: on
/// currently only works in write-only mode
/// use dummy pin for RW pin
/// doesn't track the current position
/// this is because this driver is designed to have very small memory footprint
/// the type Lcd can be zero sized depending on pin type and delay type
/// you should use seek to change line and position
///
/// currently blocking io and full width bus isn't supported
/// (it shouldn't be "hard") i'm just lazy
///
/// rw pin will allow use of busy flag but it isn't implemented
/// instead, this driver works by waiting for a while
/// waiting time is longer than the one in the spec, this is to support compatible chips(eg. ks0066)
/// without rw pin support
///
/// example
/// ```rust
/// pub struct EmbassyDelayNs;
///
/// impl DelayNs for EmbassyDelayNs {
///     async fn delay_ns(&mut self, ns: u32) {
///         embassy_time::Timer::after_micros(ns.div_ceil(1000) as u64).await;
///     }
/// }
///     let mut lcd = Lcd::<_, _, _, _, Infallible>::new(
///         LcdPinConfiguration {
///             en: pins.d7.into_output(),
///             rs: pins.d6.into_output(),
///             bus: HalfWidthBus {
///                 d4: pins.d8.into_output(),
///                 d5: pins.d9.into_output(),
///                 d6: pins.d10.into_output(),
///                 d7: pins.d11.into_output()
///             }
///         },
///         EmbassyDelayNs,
///         Lines::TwoLines,
///         EntryMode::default()
///     ).await.unwrap();
///
///     lcd.set_display_control(DisplayControl::default()).await.unwrap();
///     lcd.seek(SeekFrom::Start(0)).await.unwrap(); //first line address 0..16
///     lcd.write_all("first line".as_bytes()).await.unwrap();
///     lcd.seek(SeekFrom::Start(40)).await.unwrap(); //second line address 40..56
///     lcd.write_all("second line".as_bytes()).await.unwrap();
/// ```

use core::fmt::{Debug, Formatter};
use embedded_hal::digital::v2::{OutputPin, PinState};
use embedded_io_async::{Error, ErrorKind};

pub mod blocking;
pub mod nonblocking;

#[repr(u8)]
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    ClearDisplay   = 0b00000001,
    ReturnHome     = 0b00000010,
    EntryModeSet   = 0b00000100,
    DisplayControl = 0b00001000,
    CursorShift    = 0b00010000,
    FunctionSet    = 0b00100000,
    SetCGramAddr   = 0b01000000,
    SetDDRAMAddr   = 0b10000000
}

// #[repr(u8)]
// #[derive(Clone, Copy, Debug, PartialEq, Eq)]
// enum Move {
//     Display = 0b10000000,
//     Cursor = 0b00000000
// }
// #[repr(u8)]
// #[derive(Clone, Copy, Debug, PartialEq, Eq)]
// pub enum MoveDirection {
//     Right = 0b0100,
//     Left = 0b0000
// }
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncrementMode {
    Decremental = 0b00,
    Incremental = 0b10,
}
const FULL_WIDTH_BUS: u8 = 0b00010000;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lines {
    FourLines   = 0b1100,
    TwoLines    = 0b1000,
    OneLine5x10 = 0b0100,
    OneLine5x8  = 0b0000
}

impl Default for Lines {
    fn default() -> Self {
        Self::OneLine5x8
    }
}
// TODO debug
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct DisplayControl(u8);

impl DisplayControl {
    pub fn set_display_on(&mut self, v: bool) {
        const FLAG: u8 = 0b100;
        self.0 &= !FLAG;
        if v {
            self.0 |= FLAG;
        }
    }

    pub fn set_cursor(&mut self, v: bool) {
        const FLAG: u8 = 0b010;
        self.0 &= !FLAG;
        if v {
            self.0 |= FLAG;
        }
    }

    pub fn set_blink(&mut self, v: bool) {
        const FLAG: u8 = 0b001;
        self.0 &= !FLAG;
        if v {
            self.0 |= FLAG;
        }
    }
}

impl Default for DisplayControl {
    fn default() -> Self {
        let mut v = DisplayControl(0);
        v.set_display_on(true);
        v
    }
}
// TODO debug
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EntryMode(u8);

impl EntryMode {
    pub fn set_increment_mode(&mut self, increment_mode: IncrementMode) {
        const FLAG: u8 = 0b010;
        self.0 &= !FLAG;
        if increment_mode == IncrementMode::Incremental {
            self.0 |= FLAG;
        }
    }

    pub fn set_scroll(&mut self, v: bool) {
        const FLAG: u8 = 0b001;
        self.0 &= !FLAG;
        if v {
            self.0 |= FLAG;
        }
    }
}

impl Default for EntryMode {
    fn default() -> Self {
        let mut v = Self(0);
        v.set_increment_mode(IncrementMode::Incremental);
        v
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HalfWidthBus<
    D4: OutputPin,
    D5: OutputPin,
    D6: OutputPin,
    D7: OutputPin
> {
    pub d4: D4,
    pub d5: D5,
    pub d6: D6,
    pub d7: D7
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FullWidthBus<
    D0: OutputPin,
    D1: OutputPin,
    D2: OutputPin,
    D3: OutputPin,
    D4: OutputPin,
    D5: OutputPin,
    D6: OutputPin,
    D7: OutputPin
> {
    pub d0: D0,
    pub d1: D1,
    pub d2: D2,
    pub d3: D3,
    pub d4: D4,
    pub d5: D5,
    pub d6: D6,
    pub d7: D7
}

pub trait Bus where

{
    fn function_set(lines: Lines) -> u8;
}

impl<
    D4: OutputPin,
    D5: OutputPin,
    D6: OutputPin,
    D7: OutputPin
> Bus for HalfWidthBus<D4, D5, D6, D7> {
    fn function_set(lines: Lines) -> u8 {
        lines as u8 & !FULL_WIDTH_BUS
    }
}

impl<
    D0: OutputPin,
    D1: OutputPin,
    D2: OutputPin,
    D3: OutputPin,
    D4: OutputPin,
    D5: OutputPin,
    D6: OutputPin,
    D7: OutputPin
> Bus for FullWidthBus<D0, D1, D2, D3, D4, D5, D6, D7> {
    fn function_set(lines: Lines) -> u8 {
        lines as u8 | FULL_WIDTH_BUS
    }
}

pub(crate) fn pin_state(v: bool) -> PinState {
    match v {
        true => PinState::High,
        false => PinState::Low
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LcdPinConfiguration<
    EN: OutputPin,
    RS: OutputPin,
    B: Bus
> {
    pub en: EN,
    pub rs: RS,
    pub bus: B
}

impl<
    EN: OutputPin,
    RS: OutputPin,
    B: Bus
> LcdPinConfiguration<EN, RS, B> {
    fn pulse(&mut self) -> Result<(), EN::Error> {
        self.en.set_high()?;
        self.en.set_low()
    }
}

impl<
    EN: OutputPin,
    RS: OutputPin,
    D4: OutputPin,
    D5: OutputPin,
    D6: OutputPin,
    D7: OutputPin
> LcdPinConfiguration<EN, RS, HalfWidthBus<D4, D5, D6, D7>> {
    fn update<
        E:
            From<EN::Error> + From<RS::Error> +
            From<D4::Error> + From<D5::Error> + From<D6::Error> + From<D7::Error>
    >(&mut self, mut byte: u8) -> Result<(), E> {
        self.bus.d4.set_state(pin_state(byte & 1 == 1))?;
        byte >>= 1;
        self.bus.d5.set_state(pin_state(byte & 1 == 1))?;
        byte >>= 1;
        self.bus.d6.set_state(pin_state(byte & 1 == 1))?;
        byte >>= 1;
        self.bus.d7.set_state(pin_state(byte & 1 == 1))?;
        self.pulse()?;
        Ok(())
    }
}

impl<
    EN: OutputPin,
    RS: OutputPin,
    D0: OutputPin,
    D1: OutputPin,
    D2: OutputPin,
    D3: OutputPin,
    D4: OutputPin,
    D5: OutputPin,
    D6: OutputPin,
    D7: OutputPin
> LcdPinConfiguration<EN, RS, FullWidthBus<D0, D1, D2, D3, D4, D5, D6, D7>> {
    fn update<
        E:
            From<EN::Error> + From<RS::Error> +
            From<D0::Error> + From<D1::Error> + From<D2::Error> + From<D3::Error> +
            From<D4::Error> + From<D5::Error> + From<D6::Error> + From<D7::Error>
    >(&mut self, mut byte: u8) -> Result<(), E> {
        self.bus.d0.set_state(pin_state(byte & 1 == 1))?;
        byte >>= 1;
        self.bus.d1.set_state(pin_state(byte & 1 == 1))?;
        byte >>= 1;
        self.bus.d2.set_state(pin_state(byte & 1 == 1))?;
        byte >>= 1;
        self.bus.d3.set_state(pin_state(byte & 1 == 1))?;
        byte >>= 1;
        self.bus.d4.set_state(pin_state(byte & 1 == 1))?;
        byte >>= 1;
        self.bus.d5.set_state(pin_state(byte & 1 == 1))?;
        byte >>= 1;
        self.bus.d6.set_state(pin_state(byte & 1 == 1))?;
        byte >>= 1;
        self.bus.d7.set_state(pin_state(byte & 1 == 1))?;
        self.pulse()?;
        Ok(())
    }
}
pub trait BusSend<E> {
    fn send(&mut self, byte: u8, mode: bool) -> Result<(), E>;

    fn command_nodelay(&mut self, byte: u8) -> Result<(), E> {
        self.send(byte, false)
    }
}

impl<
    EN: OutputPin,
    RS: OutputPin,
    D4: OutputPin,
    D5: OutputPin,
    D6: OutputPin,
    D7: OutputPin,
    E:
        From<EN::Error> + From<RS::Error> +
        From<D4::Error> + From<D5::Error> + From<D6::Error> + From<D7::Error>
> BusSend<E> for LcdPinConfiguration<EN, RS, HalfWidthBus<D4, D5, D6, D7>> {
    fn send(&mut self, byte: u8, mode: bool) -> Result<(), E> {
        self.rs.set_state(pin_state(mode))?;
        self.update::<E>(byte >> 4)?;
        self.update::<E>(byte)?;
        Ok(())
    }
}

impl<
    EN: OutputPin,
    RS: OutputPin,
    D0: OutputPin,
    D1: OutputPin,
    D2: OutputPin,
    D3: OutputPin,
    D4: OutputPin,
    D5: OutputPin,
    D6: OutputPin,
    D7: OutputPin,
    E:
        From<EN::Error> + From<RS::Error> +
        From<D0::Error> + From<D1::Error> + From<D2::Error> + From<D3::Error> +
        From<D4::Error> + From<D5::Error> + From<D6::Error> + From<D7::Error>
> BusSend<E> for LcdPinConfiguration<EN, RS, FullWidthBus<D0, D1, D2, D3, D4, D5, D6, D7>> {
    fn send(&mut self, byte: u8, mode: bool) -> Result<(), E> {
        self.rs.set_state(pin_state(mode))?;
        self.update::<E>(byte)?;
        Ok(())
    }
}

pub struct LcdIOError<T>(pub Option<T>, pub ErrorKind);

impl<T> Error for LcdIOError<T> {
    fn kind(&self) -> ErrorKind {
        self.1
    }
}

impl<T> Debug for LcdIOError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        (self.1).fmt(f)
    }
}