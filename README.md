
 embedded-hal async driver for dot matrix lcd display
 
 lcd is reset and initialized with display: off, it's off just because it's like that in reset procedure,
 
 even if it seems to work with display: on
 
 currently only works in write-only mode
 
 use dummy pin for RW pin
 
 doesn't track the current position
 
 this is because this driver is designed to have very small memory footprint
 
 the type Lcd can be zero sized depending on pin type and delay type
 
 you should use seek to change line and position

 currently blocking io and full width bus isn't supported
 
 (it shouldn't be "hard") i'm just lazy

 rw pin will allow use of busy flag but it isn't implemented
 
 instead, this driver works by waiting for a while
 
 waiting time is longer than the one in the spec, this is because i found problem with my compatible chip(eg. ks0066)
 
 without rw pin support

 example
 ```rust
 pub struct EmbassyDelayNs;

 impl DelayNs for EmbassyDelayNs {
     async fn delay_ns(&mut self, ns: u32) {
         embassy_time::Timer::after_micros(ns.div_ceil(1000) as u64).await;
     }
 }
     let mut lcd = Lcd::<_, _, _, _, Infallible>::new(
         LcdPinConfiguration {
             en: pins.d7.into_output(),
             rs: pins.d6.into_output(),
             bus: HalfWidthBus {
                 d4: pins.d8.into_output(),
                 d5: pins.d9.into_output(),
                 d6: pins.d10.into_output(),
                 d7: pins.d11.into_output()
             }
         },
         EmbassyDelayNs,
         Lines::TwoLines,
         EntryMode::default()
     ).await.unwrap();

     lcd.set_display_control(DisplayControl::default()).await.unwrap();
     lcd.seek(SeekFrom::Start(0)).await.unwrap(); //first line address 0..16
     lcd.write_all("first line".as_bytes()).await.unwrap();
     lcd.seek(SeekFrom::Start(40)).await.unwrap(); //second line address 40..56
     lcd.write_all("second line".as_bytes()).await.unwrap();
 ```
