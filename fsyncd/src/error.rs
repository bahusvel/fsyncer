use std::fmt::{Debug, Display};
use std::ops::Deref;

pub struct Error<E: Display + Debug> {
    pub error: E,
    pub trace: Vec<(&'static str, u32)>,
}

pub trait FromError<E> {
    fn from_error(e: E) -> Self;
}

impl<E: Debug + Display> FromError<Error<E>> for Error<E> {
    fn from_error(e: Error<E>) -> Self {
        e
    }
}

impl<E: Debug + Display> FromError<E> for Error<E> {
    fn from_error(e: E) -> Self {
        Error {
            error: e,
            trace: Vec::new(),
        }
    }
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
            write!(f, "{}:{} ", t.0, t.1)?;
        }
        Ok(())
    }
}

macro_rules! trace_err {
    ($e:expr) => {{
        let mut e = Error::from_error($e);
        e.trace.push((file!(), line!()));
        e
    }};
}

#[macro_export]
macro_rules! trace {
    ($res:expr) => {
        match $res {
            Ok(o) => o,
            Err(e) => {
                let mut err = Error::from_error(e);
                err.trace.push((file!(), line!()));
                return Err(err);
            }
        }
    };
}
