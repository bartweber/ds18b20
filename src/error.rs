use one_wire_hal::error::ErrorKind;

#[derive(Debug, Copy, Clone)]
pub enum Error {
    OneWireError,
    FamilyCodeMismatch,
    CrcMismatch,
    Timeout,
    Other,
}


impl<E: one_wire_hal::error::Error> From<E> for Error {
    fn from(value: E) -> Self {
        match value.kind() {
            ErrorKind::FamilyCodeMismatch => Error::FamilyCodeMismatch,
            ErrorKind::CrcMismatch => Error::CrcMismatch,
            _ => Error::OneWireError,
        }
    }
}