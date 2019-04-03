use std::fmt::{Debug, Display};
use std::ops::Deref;

pub struct Error<E: Display + Debug> {
    pub error: E,
    pub trace: Vec<(&'static str, u32)>,
}

impl<E: Display + Debug> Deref for Error<E> {
    type Target = E;
    fn deref(&self) -> &E {
        &self.error
    }
}

impl<E: Display + Debug> Display for Error<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "{} from: ", self.error)?;
        for t in self.trace.iter() {
            write!(f, "{}::{} ", t.0, t.1)?;
        }
        Ok(())
    }
}

impl<E: Display + Debug> Debug for Error<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "{:?} from: ", self.error)?;
        for t in self.trace.iter() {
            write!(f, "{}::{} ", t.0, t.1)?;
        }
        Ok(())
    }
}

#[macro_export]
macro_rules! make_err {
    ($err:expr) => {
        Error {
            error: $err,
            trace: vec![(file!(), line!())],
        }
    };
}

#[macro_export]
macro_rules! trace_err {
    ($err:expr) => {{
        let mut e = $err;
        e.trace.push((file!(), line!()));
        e
    }};
}
