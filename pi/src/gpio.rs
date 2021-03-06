use core::marker::PhantomData;

use common::{IO_BASE, states, spin_sleep_cycles};
use sys::volatile::prelude::*;
use sys::volatile::{Volatile, WriteVolatile, ReadVolatile, Reserved};

/// An alternative GPIO function.
#[repr(u8)]
pub enum Function {
    Input = 0b000,
    Output = 0b001,
    Alt0 = 0b100,
    Alt1 = 0b101,
    Alt2 = 0b110,
    Alt3 = 0b111,
    Alt4 = 0b011,
    Alt5 = 0b010
}

pub enum Event {
    RisingEdge,
    FallingEdge,
    HighLevel,
    LowLevel,
    AsyncRisingEdge,
    AsyncFallingEdge,
}

#[repr(u32)]
pub enum Pud {
    Off = 0b00,
    Down = 0b01,
    Up = 0b10,
}

#[repr(C)]
#[allow(non_snake_case)]
struct Registers {
    FSEL: [Volatile<u32>; 6],
    __r0: Reserved<u32>,
    SET: [WriteVolatile<u32>; 2],
    __r1: Reserved<u32>,
    CLR: [WriteVolatile<u32>; 2],
    __r2: Reserved<u32>,
    LEV: [ReadVolatile<u32>; 2],
    __r3: Reserved<u32>,
    EDS: [Volatile<u32>; 2],
    __r4: Reserved<u32>,
    REN: [Volatile<u32>; 2],
    __r5: Reserved<u32>,
    FEN: [Volatile<u32>; 2],
    __r6: Reserved<u32>,
    HEN: [Volatile<u32>; 2],
    __r7: Reserved<u32>,
    LEN: [Volatile<u32>; 2],
    __r8: Reserved<u32>,
    AREN: [Volatile<u32>; 2],
    __r9: Reserved<u32>,
    AFEN: [Volatile<u32>; 2],
    __r10: Reserved<u32>,
    PUD: Volatile<u32>,
    PUDCLK: [Volatile<u32>; 2],
}

/// Possible states for a GPIO pin.
states! {
    Uninitialized, Input, Output, Alt
}

/// A GPIP pin in state `State`.
///
/// The `State` generic always corresponds to an uninstantiatable type that is
/// use solely to mark and track the state of a given GPIO pin. A `Gpio`
/// structure starts in the `Uninitialized` state and must be transitions into
/// one of `Input`, `Output`, or `Alt` via the `into_input`, `into_output`, and
/// `into_alt` methods before it can be used.
pub struct Gpio<State> {
    pin: u8,
    registers: &'static mut Registers,
    _state: PhantomData<State>
}

/// The base address of the `GPIO` registers.
pub const GPIO_BASE: usize = 0x200000;

impl<T> Gpio<T> {
    /// Transitions `self` to state `S`, consuming `self` and returning a new
    /// `Gpio` instance in state `S`. This method should _never_ be exposed to
    /// the public!
    #[inline(always)]
    fn transition<S>(self) -> Gpio<S> {
        Gpio {
            pin: self.pin,
            registers: self.registers,
            _state: PhantomData
        }
    }

    pub fn set_pud(&mut self, pud: Pud) {
        let index = (self.pin / 32) as usize;
        let shift = (self.pin % 32) as u32;
        self.registers.PUD.write(pud as u32);
        spin_sleep_cycles(150);
        self.registers.PUDCLK[index].and_mask(1 << shift);
        spin_sleep_cycles(150);
        self.registers.PUD.write(0);
        self.registers.PUDCLK[index].write(0);
    }

    pub fn set_event_detection(&mut self, event: Event) {
        let index = (self.pin / 32) as usize;
        let mask = 1 << (self.pin % 32) as u32;
        match event {
            Event::RisingEdge => self.registers.REN[index].or_mask(mask),
            Event::FallingEdge => self.registers.FEN[index].or_mask(mask),
            Event::HighLevel => self.registers.HEN[index].or_mask(mask),
            Event::LowLevel => self.registers.LEN[index].or_mask(mask),
            Event::AsyncRisingEdge => self.registers.AREN[index].or_mask(mask),
            Event::AsyncFallingEdge => self.registers.AFEN[index].or_mask(mask),
        }
        self.clear_event();
    }

    pub fn clear_event_detection(&mut self, event: Event) {
        let index = (self.pin / 32) as usize;
        let mask = !(1 << (self.pin % 32) as u32);
        match event {
            Event::RisingEdge => self.registers.REN[index].and_mask(mask),
            Event::FallingEdge => self.registers.FEN[index].and_mask(mask),
            Event::HighLevel => self.registers.HEN[index].and_mask(mask),
            Event::LowLevel => self.registers.LEN[index].and_mask(mask),
            Event::AsyncRisingEdge => self.registers.AREN[index].and_mask(mask),
            Event::AsyncFallingEdge => self.registers.AFEN[index].and_mask(mask),
        }
        self.clear_event();
    }

    pub fn get_event_detection(&mut self, event: Event) -> bool {
        let index = (self.pin / 32) as usize;
        let mask = 1 << (self.pin % 32) as u32;
        let val = match event {
            Event::RisingEdge => self.registers.REN[index].read(),
            Event::FallingEdge => self.registers.FEN[index].read(),
            Event::HighLevel => self.registers.HEN[index].read(),
            Event::LowLevel => self.registers.LEN[index].read(),
            Event::AsyncRisingEdge => self.registers.AREN[index].read(),
            Event::AsyncFallingEdge => self.registers.AFEN[index].read(),
        };
        (val & mask) != 0
    }

    pub fn check_event(&mut self) -> bool {
        let index = (self.pin / 32) as usize;
        let shift = (self.pin % 32) as u32;
        (self.registers.EDS[index].read() & (1 << shift)) != 0
    }
    pub fn clear_event(&mut self) {
        let index = (self.pin / 32) as usize;
        let shift = (self.pin % 32) as u32;
        if self.check_event() {
            self.registers.EDS[index].write(1 << shift);
        }
    }
    pub fn check_and_clear_event(&mut self) -> bool {
        let event = self.check_event();
        if event {
            self.clear_event();
        }
        event
    }
}

impl Gpio<Uninitialized> {
    /// Returns a new `GPIO` structure for pin number `pin`.
    ///
    /// # Panics
    ///
    /// Panics if `pin` > `53`.
    pub fn new(pin: u8) -> Self {
        unsafe { Self::new_from(IO_BASE + GPIO_BASE, pin) }
    }

    /// Returns a new `GPIO` structure for pin number `pin`.
    ///
    /// # Panics
    ///
    /// Panics if `pin` > `53`.
    pub unsafe fn new_from(base: usize, pin: u8) -> Self {
        if pin > 53 {
            panic!("Gpio::new(): pin {} exceeds maximum of 53", pin);
        }
        let registers = &mut *(base as *mut Registers);
        Self { registers, pin, _state: PhantomData }
    }

    /// Enables the alternative function `function` for `self`. Consumes self
    /// and returns a `Gpio` structure in the `Alt` state.
    pub fn into_alt(self, function: Function) -> Gpio<Alt> {
        let index = (self.pin / 10) as usize;
        let shift = ((self.pin % 10) * 3) as u32;
        let value = (function as u32) << shift;
        let mask = !(0b111 << shift);
        let data = self.registers.FSEL[index].read();
        self.registers.FSEL[index].write(data & mask | value);

        self.transition()
    }

    /// Sets this pin to be an _output_ pin. Consumes self and returns a `Gpio`
    /// structure in the `Output` state.
    pub fn into_output(self) -> Gpio<Output> {
        self.into_alt(Function::Output).transition()
    }

    /// Sets this pin to be an _input_ pin. Consumes self and returns a `Gpio`
    /// structure in the `Input` state.
    pub fn into_input(self) -> Gpio<Input> {
        self.into_alt(Function::Input).transition()
    }
}

impl Gpio<Output> {
    /// Sets (turns on) the pin.
    pub fn set(&mut self) {
        let index = (self.pin / 32) as usize;
        let shift = (self.pin % 32) as u32;
        self.registers.SET[index].write(1 << shift);
    }

    /// Clears (turns off) the pin.
    pub fn clear(&mut self) {
        let index = (self.pin / 32) as usize;
        let shift = (self.pin % 32) as u32;
        self.registers.CLR[index].write(1 << shift);
    }
}

impl Gpio<Input> {
    /// Reads the pin's value. Returns `true` if the level is high and `false`
    /// if the level is low.
    pub fn level(&mut self) -> bool {
        let index = (self.pin / 32) as usize;
        let shift = (self.pin % 32) as u32;
        self.registers.LEV[index].read() & (1 << shift) != 0
    }
}

impl Gpio<Alt> {
    pub fn pull(&mut self, value: Pud) {
        self.set_pud(value);
    }
}
