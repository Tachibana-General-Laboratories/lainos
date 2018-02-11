use stack_vec::StackVec;
use core::str::from_utf8;

use uart0;
use power;

/// Error type for `Command` parse failures.
#[derive(Debug)]
enum Error {
    Empty,
    TooManyArgs
}

/// A structure representing a single shell command.
struct Command<'a> {
    args: StackVec<'a, &'a str>
}

impl<'a> Command<'a> {
    /// Parse a command from a string `s` using `buf` as storage for the
    /// arguments.
    ///
    /// # Errors
    ///
    /// If `s` contains no arguments, returns `Error::Empty`. If there are more
    /// arguments than `buf` can hold, returns `Error::TooManyArgs`.
    fn parse(s: &'a str, buf: &'a mut [&'a str]) -> Result<Command<'a>, Error> {
        let mut args = StackVec::new(buf);
        for arg in s.split(' ').filter(|a| !a.is_empty()) {
            args.push(arg).map_err(|_| Error::TooManyArgs)?;
        }

        if args.is_empty() {
            return Err(Error::Empty);
        }

        Ok(Command { args })
    }

    /// Returns this command's path. This is equivalent to the first argument.
    fn path(&self) -> &str {
        &self.args[0]
    }
}

/// Starts a shell using `prefix` as the prefix for each line. This function
/// never returns: it is perpetually in a shell loop.
pub fn shell(prefix: &str) -> ! {
    print!("\n{}", prefix);

    let mut buf = [0u8; 512];
    let mut buf = StackVec::new(&mut buf);
    loop {
        match uart0::receive() {
            0 => (),
            b'\r' | b'\n' => {
                print!("\r\n");
                {
                    let s = from_utf8(&buf).unwrap();
                    let mut str_buf = [""; 64];
                    match Command::parse(s, &mut str_buf) {
                        Err(Error::Empty) => (),
                        Err(Error::TooManyArgs) => println!("error: too many arguments"),
                        Ok(cmd) => {
                            run_cmd(cmd);
                            print!("\r\n");
                        }
                    }
                }
                buf.truncate(0);
                print!("{}", prefix);
            }
            127 => (), // DEL
            8 => { // BS
                if !buf.is_empty() {
                    uart0::send(8);
                    uart0::send(32);
                    uart0::send(8);
                    buf.pop();
                }
            }
            c @ 32...126 => {
                if !buf.is_full() {
                    buf.push(c);
                    uart0::send(c);
                }
            }
            _ => uart0::send(7), // send bell
        }
    }
}

fn run_cmd(cmd: Command) {
    match cmd.path() {
        "echo" => {
            for (i, arg) in cmd.args.iter().enumerate() {
                match i {
                    0 => (),
                    1 => print!("{}", arg),
                    _ => print!(" {}", arg),
                }
            }
        }
        "poweroff" => {
            print!("power-off the machine\n");
            power::power_off();
        }
        "reset" => {
            print!("reset the machine\n");
            power::reset();
        }
        _ => print!("unknown command: {}", cmd.path()),
    }
}
