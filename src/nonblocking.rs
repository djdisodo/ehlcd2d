use core::cmp::max;
use core::marker::PhantomData;
use embedded_hal::digital::v2::OutputPin;
use embedded_hal_async::delay::DelayNs;
use embedded_io_async::{ErrorKind, ErrorType, Seek, SeekFrom, Write};
use crate::{Bus, BusSend, Command, DisplayControl, Lines, EntryMode, HalfWidthBus, LcdIOError, LcdPinConfiguration};

pub struct Lcd<
    EN: OutputPin,
    RS: OutputPin,
    RW: OutputPin,
    B: Bus,
    DELAY: DelayNs,
    E
> {
    pins: LcdPinConfiguration<EN, RS, RW, B>,
    delay: DELAY,
    _error: PhantomData<E>
}

impl<
    EN: OutputPin,
    RS: OutputPin,
    RW: OutputPin,
    B: Bus,
    DELAY: DelayNs,
    E: From<EN::Error> + From<RS::Error> + From<RW::Error>
> Lcd<EN, RS, RW, B, DELAY, E> where
    Self: Reset<E>,
    LcdPinConfiguration<EN, RS, RW, B>: BusSend<E>
{
    /// lcd is reset and initialized with display: off, it's off just because it's like that in reset procedure,
    /// even if it seems to work with display: on
    /// currently only works in write-only mode
    /// use dummy pin for RW pin
    /// doesn't track the current position
    /// you should use seek to change line, position
    ///
    /// example
    /// ```rust
    ///     let mut lcd = Lcd::<_, _, _, _, _, Infallible>::new(
    ///         LcdPinConfiguration {
    ///             en: pins.d7.into_output(),
    ///             rs: pins.d6.into_output(),
    ///             rw: DummyPin::new_low(),
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
    pub async fn new(pins: LcdPinConfiguration<EN, RS, RW, B>, delay: DELAY, lines: Lines, entry: EntryMode) -> Result<Self, E> {
        let mut v = Self {
            pins,
            delay,
            _error: PhantomData::default()
        };
        v.init(lines, entry).await?;
        Ok(v)
    }

    async fn command(&mut self, command: u8) -> Result<(), E> {
        self.pins.command_nodelay(command)?;
        self.delay.delay_us(50).await; //most of commands require 37us but not all
        Ok(())
    }

    async fn init(&mut self, lines: Lines, entry: EntryMode) -> Result<(), E> {
        self.pins.rs.set_low()?;
        self.pins.en.set_low()?;
        self.pins.rw.set_low()?;
        self.delay.delay_us(15000).await;

        self.reset().await?;
        self.command(Command::FunctionSet as u8 | B::function_set(lines)).await?;
        let mut dc = DisplayControl::default();
        dc.set_display_on(false);
        self.display_control(dc).await?;
        self.clear().await?;
        self.entry_mode_set(entry).await
    }

    async fn set_ram_addr(&mut self, addr: u8) -> Result<(), E> {
        self.command(Command::SetDDRAMAddr as u8 | addr).await
    }

    pub async fn display_control(&mut self, control: DisplayControl) -> Result<(), E> {
        self.command(Command::DisplayControl as u8 | control.0).await
    }

    pub async fn entry_mode_set(&mut self, entry: EntryMode) -> Result<(), E> {
        self.command(Command::EntryModeSet as u8 | entry.0).await
    }

    pub async fn clear(&mut self) -> Result<(), E> {
        self.pins.command_nodelay(Command::ClearDisplay as u8)?;
        self.delay.delay_us(1520).await; //not in datasheet
        Ok(())
    }

    pub async fn home(&mut self) -> Result<(), E> {
        self.pins.command_nodelay(Command::ReturnHome as u8)?;
        self.delay.delay_us(1520).await;
        Ok(())
    }

    pub async fn write_char(&mut self, c: u8) -> Result<(), E> {
        self.pins.send(c, true)?;
        self.delay.delay_us(41).await;
        Ok(())
    }

    pub async fn set_display_control(&mut self, control: DisplayControl) -> Result<(), E> {
        self.command(Command::DisplayControl as u8 | control.0).await
    }
}


impl<
    EN: OutputPin,
    RS: OutputPin,
    RW: OutputPin,
    B: Bus,
    DELAY: DelayNs,
    E: From<EN::Error> + From<RS::Error> + From<RW::Error>
> ErrorType for Lcd<EN, RS, RW, B, DELAY, E> where
    Self: Reset<E>,
    LcdPinConfiguration<EN, RS, RW, B>: BusSend<E>
{
    type Error = LcdIOError<E>;
}

impl<
    EN: OutputPin,
    RS: OutputPin,
    RW: OutputPin,
    B: Bus,
    DELAY: DelayNs,
    E: From<EN::Error> + From<RS::Error> + From<RW::Error>
> Write for Lcd<EN, RS, RW, B, DELAY, E> where
    Self: Reset<E>,
    LcdPinConfiguration<EN, RS, RW, B>: BusSend<E>
{
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        let result: Result<usize, E> = try {
            for x in buf {
                self.write_char(*x).await?;
            }
            buf.len()
        };
        result.map_err(|e| LcdIOError(Some(e), ErrorKind::Other))
    }
}

impl<
    EN: OutputPin,
    RS: OutputPin,
    RW: OutputPin,
    B: Bus,
    DELAY: DelayNs,
    E: From<EN::Error> + From<RS::Error> + From<RW::Error>
> Seek for Lcd<EN, RS, RW, B, DELAY, E> where
    Self: Reset<E>,
    LcdPinConfiguration<EN, RS, RW, B>: BusSend<E>
{

    async fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        let v = match pos {
            SeekFrom::Start(mut v) => {
                v %= 80;
                v
            }
            SeekFrom::End(mut v) => {
                v %= 80;
                v += 80;
                v as u64
            }
            SeekFrom::Current(mut v) => return Err(LcdIOError(None, ErrorKind::Unsupported))
        };
        if v == 0 {
            return self.home().await.map_err(|e| LcdIOError(Some(e), ErrorKind::Other)).map(|_| 0);
        }
        self.set_ram_addr(v as u8).await.map_err(|e| LcdIOError(Some(e), ErrorKind::Other)).map(|_| v)
    }
}


pub struct TrackPosition<T, const SIZE: u8> {
    inner: T,
    position: u8
}

impl<T: ErrorType, const SIZE: u8> ErrorType for TrackPosition<T, SIZE> {
    type Error = T::Error;
}

impl<T: Write, const SIZE: u8> Write for TrackPosition<T, SIZE> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.inner.write(buf).await.map(|wrote| {
            self.position += (wrote % SIZE as usize) as u8;
            self.position %= SIZE;
            wrote
        })
    }
}

impl<T: Seek, const SIZE: u8> Seek for TrackPosition<T, SIZE> {
    async fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        match pos {
            SeekFrom::Start(_) | SeekFrom::End(_) => {
                self.inner.seek(pos).await.map(|pos| {
                    self.position = pos as u8;
                    pos
                })
            },
            SeekFrom::Current(mut v) => {
                v %= SIZE as i64;
                if v < 0 {
                    v += SIZE as i64;
                }
                let mut v = v as u8;
                self.position += v;
                self.position %= SIZE;
                self.inner.seek(SeekFrom::Start(self.position as u64)).await
            }
        }
    }
}

trait Reset<E> {
    async fn reset(&mut self) -> Result<(), E>;
}

impl<
    EN: OutputPin,
    RS: OutputPin,
    RW: OutputPin,
    D4: OutputPin,
    D5: OutputPin,
    D6: OutputPin,
    D7: OutputPin,
    E:
        From<EN::Error> + From<RS::Error> + From<RW::Error> +
        From<D4::Error> + From<D5::Error> + From<D6::Error> + From<D7::Error>,
    DELAY: DelayNs,
    _E
> Reset<E> for Lcd<EN, RS, RW, HalfWidthBus<D4, D5, D6, D7>, DELAY, _E> {
    async fn reset(&mut self) -> Result<(), E> {
        self.pins.update::<E>(0b0011)?;
        self.delay.delay_us(4100).await;
        self.pins.pulse()?;
        self.delay.delay_us(4100).await; //should be 100 according to spec? but it doesn't seem to work
        self.pins.pulse()?;
        self.delay.delay_us(37).await;
        self.pins.update::<E>(0b0010)?;
        Ok(())
    }
}